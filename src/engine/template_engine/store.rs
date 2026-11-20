use crate::engine::openxml_arch::{OpenXmlPackage, SelectivePackageView};
use crate::engine::std_engine::ir::DocumentIr;
use crate::models::{
    DocumentTemplate, ImportDocxTemplateRequest, SaveTemplateRequest, StdDocScope,
    TemplateBlockKind, TemplateCatalogResponse, TemplateGroup, TemplateSectionBlock,
    TemplateTableBlueprint, TemplateTableColumn, TemplateUsageStatus, TemplateVersionRecord,
};
use chrono::Local;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const BUILTIN_TEMPLATES_JSON: &str = include_str!("../../../ui/src/data/builtinTemplates.json");

#[derive(Debug, Clone)]
pub struct TemplateStore {
    pub root_dir: PathBuf,
}

impl TemplateStore {
    pub fn default_store() -> Self {
        let base = env::var("APPDATA")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                env::var("LOCALAPPDATA")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        Self {
            root_dir: base.join("MonWord").join("templates"),
        }
    }

    #[allow(dead_code)]
    pub fn with_root(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn dropin_dir(&self) -> PathBuf {
        self.root_dir.join("dropin")
    }

    fn user_templates_path(&self) -> PathBuf {
        self.root_dir.join("user_templates.json")
    }

    fn system_overrides_path(&self) -> PathBuf {
        self.root_dir.join("system_overrides.json")
    }

    fn defaults_path(&self) -> PathBuf {
        self.root_dir.join("defaults.json")
    }

    fn versions_dir(&self) -> PathBuf {
        self.root_dir.join("versions")
    }

    fn version_file_path(&self, template_id: &str) -> PathBuf {
        self.versions_dir()
            .join(format!("{}.json", sanitize_id_for_fs(template_id)))
    }

    pub fn docx_assets_dir(&self) -> PathBuf {
        self.root_dir.join("docx_assets")
    }

    pub fn asset_file_path(&self, asset_id: &str) -> PathBuf {
        self.docx_assets_dir()
            .join(format!("{}.docx", sanitize_id_for_fs(asset_id)))
    }

    pub fn load_builtin_templates() -> Vec<DocumentTemplate> {
        let cleaned = BUILTIN_TEMPLATES_JSON.trim_start_matches('\u{feff}');
        serde_json::from_str::<Vec<DocumentTemplate>>(cleaned).unwrap_or_default()
    }

    fn load_dropin_templates(&self) -> Vec<DocumentTemplate> {
        let dir = self.dropin_dir();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
            })
            .collect();
        paths.sort();

        for path in paths {
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let cleaned = raw.trim_start_matches('\u{feff}');
            if let Ok(mut list) = serde_json::from_str::<Vec<DocumentTemplate>>(cleaned) {
                for tpl in &mut list {
                    ensure_template_defaults(tpl);
                }
                out.extend(list);
            } else if let Ok(mut single) = serde_json::from_str::<DocumentTemplate>(cleaned) {
                ensure_template_defaults(&mut single);
                out.push(single);
            }
        }
        out
    }

    fn load_json_list<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
        let Ok(raw) = fs::read_to_string(path) else {
            return Vec::new();
        };
        let cleaned = raw.trim_start_matches('\u{feff}');
        serde_json::from_str::<Vec<T>>(cleaned).unwrap_or_default()
    }

    fn load_defaults_map(&self) -> BTreeMap<String, String> {
        let Ok(raw) = fs::read_to_string(self.defaults_path()) else {
            return BTreeMap::new();
        };
        let cleaned = raw.trim_start_matches('\u{feff}');
        serde_json::from_str::<BTreeMap<String, String>>(cleaned).unwrap_or_default()
    }

    fn save_json_atomic<T: serde::Serialize>(&self, path: &Path, value: &T) -> Result<(), String> {
        crate::engine::reliability::safe_save::save_json_atomic(path, value)?;
        Ok(())
    }

    pub fn list_catalog(&self) -> Result<TemplateCatalogResponse, String> {
        let _ = fs::create_dir_all(self.dropin_dir());
        let builtins = Self::load_builtin_templates();
        let overrides: Vec<DocumentTemplate> = Self::load_json_list(&self.system_overrides_path());
        let user_list: Vec<DocumentTemplate> = Self::load_json_list(&self.user_templates_path());
        let dropins = self.load_dropin_templates();
        let mut defaults = self.load_defaults_map();

        let mut merged: Vec<DocumentTemplate> = Vec::new();
        let mut seen_ids = BTreeSet::new();

        for mut sys_tpl in builtins {
            if let Some(ov) = overrides
                .iter()
                .find(|o| o.template_id == sys_tpl.template_id)
            {
                let mut cloned = ov.clone();
                cloned.is_system = true;
                sys_tpl = cloned;
            }
            if seen_ids.insert(sys_tpl.template_id.clone()) {
                merged.push(sys_tpl);
            }
        }

        for mut tpl in user_list {
            tpl.is_system = false;
            ensure_template_defaults(&mut tpl);
            if seen_ids.insert(tpl.template_id.clone()) {
                merged.push(tpl);
            }
        }

        for mut tpl in dropins {
            ensure_template_defaults(&mut tpl);
            if seen_ids.insert(tpl.template_id.clone()) {
                merged.push(tpl);
            }
        }

        for tpl in &merged {
            let k = default_key(tpl.group, &tpl.doc_type);
            if tpl.is_default_for_doc_type && !defaults.contains_key(&k) {
                defaults.insert(k, tpl.template_id.clone());
            }
        }

        for tpl in &mut merged {
            let k = default_key(tpl.group, &tpl.doc_type);
            if let Some(def_id) = defaults.get(&k) {
                tpl.is_default_for_doc_type = def_id == &tpl.template_id;
            }
            if tpl.detected_placeholders.is_empty() {
                tpl.detected_placeholders = extract_placeholders_from_template(tpl);
            }
        }

        Ok(TemplateCatalogResponse {
            templates: merged,
            custom_dropin_dir: self.dropin_dir().to_string_lossy().to_string(),
            defaults,
        })
    }

    pub fn get_template(&self, template_id: &str) -> Result<DocumentTemplate, String> {
        let catalog = self.list_catalog()?;
        catalog
            .templates
            .into_iter()
            .find(|t| t.template_id == template_id.trim())
            .ok_or_else(|| format!("Không tìm thấy mẫu văn bản với mã '{}'.", template_id))
    }

    pub fn list_versions(&self, template_id: &str) -> Result<Vec<TemplateVersionRecord>, String> {
        let path = self.version_file_path(template_id);
        let mut records: Vec<TemplateVersionRecord> = Self::load_json_list(&path);
        if records.is_empty() {
            if let Some(builtin) = Self::load_builtin_templates()
                .into_iter()
                .find(|t| t.template_id == template_id.trim())
            {
                records.push(TemplateVersionRecord {
                    version: builtin.version.clone(),
                    saved_at: builtin.updated_at.clone(),
                    change_note: "Phiên bản gốc mặc định của hệ thống".to_string(),
                    has_docx_asset: builtin.source_docx_asset_id.is_some(),
                    docx_asset_id: builtin.source_docx_asset_id.clone(),
                    snapshot: builtin,
                });
            }
        }
        Ok(records)
    }

    fn append_version_record(
        &self,
        template_id: &str,
        record: TemplateVersionRecord,
    ) -> Result<(), String> {
        let mut list = self.list_versions(template_id)?;
        if let Some(existing) = list.iter_mut().find(|r| r.version == record.version) {
            *existing = record;
        } else {
            list.push(record);
        }
        let path = self.version_file_path(template_id);
        self.save_json_atomic(&path, &list)
    }

    pub fn save_template(
        &self,
        mut req: SaveTemplateRequest,
    ) -> Result<TemplateCatalogResponse, String> {
        let name = req.template.name.trim().to_string();
        if name.is_empty() {
            return Err("Tên mẫu văn bản không được để trống.".to_string());
        }
        let doc_type = req.template.doc_type.trim().to_string();
        if doc_type.is_empty() {
            return Err("Loại văn bản (docType) không được để trống.".to_string());
        }
        req.template.name = name;
        req.template.doc_type = doc_type;
        ensure_template_defaults(&mut req.template);

        let now_str = Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
        let builtins = Self::load_builtin_templates();
        let builtin_opt = builtins
            .iter()
            .find(|b| b.template_id == req.template.template_id)
            .cloned();

        let is_system_target = builtin_opt.is_some() || req.template.is_system;

        if is_system_target {
            if builtin_opt.is_none() {
                return Err(
                    "Không thể đánh dấu mẫu người dùng là mẫu hệ thống built-in.".to_string(),
                );
            }
            if !req.confirm_system_override {
                return Err(
                    "Không cho phép ghi đè mẫu hệ thống khi chưa có xác nhận. Hãy chọn \"Sao chép mẫu\" hoặc xác nhận ghi đè mẫu hệ thống."
                        .to_string(),
                );
            }
            let current = self.get_template(&req.template.template_id)?;
            // Seed current state into version history before advancing
            if let Some(pristine) = builtin_opt {
                let mut versions = self.list_versions(&pristine.template_id)?;
                if versions.is_empty() {
                    versions.push(TemplateVersionRecord {
                        version: pristine.version.clone(),
                        saved_at: pristine.updated_at.clone(),
                        change_note: "Phiên bản gốc mặc định của hệ thống".to_string(),
                        has_docx_asset: pristine.source_docx_asset_id.is_some(),
                        docx_asset_id: pristine.source_docx_asset_id.clone(),
                        snapshot: pristine,
                    });
                    self.save_json_atomic(&self.version_file_path(&req.template.template_id), &versions)?;
                }
            }
            let _ = self.append_version_record(
                &current.template_id,
                TemplateVersionRecord {
                    version: current.version.clone(),
                    saved_at: if current.updated_at.is_empty() {
                        now_str.clone()
                    } else {
                        current.updated_at.clone()
                    },
                    change_note: "Bản trước khi ghi đè mẫu hệ thống".to_string(),
                    has_docx_asset: current.source_docx_asset_id.is_some(),
                    docx_asset_id: current.source_docx_asset_id.clone(),
                    snapshot: current.clone(),
                },
            );

            let next_ver = bump_semver_patch(&current.version);
            req.template.is_system = true;
            req.template.version = next_ver.clone();
            req.template.updated_at = now_str.clone();
            req.template.detected_placeholders = extract_placeholders_from_template(&req.template);

            let mut overrides: Vec<DocumentTemplate> =
                Self::load_json_list(&self.system_overrides_path());
            if let Some(slot) = overrides
                .iter_mut()
                .find(|o| o.template_id == req.template.template_id)
            {
                *slot = req.template.clone();
            } else {
                overrides.push(req.template.clone());
            }
            self.save_json_atomic(&self.system_overrides_path(), &overrides)?;

            let note = if req.change_note.trim().is_empty() {
                "Cập nhật mẫu hệ thống (đã xác nhận)".to_string()
            } else {
                req.change_note.trim().to_string()
            };
            let saved_id = req.template.template_id.clone();
            self.append_version_record(
                &saved_id,
                TemplateVersionRecord {
                    version: next_ver,
                    saved_at: now_str,
                    change_note: note,
                    has_docx_asset: req.template.source_docx_asset_id.is_some(),
                    docx_asset_id: req.template.source_docx_asset_id.clone(),
                    snapshot: req.template,
                },
            )?;
            return self.list_catalog();
        }

        // User template (create or update)
        req.template.is_system = false;
        let mut user_list: Vec<DocumentTemplate> = Self::load_json_list(&self.user_templates_path());
        if req.template.template_id.trim().is_empty() {
            req.template.template_id = format!(
                "usr-{}-{}",
                slugify(&req.template.doc_type),
                Local::now().timestamp_millis()
            );
        }

        if let Some(existing_idx) = user_list
            .iter()
            .position(|u| u.template_id == req.template.template_id)
        {
            let prev = user_list[existing_idx].clone();
            self.append_version_record(
                &prev.template_id,
                TemplateVersionRecord {
                    version: prev.version.clone(),
                    saved_at: if prev.updated_at.is_empty() {
                        now_str.clone()
                    } else {
                        prev.updated_at.clone()
                    },
                    change_note: "Phiên bản trước đó".to_string(),
                    has_docx_asset: prev.source_docx_asset_id.is_some(),
                    docx_asset_id: prev.source_docx_asset_id.clone(),
                    snapshot: prev.clone(),
                },
            )?;

            let next_ver = bump_semver_patch(&prev.version);
            req.template.version = next_ver.clone();
            req.template.updated_at = now_str.clone();
            req.template.detected_placeholders = extract_placeholders_from_template(&req.template);
            user_list[existing_idx] = req.template.clone();
            self.save_json_atomic(&self.user_templates_path(), &user_list)?;

            let note = if req.change_note.trim().is_empty() {
                format!("Cập nhật mẫu lên {}", next_ver)
            } else {
                req.change_note.trim().to_string()
            };
            let saved_id = req.template.template_id.clone();
            self.append_version_record(
                &saved_id,
                TemplateVersionRecord {
                    version: next_ver,
                    saved_at: now_str,
                    change_note: note,
                    has_docx_asset: req.template.source_docx_asset_id.is_some(),
                    docx_asset_id: req.template.source_docx_asset_id.clone(),
                    snapshot: req.template,
                },
            )?;
        } else {
            if req.template.version.trim().is_empty() {
                req.template.version = "1.0.0".to_string();
            }
            req.template.updated_at = now_str.clone();
            req.template.detected_placeholders = extract_placeholders_from_template(&req.template);
            user_list.push(req.template.clone());
            self.save_json_atomic(&self.user_templates_path(), &user_list)?;

            let note = if req.change_note.trim().is_empty() {
                "Tạo mẫu mới".to_string()
            } else {
                req.change_note.trim().to_string()
            };
            let saved_id = req.template.template_id.clone();
            self.append_version_record(
                &saved_id,
                TemplateVersionRecord {
                    version: req.template.version.clone(),
                    saved_at: now_str,
                    change_note: note,
                    has_docx_asset: req.template.source_docx_asset_id.is_some(),
                    docx_asset_id: req.template.source_docx_asset_id.clone(),
                    snapshot: req.template,
                },
            )?;
        }

        self.list_catalog()
    }

    pub fn clone_template(
        &self,
        source_id: &str,
        new_name: Option<String>,
    ) -> Result<(DocumentTemplate, TemplateCatalogResponse), String> {
        let mut cloned = self.get_template(source_id)?;
        let now_ms = Local::now().timestamp_millis();
        let new_id = format!("usr-{}-{}", slugify(&cloned.doc_type), now_ms);

        if let Some(old_asset_id) = cloned.source_docx_asset_id.take() {
            let old_path = self.asset_file_path(&old_asset_id);
            if old_path.is_file() {
                let new_asset_id = format!("asset_{}", new_id);
                let new_asset_path = self.asset_file_path(&new_asset_id);
                if let Some(parent) = new_asset_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::copy(&old_path, &new_asset_path)
                    .map_err(|e| format!("Không thể sao chép tệp DOCX gốc của mẫu: {}", e))?;
                cloned.source_docx_asset_id = Some(new_asset_id);
            }
        }

        cloned.template_id = new_id.clone();
        cloned.name = match new_name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => format!("{} (Bản sao)", cloned.name),
        };
        cloned.is_system = false;
        cloned.is_default_for_doc_type = false;
        cloned.version = "1.0.0".to_string();
        cloned.status = TemplateUsageStatus::Active;

        let catalog = self.save_template(SaveTemplateRequest {
            template: cloned.clone(),
            confirm_system_override: false,
            change_note: format!("Sao chép từ mẫu {}", source_id),
        })?;
        let saved = catalog
            .templates
            .iter()
            .find(|t| t.template_id == new_id)
            .cloned()
            .unwrap_or(cloned);
        Ok((saved, catalog))
    }

    pub fn delete_template(&self, template_id: &str) -> Result<TemplateCatalogResponse, String> {
        let target_id = template_id.trim();
        let builtins = Self::load_builtin_templates();
        if builtins.iter().any(|b| b.template_id == target_id) {
            return Err(
                "Không được phép xóa mẫu mặc định của hệ thống (có thể lưu trữ hoặc khôi phục phiên bản gốc)."
                    .to_string(),
            );
        }

        let mut user_list: Vec<DocumentTemplate> = Self::load_json_list(&self.user_templates_path());
        let Some(pos) = user_list.iter().position(|u| u.template_id == target_id) else {
            return Err("Không tìm thấy mẫu do người dùng tạo cần xóa.".to_string());
        };
        let removed = user_list.remove(pos);
        self.save_json_atomic(&self.user_templates_path(), &user_list)?;

        if let Some(asset_id) = removed.source_docx_asset_id {
            let asset = self.asset_file_path(&asset_id);
            let _ = fs::remove_file(asset);
        }
        let ver_path = self.version_file_path(target_id);
        let _ = fs::remove_file(ver_path);

        let mut defaults = self.load_defaults_map();
        defaults.retain(|_, v| v != target_id);
        let _ = self.save_json_atomic(&self.defaults_path(), &defaults);

        self.list_catalog()
    }

    pub fn set_default_template(&self, template_id: &str) -> Result<TemplateCatalogResponse, String> {
        let tpl = self.get_template(template_id)?;
        let mut defaults = self.load_defaults_map();
        defaults.insert(default_key(tpl.group, &tpl.doc_type), tpl.template_id);
        self.save_json_atomic(&self.defaults_path(), &defaults)?;
        self.list_catalog()
    }

    pub fn restore_version(
        &self,
        template_id: &str,
        target_version: &str,
    ) -> Result<(DocumentTemplate, TemplateCatalogResponse), String> {
        let target_id = template_id.trim();
        let versions = self.list_versions(target_id)?;
        let rec = versions
            .into_iter()
            .find(|r| r.version == target_version.trim())
            .ok_or_else(|| {
                format!(
                    "Không tìm thấy phiên bản '{}' của mẫu '{}'.",
                    target_version, target_id
                )
            })?;

        let builtins = Self::load_builtin_templates();
        if builtins.iter().any(|b| b.template_id == target_id) {
            if rec.version == "1.0.0" {
                let mut overrides: Vec<DocumentTemplate> =
                    Self::load_json_list(&self.system_overrides_path());
                overrides.retain(|o| o.template_id != target_id);
                self.save_json_atomic(&self.system_overrides_path(), &overrides)?;
            } else {
                let mut overrides: Vec<DocumentTemplate> =
                    Self::load_json_list(&self.system_overrides_path());
                let mut snap = rec.snapshot.clone();
                snap.is_system = true;
                snap.updated_at = Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
                if let Some(slot) = overrides.iter_mut().find(|o| o.template_id == target_id) {
                    *slot = snap;
                } else {
                    overrides.push(snap);
                }
                self.save_json_atomic(&self.system_overrides_path(), &overrides)?;
            }
            let cat = self.list_catalog()?;
            let active = cat
                .templates
                .iter()
                .find(|t| t.template_id == target_id)
                .cloned()
                .unwrap_or(rec.snapshot);
            return Ok((active, cat));
        }

        let mut user_list: Vec<DocumentTemplate> = Self::load_json_list(&self.user_templates_path());
        let mut snap = rec.snapshot.clone();
        snap.is_system = false;
        snap.updated_at = Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
        if let Some(slot) = user_list.iter_mut().find(|u| u.template_id == target_id) {
            *slot = snap.clone();
        } else {
            user_list.push(snap.clone());
        }
        self.save_json_atomic(&self.user_templates_path(), &user_list)?;
        let cat = self.list_catalog()?;
        Ok((snap, cat))
    }

    pub fn import_docx_template(
        &self,
        req: ImportDocxTemplateRequest,
    ) -> Result<(DocumentTemplate, TemplateCatalogResponse), String> {
        let src = Path::new(&req.source_path);
        if !src.is_file() {
            return Err(format!("Không tìm thấy tệp DOCX mẫu: {}", src.display()));
        }
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "docx" && ext != "docm" {
            return Err("Chỉ hỗ trợ nhập mẫu từ tệp Word OpenXML (.docx hoặc .docm).".to_string());
        }

        // Validate full OpenXML archive structure without stripping binary assets
        let pkg = OpenXmlPackage::open(src)?;
        if !pkg.is_word() {
            return Err("Tệp không chứa phần nội dung Word hợp lệ (word/document.xml).".to_string());
        }
        let view = pkg.to_selective_view()?;
        let doc_ir = DocumentIr::build(src, &view, req.scope, None)?;

        let detected_placeholders = extract_placeholders_from_selective_view(&view);
        let now_ms = Local::now().timestamp_millis();
        let slug = slugify(if req.doc_type.trim().is_empty() {
            "customDocx"
        } else {
            &req.doc_type
        });
        let template_id = format!("usr-docx-{}-{}", slug, now_ms);
        let asset_id = format!("docx_{}", template_id);
        let asset_dest = self.asset_file_path(&asset_id);
        if let Some(parent) = asset_dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Không tạo được thư mục tài nguyên DOCX: {}", e))?;
        }
        let _ = crate::engine::reliability::safe_save::save_openxml_verified(
            &pkg,
            &asset_dest,
            None,
            &crate::engine::reliability::preservation::PreservationPolicy::default(),
        )?;

        let mut summary_blocks = Vec::new();
        summary_blocks.push(TemplateSectionBlock {
            section_id: "docx-imported-body".to_string(),
            title: "Cấu trúc DOCX gốc được bảo toàn nguyên vẹn".to_string(),
            kind: TemplateBlockKind::MetadataBlock,
            required: true,
            enabled_by_default: true,
            heading_alignment: "left".to_string(),
            heading_bold: true,
            heading_italic: false,
            heading_uppercase: false,
            body_paragraphs: vec![format!(
                "Mẫu giữ nguyên {} đoạn, {} bảng biểu, {} hình ảnh/logo và {} section từ tệp gốc {}.",
                doc_ir.paragraphs.len(),
                doc_ir.tables.len(),
                doc_ir.assets.media_count.max(doc_ir.assets.drawing_count),
                doc_ir.sections.len(),
                src.file_name().and_then(|n| n.to_str()).unwrap_or("template.docx")
            )],
            inline_table: if !doc_ir.tables.is_empty() {
                Some(TemplateTableBlueprint {
                    caption: format!("Bao gồm {} bảng nguyên bản trong DOCX", doc_ir.tables.len()),
                    columns: vec![TemplateTableColumn {
                        header: "Bảng OpenXML gốc được giữ nguyên".to_string(),
                        width_pct: 100,
                        align: "left".to_string(),
                    }],
                    sample_rows: vec![],
                })
            } else {
                None
            },
        });

        let tpl_name = if req.name.trim().is_empty() {
            src.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Mẫu DOCX nhập ngoài")
                .to_string()
        } else {
            req.name.trim().to_string()
        };

        let mut tpl = DocumentTemplate {
            template_id: template_id.clone(),
            name: tpl_name,
            group: if matches!(req.group, TemplateGroup::Administrative | TemplateGroup::Enterprise)
            {
                req.group
            } else {
                TemplateGroup::Custom
            },
            doc_type: if req.doc_type.trim().is_empty() {
                "customDocx".to_string()
            } else {
                req.doc_type.trim().to_string()
            },
            scope: if matches!(req.scope, StdDocScope::AutoDetect) {
                StdDocScope::AdminNd30
            } else {
                req.scope
            },
            content_structure: summary_blocks,
            version: "1.0.0".to_string(),
            status: TemplateUsageStatus::Active,
            description: format!(
                "Mẫu nhập trực tiếp từ tệp DOCX ({} trường động phát hiện).",
                detected_placeholders.len()
            ),
            is_system: false,
            is_default_for_doc_type: false,
            source_docx_asset_id: Some(asset_id),
            detected_placeholders,
            ..Default::default()
        };

        if let Some(first_sec) = doc_ir.sections.first() {
            tpl.format_rules.margin_top_mm = first_sec.summary.margin_top_mm;
            tpl.format_rules.margin_bottom_mm = first_sec.summary.margin_bottom_mm;
            tpl.format_rules.margin_left_mm = first_sec.summary.margin_left_mm;
            tpl.format_rules.margin_right_mm = first_sec.summary.margin_right_mm;
        }

        let catalog = self.save_template(SaveTemplateRequest {
            template: tpl.clone(),
            confirm_system_override: false,
            change_note: format!(
                "Nhập mẫu DOCX từ {}",
                src.file_name().and_then(|n| n.to_str()).unwrap_or("DOCX")
            ),
        })?;
        let saved = catalog
            .templates
            .iter()
            .find(|t| t.template_id == template_id)
            .cloned()
            .unwrap_or(tpl);
        Ok((saved, catalog))
    }

    pub fn open_template_master_package(&self, tpl: &DocumentTemplate) -> Result<Option<OpenXmlPackage>, String> {
        let Some(asset_id) = &tpl.source_docx_asset_id else {
            return Ok(None);
        };
        let path = self.asset_file_path(asset_id);
        if !path.is_file() {
            return Err(format!(
                "Thiếu tệp tài nguyên DOCX gốc '{}' của mẫu '{}'.",
                path.display(),
                tpl.name
            ));
        }
        Ok(Some(OpenXmlPackage::open(&path)?))
    }
}

pub fn default_key(group: TemplateGroup, doc_type: &str) -> String {
    let g = match group {
        TemplateGroup::Administrative => "administrative",
        TemplateGroup::Enterprise => "enterprise",
        TemplateGroup::Party => "party",
        TemplateGroup::Academic => "academic",
        TemplateGroup::Custom => "custom",
    };
    format!("{}:{}", g, doc_type.trim())
}

fn ensure_template_defaults(tpl: &mut DocumentTemplate) {
    if tpl.version.trim().is_empty() {
        tpl.version = "1.0.0".to_string();
    }
    if matches!(tpl.scope, StdDocScope::AutoDetect | StdDocScope::UnknownRequiresSelection) {
        tpl.scope = match tpl.group {
            TemplateGroup::Enterprise => StdDocScope::EnterpriseInternal,
            TemplateGroup::Party => StdDocScope::PartyDocument,
            TemplateGroup::Academic => StdDocScope::AcademicThesis,
            TemplateGroup::Administrative | TemplateGroup::Custom => StdDocScope::AdminNd30,
        };
    }
}

fn sanitize_id_for_fs(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn slugify(s: &str) -> String {
    let raw: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .take(24)
        .collect();
    if raw.is_empty() {
        "custom".to_string()
    } else {
        raw
    }
}

fn bump_semver_patch(ver: &str) -> String {
    let parts: Vec<u32> = ver
        .trim()
        .trim_start_matches(['v', 'V'])
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect();
    match parts.as_slice() {
        [maj, min, pat, ..] => format!("{}.{}.{}", maj, min, pat + 1),
        [maj, min] => format!("{}.{}.1", maj, min),
        [maj] => format!("{}.0.1", maj),
        _ => "1.0.1".to_string(),
    }
}

fn placeholder_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*([a-zA-Z0-9_]+)\s*\}\}").expect("valid placeholder regex"))
}

pub fn collect_placeholders_in_str(text: &str, out: &mut BTreeSet<String>) {
    for cap in placeholder_regex().captures_iter(text) {
        if let Some(m) = cap.get(1) {
            out.insert(m.as_str().trim().to_string());
        }
    }
}

pub fn extract_placeholders_from_template(tpl: &DocumentTemplate) -> Vec<String> {
    let mut set = BTreeSet::new();
    collect_placeholders_in_str(&tpl.signature_config.signer_title_placeholder, &mut set);
    collect_placeholders_in_str(&tpl.signature_config.signer_name_placeholder, &mut set);
    collect_placeholders_in_str(&tpl.footer_config.footer_note_template, &mut set);
    for rec in &tpl.footer_config.default_recipients {
        collect_placeholders_in_str(rec, &mut set);
    }
    for sec in &tpl.content_structure {
        collect_placeholders_in_str(&sec.title, &mut set);
        for p in &sec.body_paragraphs {
            collect_placeholders_in_str(p, &mut set);
        }
        if let Some(tbl) = &sec.inline_table {
            collect_placeholders_in_str(&tbl.caption, &mut set);
            for row in &tbl.sample_rows {
                for cell in row {
                    collect_placeholders_in_str(cell, &mut set);
                }
            }
        }
    }
    for existing in &tpl.detected_placeholders {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            set.insert(trimmed.to_string());
        }
    }
    set.into_iter().collect()
}

pub fn extract_placeholders_from_selective_view(view: &SelectivePackageView) -> Vec<String> {
    let mut set = BTreeSet::new();
    for (part_name, xml) in &view.xml_parts {
        let lower = part_name.to_ascii_lowercase();
        let relevant = lower == "word/document.xml"
            || ((lower.starts_with("word/header") || lower.starts_with("word/footer"))
                && lower.ends_with(".xml"));
        if !relevant {
            continue;
        }
        if let Ok(doc) = crate::engine::xml_helpers::XmlDocument::parse(xml) {
            // Walk paragraphs to detect split-run placeholders across <w:r> boundaries too
            collect_placeholders_recursive(&doc.root, &mut set);
        } else {
            collect_placeholders_in_str(xml, &mut set);
        }
    }
    set.into_iter().collect()
}

fn collect_placeholders_recursive(
    el: &crate::engine::xml_helpers::XmlElement,
    out: &mut BTreeSet<String>,
) {
    if el.name == "w:p" {
        let text = el.extract_text();
        collect_placeholders_in_str(&text, out);
        return;
    }
    for child in el.children_elements() {
        collect_placeholders_recursive(child, out);
    }
}
