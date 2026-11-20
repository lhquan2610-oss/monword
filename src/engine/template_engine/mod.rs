pub mod renderer;
pub mod store;

use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::backup_manager;
use crate::engine::reliability::error_code::{classify_error_message, AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::preservation::{PreservationPolicy, PreservationSnapshot};
use crate::engine::reliability::safe_save;
use crate::engine::std_engine::{self, ir::DocumentIr};
use crate::engine::template_engine::renderer::ResolvedTemplateData;
use crate::engine::{self, org_profile};
use crate::models::{
    AssetInventory, DocumentTemplate, OrganizationProfile, OutputMode, RuleCheckStatus,
    TemplateApplyRequest, TemplateApplyResult, TemplateCompatibilityReport, TemplatePipelineStep,
    TemplatePreviewResult, TemplateTargetMode,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub use store::TemplateStore;

pub fn build_preview_and_package(
    store: &TemplateStore,
    req: &TemplateApplyRequest,
) -> Result<(TemplatePreviewResult, OpenXmlPackage, Vec<String>), String> {
    let mut steps = Vec::with_capacity(10);
    let mut logs = Vec::new();

    // Bước 1: Đọc template
    let tpl = store.get_template(&req.template_id)?;
    steps.push(TemplatePipelineStep {
        step_number: 1,
        step_name: "Đọc cấu trúc mẫu".to_string(),
        detail: format!(
            "Đã nạp mẫu \"{}\" (v{}, mã: {})",
            tpl.name, tpl.version, tpl.template_id
        ),
        completed: true,
    });

    // Bước 2: Đọc Organization Profile
    let org_opt: Option<OrganizationProfile> = if let Some(ref ov) = req.org_profile_override {
        Some(ov.clone())
    } else if let Some(ref oid) = req.org_profile_id {
        if oid.trim().is_empty() {
            None
        } else {
            org_profile::get_org_profile(oid)
        }
    } else {
        None
    };

    let org_detail = match &org_opt {
        Some(o) => format!("Hồ sơ \"{}\" ({})", o.profile_name, o.issuing_org),
        None => "Chưa chọn Hồ sơ Tổ chức — dùng dữ liệu nhập tay / đánh dấu trường thiếu".to_string(),
    };
    steps.push(TemplatePipelineStep {
        step_number: 2,
        step_name: "Đọc Organization Profile".to_string(),
        detail: org_detail,
        completed: true,
    });

    // Bước 3: Xác định loại văn bản
    steps.push(TemplatePipelineStep {
        step_number: 3,
        step_name: "Xác định loại văn bản & phạm vi".to_string(),
        detail: format!(
            "Nhóm {:?} · Loại \"{}\" · Phạm vi {:?}",
            tpl.group, tpl.doc_type, tpl.scope
        ),
        completed: true,
    });

    // Determine working package source & placeholders
    let mut existing_pkg: Option<OpenXmlPackage> = None;
    let mut is_imported_docx_master = false;
    let mut doc_placeholders: Vec<String> = Vec::new();
    let mut baseline_assets = AssetInventory::default();

    if req.target_mode == TemplateTargetMode::ApplyToExistingDocx {
        let target_raw = req
            .target_path
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                "Vui lòng chọn tệp DOCX mục tiêu trước khi áp dụng mẫu vào tài liệu có sẵn."
                    .to_string()
            })?;
        let target_path = Path::new(target_raw);
        if !target_path.is_file() {
            return Err(format!("Không tìm thấy tệp DOCX mục tiêu: {}", target_raw));
        }
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "docx" && ext != "docm" {
            return Err("Chỉ hỗ trợ áp dụng mẫu bảo toàn cấu trúc cho tệp .docx / .docm.".to_string());
        }
        let pkg = OpenXmlPackage::open(target_path)?;
        let view = pkg.to_selective_view()?;
        doc_placeholders = store::extract_placeholders_from_selective_view(&view);
        if let Ok(ir) = DocumentIr::build(target_path, &view, tpl.scope, org_opt.as_ref()) {
            baseline_assets = ir.assets;
        }
        existing_pkg = Some(pkg);
    } else if let Some(master_pkg) = store.open_template_master_package(&tpl)? {
        is_imported_docx_master = true;
        let view = master_pkg.to_selective_view()?;
        doc_placeholders = store::extract_placeholders_from_selective_view(&view);
        if let Ok(ir) = DocumentIr::build(
            Path::new("master_template.docx"),
            &view,
            tpl.scope,
            org_opt.as_ref(),
        ) {
            baseline_assets = ir.assets;
        }
        existing_pkg = Some(master_pkg);
    }

    let resolved = renderer::resolve_template_data(
        &tpl,
        org_opt.as_ref(),
        &req.dynamic_fields,
        &doc_placeholders,
    );

    // Build / transform package (Steps 4, 5, 6, 7)
    let pkg = match existing_pkg {
        Some(mut target_pkg) => {
            let apply_framing = !is_imported_docx_master;
            renderer::apply_template_to_existing_package(
                &mut target_pkg,
                &tpl,
                req,
                &resolved,
                apply_framing,
                &mut logs,
            )?;
            target_pkg
        }
        None => renderer::build_new_docx_package_from_template(
            &tpl,
            req,
            &resolved,
            false,
            &mut logs,
        )?,
    };

    steps.push(TemplatePipelineStep {
        step_number: 4,
        step_name: "Tạo / chuẩn hóa phần đầu".to_string(),
        detail: if !tpl.header_config.enabled {
            "Bỏ qua phần đầu theo cấu hình mẫu".to_string()
        } else if is_imported_docx_master {
            "Giữ nguyên phần đầu của mẫu DOCX và điền trường động".to_string()
        } else {
            format!(
                "Phần đầu cho \"{}\" · Số: {}",
                resolved.effective_org.issuing_org, resolved.so_va_ky_hieu
            )
        },
        completed: true,
    });

    let active_sections = renderer::resolve_sections(&tpl, req);
    steps.push(TemplatePipelineStep {
        step_number: 5,
        step_name: "Tạo / bảo toàn nội dung".to_string(),
        detail: if req.target_mode == TemplateTargetMode::ApplyToExistingDocx {
            "Bảo toàn nội dung, bảng biểu, hình ảnh, nhiều section trong tệp gốc".to_string()
        } else if is_imported_docx_master {
            "Giữ nguyên cấu trúc nội dung DOCX mẫu & thế biến động".to_string()
        } else {
            format!("{} khối nội dung (bắt buộc + tùy chọn)", active_sections.len())
        },
        completed: true,
    });

    steps.push(TemplatePipelineStep {
        step_number: 6,
        step_name: "Tạo / bảo toàn phần kết thúc".to_string(),
        detail: format!(
            "Chữ ký: {:?} · Nơi nhận: {}",
            tpl.signature_config.layout,
            if tpl.footer_config.show_recipient_list {
                "Hiển thị"
            } else {
                "Ẩn"
            }
        ),
        completed: true,
    });

    steps.push(TemplatePipelineStep {
        step_number: 7,
        step_name: "Áp dụng quy tắc định dạng".to_string(),
        detail: format!(
            "{} · {}pt · Lề T/D/Tr/Ph: {}/{}/{}/{} mm",
            tpl.format_rules.font_family,
            tpl.format_rules.body_font_size_pt,
            tpl.format_rules.margin_top_mm,
            tpl.format_rules.margin_bottom_mm,
            tpl.format_rules.margin_left_mm,
            tpl.format_rules.margin_right_mm
        ),
        completed: true,
    });

    // Bước 8: Kiểm tra tính tương thích với Phase 3 Document Standardization Engine
    let view_after = pkg.to_selective_view()?;
    let virtual_path = req
        .target_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{}.docx", tpl.doc_type)));
    let ir_after = DocumentIr::build(&virtual_path, &view_after, tpl.scope, Some(&resolved.effective_org))?;
    let std_report = std_engine::evaluate_ir(&ir_after, Some(&resolved.effective_org), &[]);

    let mut compat_warnings = Vec::new();
    if !resolved.missing_fields.is_empty() {
        compat_warnings.push(format!(
            "Còn {} trường động chưa nhập dữ liệu — đã đánh dấu rõ [CHƯA NHẬP: ...] trong tài liệu.",
            resolved.missing_fields.len()
        ));
    }
    if req.target_mode == TemplateTargetMode::ApplyToExistingDocx || is_imported_docx_master {
        if ir_after.assets.media_count < baseline_assets.media_count
            || ir_after.assets.drawing_count < baseline_assets.drawing_count
        {
            compat_warnings.push("Cảnh báo: số lượng hình ảnh/logo thay đổi sau xử lý.".to_string());
        }
    }
    if let Ok(layout_rep) = crate::engine::layout_intelligence::analyze_view_and_ir(&view_after, &ir_after) {
        let overflow_count = layout_rep
            .warnings
            .iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    crate::models::LayoutWarningKind::TableOverflowWidth
                        | crate::models::LayoutWarningKind::ImageOverflowPage
                )
            })
            .count();
        if overflow_count > 0 {
            compat_warnings.push(format!(
                "Layout Intelligence: phát hiện {} thành phần tràn lề trang sau áp dụng mẫu (có thể xử lý tại mục Phân tích Bố cục).",
                overflow_count
            ));
        }
    }

    let critical_failures = std_report
        .rule_summaries
        .iter()
        .filter(|r| {
            r.status == RuleCheckStatus::Failed
                && matches!(
                    r.severity,
                    crate::models::StdSeverity::Critical | crate::models::StdSeverity::Error
                )
        })
        .count();

    let compatibility = TemplateCompatibilityReport {
        compatible: critical_failures == 0,
        scope: std_report.effective_scope,
        passed_rules: std_report.status_counts.passed,
        failed_rules: std_report.status_counts.failed,
        needs_review_rules: std_report.status_counts.needs_review,
        missing_fields: resolved.missing_fields.clone(),
        preserved_assets: ir_after.assets.clone(),
        warnings: compat_warnings,
        std_findings: std_report.findings,
    };

    steps.push(TemplatePipelineStep {
        step_number: 8,
        step_name: "Kiểm tra tính tương thích".to_string(),
        detail: format!(
            "{} quy tắc đạt · {} cảnh báo/lỗi · {} trường chưa nhập",
            compatibility.passed_rules,
            compatibility.failed_rules,
            compatibility.missing_fields.len()
        ),
        completed: true,
    });

    // Bước 9: Dựng bản xem trước có cấu trúc
    let preview = build_preview_result(
        &tpl,
        req,
        &resolved,
        compatibility,
        steps,
    );

    Ok((preview, pkg, logs))
}

fn build_preview_result(
    tpl: &DocumentTemplate,
    req: &TemplateApplyRequest,
    resolved: &ResolvedTemplateData,
    compatibility: TemplateCompatibilityReport,
    mut steps: Vec<TemplatePipelineStep>,
) -> TemplatePreviewResult {
    let mut header_left = Vec::new();
    if resolved.header_cfg.show_governing_org && !resolved.effective_org.governing_org.is_empty() {
        header_left.push(resolved.effective_org.governing_org.to_uppercase());
    }
    if tpl.header_config.show_issuing_org {
        header_left.push(resolved.effective_org.issuing_org.to_uppercase());
    }
    if tpl.header_config.show_number_symbol {
        header_left.push(format!("Số: {}", resolved.so_va_ky_hieu));
    }
    let subj_val = resolved
        .values
        .get("trich_yeu")
        .cloned()
        .unwrap_or_default();
    if tpl.header_config.use_left_cell_subject && !subj_val.is_empty() {
        if subj_val.to_lowercase().starts_with("v/v") {
            header_left.push(subj_val.clone());
        } else {
            header_left.push(format!("V/v {}", subj_val));
        }
    }

    let mut header_right = Vec::new();
    if resolved.header_cfg.show_national_header {
        header_right.push("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM".to_string());
        header_right.push("Độc lập - Tự do - Hạnh phúc".to_string());
    }
    if tpl.header_config.show_location_date {
        header_right.push(format!(
            "{}, {}",
            resolved.dia_danh_line.trim_end_matches(','),
            resolved.ngay_thang_line
        ));
    }

    let mut title_lines = Vec::new();
    if tpl.header_config.show_doc_type_and_subject && !tpl.header_config.use_left_cell_subject {
        let dt_lbl = if !tpl.header_config.custom_doc_type_label.trim().is_empty() {
            tpl.header_config.custom_doc_type_label.trim().to_uppercase()
        } else {
            tpl.header_config.base_admin_doc_type.default_label().to_string()
        };
        title_lines.push(dt_lbl);
        if !subj_val.is_empty() {
            title_lines.push(subj_val);
        }
    }

    let body_blocks = renderer::build_preview_blocks(tpl, req, resolved);
    let recipient_lines = if tpl.footer_config.show_recipient_list {
        tpl.footer_config
            .default_recipients
            .iter()
            .map(|r| renderer::render_placeholder_string(r, &resolved.values))
            .collect()
    } else {
        Vec::new()
    };
    let signature_columns = renderer::build_signature_preview_columns(tpl, resolved);

    steps.push(TemplatePipelineStep {
        step_number: 9,
        step_name: "Hiển thị bản xem trước".to_string(),
        detail: "Sẵn sàng xem trước bố cục đầu trang, thân, bảng, chữ ký & thông tin thiếu".to_string(),
        completed: true,
    });
    steps.push(TemplatePipelineStep {
        step_number: 10,
        step_name: "Xuất DOCX hoàn chỉnh".to_string(),
        detail: match req.target_mode {
            TemplateTargetMode::CreateNewDocx => "Tạo mới tệp .docx hoàn chỉnh".to_string(),
            TemplateTargetMode::ApplyToExistingDocx => {
                "Áp dụng vào tệp .docx hiện có và kiểm tra hậu lưu".to_string()
            }
        },
        completed: false,
    });

    TemplatePreviewResult {
        template_id: tpl.template_id.clone(),
        template_name: tpl.name.clone(),
        doc_type: tpl.doc_type.clone(),
        resolved_org_name: resolved.effective_org.issuing_org.clone(),
        header_left_lines: header_left,
        header_right_lines: header_right,
        title_lines,
        body_blocks,
        recipient_lines,
        signature_columns,
        pipeline_steps: steps,
        compatibility,
    }
}

pub fn preview_template(
    store: &TemplateStore,
    req: &TemplateApplyRequest,
) -> Result<TemplatePreviewResult, String> {
    let (preview, _pkg, _logs) = build_preview_and_package(store, req)?;
    Ok(preview)
}

pub fn apply_template(store: &TemplateStore, req: &TemplateApplyRequest) -> TemplateApplyResult {
    let target_for_log = req
        .target_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| req.dest_path.as_deref().filter(|s| !s.trim().is_empty()))
        .map(Path::new);
    let timer = OpTimer::start("template_engine", "apply_template", target_for_log);

    let existing_snapshot = if req.target_mode == TemplateTargetMode::ApplyToExistingDocx {
        req.target_path
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .and_then(|raw| OpenXmlPackage::open(Path::new(raw)).ok())
            .and_then(|orig_pkg| PreservationSnapshot::capture(&orig_pkg).ok())
    } else {
        None
    };

    let (mut preview, pkg, mut logs) = match build_preview_and_package(store, req) {
        Ok(res) => res,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return TemplateApplyResult {
                success: false,
                output_path: String::new(),
                backup_path: None,
                error: Some(msg),
                error_code: Some(code),
                post_save_verified: false,
                output_size_bytes: 0,
                preview: None,
                logs: Vec::new(),
            };
        }
    };

    let mut backup_path: Option<String> = None;
    let output_path: PathBuf = match req.target_mode {
        TemplateTargetMode::CreateNewDocx => {
            if let Some(ref dest) = req.dest_path {
                if !dest.trim().is_empty() {
                    PathBuf::from(dest.trim())
                } else {
                    env::temp_dir().join(format!("{}_MonWord.docx", req.template_id))
                }
            } else {
                env::temp_dir().join(format!("{}_MonWord.docx", req.template_id))
            }
        }
        TemplateTargetMode::ApplyToExistingDocx => {
            let target_raw = req.target_path.as_deref().unwrap_or("");
            let target_p = Path::new(target_raw);
            let resolved_out = if let Some(ref dest) = req.dest_path {
                if !dest.trim().is_empty() {
                    PathBuf::from(dest.trim())
                } else {
                    resolve_template_target_output(target_p, req.output_mode)
                }
            } else {
                resolve_template_target_output(target_p, req.output_mode)
            };

            if req.output_mode == OutputMode::OverwriteWithBackup || resolved_out == target_p {
                match backup_manager::create_versioned_backup(target_p, "template_engine") {
                    Ok(rec) => {
                        backup_path = Some(rec.backup_path);
                    }
                    Err(e) => {
                        let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
                        let code = classified.code.as_str().to_string();
                        let msg = classified.format_user_message();
                        timer.finish_err(&code, &msg);
                        return TemplateApplyResult {
                            success: false,
                            output_path: String::new(),
                            backup_path: None,
                            error: Some(msg),
                            error_code: Some(code),
                            post_save_verified: false,
                            output_size_bytes: 0,
                            preview: Some(preview),
                            logs,
                        };
                    }
                }
            }
            resolved_out
        }
    };

    if let Err(e) = safe_save::save_openxml_verified(
        &pkg,
        &output_path,
        existing_snapshot.as_ref(),
        &PreservationPolicy::default(),
    ) {
        let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
        let code = classified.code.as_str().to_string();
        let msg = classified.format_user_message();
        timer.finish_err(&code, &msg);
        return TemplateApplyResult {
            success: false,
            output_path: output_path.to_string_lossy().to_string(),
            backup_path,
            error: Some(msg),
            error_code: Some(code),
            post_save_verified: false,
            output_size_bytes: 0,
            preview: Some(preview),
            logs,
        };
    }

    // Re-read saved output from disk to guarantee XML & OpenXML validity
    let post_save_res = OpenXmlPackage::open_selective_for_analysis(&output_path);
    let post_save_verified = post_save_res.is_ok();
    let output_size_bytes = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

    if !post_save_verified {
        if let (Some(ref bak), Some(target_raw)) = (&backup_path, req.target_path.as_deref()) {
            let target_p = Path::new(target_raw);
            if output_path == target_p {
                let _ = backup_manager::restore_backup_checked(target_p, Path::new(bak));
            }
        }
    }

    if let Some(step10) = preview.pipeline_steps.iter_mut().find(|s| s.step_number == 10) {
        step10.completed = post_save_verified;
        step10.detail = format!(
            "Đã lưu {} ({} bytes) · Xác minh hậu lưu: {}",
            output_path.display(),
            output_size_bytes,
            if post_save_verified { "ĐẠT" } else { "LỖI" }
        );
    }

    logs.push(format!(
        "Đã xuất tài liệu hoàn chỉnh tới {} ({} bytes).",
        output_path.display(),
        output_size_bytes
    ));

    let (err_opt, code_opt) = match post_save_res {
        Ok(_) => {
            timer.finish_ok(&format!(
                "Áp dụng mẫu '{}' thành công ({} bytes)",
                req.template_id, output_size_bytes
            ));
            (None, None)
        }
        Err(e) => {
            let msg = AppStructuredError::new(
                AppErrorCode::ErrPostSaveVerify,
                format!("Không xác minh được cấu trúc OpenXML sau khi ghi đĩa: {}", e),
            )
            .format_user_message();
            timer.finish_err("ERR_POST_SAVE_VERIFY", &msg);
            (Some(msg), Some("ERR_POST_SAVE_VERIFY".to_string()))
        }
    };

    TemplateApplyResult {
        success: post_save_verified,
        output_path: output_path.to_string_lossy().to_string(),
        backup_path,
        error: err_opt,
        error_code: code_opt,
        post_save_verified,
        output_size_bytes,
        preview: Some(preview),
        logs,
    }
}

pub fn export_docx_template(
    store: &TemplateStore,
    template_id: &str,
    dest_path: &Path,
) -> Result<String, String> {
    let tpl: DocumentTemplate = store.get_template(template_id)?;
    if let Some(master_pkg) = store.open_template_master_package(&tpl)? {
        safe_save::save_openxml_verified(
            &master_pkg,
            dest_path,
            None,
            &PreservationPolicy::default(),
        )?;
        return Ok(dest_path.to_string_lossy().to_string());
    }

    let req = TemplateApplyRequest {
        template_id: tpl.template_id.clone(),
        ..Default::default()
    };
    let synthetic_fields = crate::models::DynamicTemplateFields {
        ten_co_quan: "{{ten_co_quan}}".to_string(),
        co_quan_chu_quan: "{{co_quan_chu_quan}}".to_string(),
        viet_tat_co_quan: "{{viet_tat_co_quan}}".to_string(),
        so_van_ban: "{{so_van_ban}}".to_string(),
        dia_danh: "{{dia_danh}}".to_string(),
        ngay_thang: "{{ngay_thang}}".to_string(),
        nguoi_ky: "{{nguoi_ky}}".to_string(),
        chuc_vu: "{{chuc_vu}}".to_string(),
        nguoi_nhan: "{{nguoi_nhan}}".to_string(),
        trich_yeu: "{{trich_yeu}}".to_string(),
        ..Default::default()
    };
    let resolved = renderer::resolve_template_data(&tpl, None, &synthetic_fields, &[]);
    let mut logs = Vec::new();
    let pkg = renderer::build_new_docx_package_from_template(
        &tpl,
        &req,
        &resolved,
        true,
        &mut logs,
    )?;
    safe_save::save_openxml_verified(&pkg, dest_path, None, &PreservationPolicy::default())?;
    Ok(dest_path.to_string_lossy().to_string())
}

pub fn restore_template_backup(original_path: &Path, backup_path: &Path) -> Result<String, String> {
    if backup_path.as_os_str().is_empty() {
        backup_manager::restore_initial_baseline(original_path)
    } else {
        engine::restore_backup(original_path, backup_path)
    }
}

fn resolve_template_target_output(input_path: &Path, output_mode: OutputMode) -> PathBuf {
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("docx")
        .to_ascii_lowercase();
    let out_ext = if ext == "docm" { "docm" } else { "docx" };
    match output_mode {
        OutputMode::NewCopy => {
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document");
            let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}_TheoMau.{}", stem, out_ext))
        }
        OutputMode::OverwriteWithBackup => input_path.to_path_buf(),
    }
}
