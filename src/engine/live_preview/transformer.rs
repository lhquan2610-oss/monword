use super::cancel::CancelToken;
use super::extractor::{classify_paper_label, estimate_paragraph_height_mm, extract_from_selective_view, ExtractedPreviewDoc};
use super::paginator::paginate_document;
use crate::engine::compliance_audit::sha256_util::sha256_hex;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::std_engine::{self, fixer, ir::DocumentIr};
use crate::engine::template_engine::{self, TemplateStore};
use crate::engine::{formatter, header_builder, org_profile, page_toc, vn_cleaner};
use crate::models::{
    CleanerConfig, FormatConfig, OrganizationProfile, OrientationMode, OutputMode, PageTocConfig,
    PaperSize, PreviewBlock, PreviewBuildRequest, PreviewOverrideConfig, PreviewPageModel,
    StdDocScope, StdExecutionRequest, TemplateApplyRequest, TemplateTargetMode,
};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

#[derive(Clone)]
pub struct CachedPreviewBaseline {
    pub path_key: String,
    pub file_len: u64,
    pub modified_ms: u128,
    pub doc_sha256: String,
    pub extracted: ExtractedPreviewDoc,
    pub pages: Vec<PreviewPageModel>,
}

fn baseline_cache() -> &'static Mutex<VecDeque<CachedPreviewBaseline>> {
    static CACHE: OnceLock<Mutex<VecDeque<CachedPreviewBaseline>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::with_capacity(3)))
}

fn file_stamp(path: &Path) -> Result<(u64, u128), String> {
    let meta = fs::metadata(path)
        .map_err(|e| format!("Không đọc được thông tin tệp '{}': {}", path.display(), e))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    Ok((meta.len(), mtime))
}

pub fn load_or_extract_baseline(
    path: &Path,
    cancel: Option<&CancelToken>,
) -> Result<(CachedPreviewBaseline, bool), String> {
    let path_key = path.to_string_lossy().to_ascii_lowercase();
    let (file_len, modified_ms) = file_stamp(path)?;

    if let Ok(guard) = baseline_cache().lock() {
        if let Some(hit) = guard
            .iter()
            .find(|c| c.path_key == path_key && c.file_len == file_len && c.modified_ms == modified_ms)
        {
            return Ok((hit.clone(), true));
        }
    }

    let raw_bytes = fs::read(path)
        .map_err(|e| format!("Không đọc được tệp '{}': {}", path.display(), e))?;
    let doc_sha256 = sha256_hex(&raw_bytes);
    let view = OpenXmlPackage::open_selective_for_analysis(path)?;
    let extracted = extract_from_selective_view(&view, cancel)?;
    let prefer_cached_breaks = extracted
        .blocks
        .iter()
        .any(|b| matches!(b, PreviewBlock::Paragraph(p) if p.has_last_rendered_break));
    let pages = paginate_document(&extracted, prefer_cached_breaks, cancel)?;

    let cached = CachedPreviewBaseline {
        path_key,
        file_len,
        modified_ms,
        doc_sha256,
        extracted,
        pages,
    };

    if let Ok(mut guard) = baseline_cache().lock() {
        guard.retain(|c| c.path_key != cached.path_key);
        if guard.len() >= 2 {
            guard.pop_front();
        }
        guard.push_back(cached.clone());
    }

    Ok((cached, false))
}

pub fn can_use_incremental_patch(req: &PreviewBuildRequest) -> bool {
    if !req.prefer_incremental || !req.include_after {
        return false;
    }
    if req
        .after_file_path
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return false;
    }
    let ov = &req.overrides;
    if ov.apply_vn_cleaner
        || ov.apply_std_group_a
        || ov.template_id.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false)
        || ov.apply_header_mode.is_some()
        || req.process_config.is_some()
    {
        return false;
    }
    ov.enabled
}

pub fn apply_incremental_overrides_to_extracted(
    base: &ExtractedPreviewDoc,
    ov: &PreviewOverrideConfig,
) -> ExtractedPreviewDoc {
    let mut out = base.clone();

    // 1. Update Section geometry (paper size, orientation, margins, swap landscape margins)
    for sect in &mut out.sections {
        if let Some(ps) = ov.paper_size {
            let (pw, ph) = match ps {
                PaperSize::A4 => (210.0, 297.0),
                PaperSize::A3 => (297.0, 420.0),
                PaperSize::Letter => (215.9, 279.4),
            };
            let is_land = match ov.orientation_override.as_deref() {
                Some("allLandscape") => true,
                Some("allPortrait") => false,
                _ => sect.orientation == "landscape",
            };
            sect.width_mm = if is_land { ph } else { pw };
            sect.height_mm = if is_land { pw } else { ph };
            sect.orientation = if is_land { "landscape" } else { "portrait" }.to_string();
            sect.paper_label = classify_paper_label(sect.width_mm, sect.height_mm);
        } else if let Some(ref ori) = ov.orientation_override {
            let want_land = match ori.as_str() {
                "allLandscape" => Some(true),
                "allPortrait" => Some(false),
                _ => None,
            };
            if let Some(is_land) = want_land {
                let min_d = sect.width_mm.min(sect.height_mm);
                let max_d = sect.width_mm.max(sect.height_mm);
                sect.width_mm = if is_land { max_d } else { min_d };
                sect.height_mm = if is_land { min_d } else { max_d };
                sect.orientation = if is_land { "landscape" } else { "portrait" }.to_string();
            }
        }

        let top_base = ov.margin_top_cm.map(|c| c * 10.0).unwrap_or(sect.margin_top_mm);
        let bot_base = ov
            .margin_bottom_cm
            .map(|c| c * 10.0)
            .unwrap_or(sect.margin_bottom_mm);
        let left_base = ov
            .margin_left_cm
            .map(|c| c * 10.0)
            .unwrap_or(sect.margin_left_mm);
        let right_base = ov
            .margin_right_cm
            .map(|c| c * 10.0)
            .unwrap_or(sect.margin_right_mm);

        if sect.orientation == "landscape"
            && ov.auto_swap_binding_margin
            && (ov.margin_top_cm.is_some()
                || ov.margin_left_cm.is_some()
                || ov.margin_bottom_cm.is_some()
                || ov.margin_right_cm.is_some())
        {
            sect.margin_top_mm = left_base;
            sect.margin_bottom_mm = right_base;
            sect.margin_left_mm = bot_base;
            sect.margin_right_mm = top_base;
        } else {
            sect.margin_top_mm = top_base;
            sect.margin_bottom_mm = bot_base;
            sect.margin_left_mm = left_base;
            sect.margin_right_mm = right_base;
        }
    }

    // 2. Patch paragraph & table typography/spacing/alignment + recompute estimated heights
    for blk in &mut out.blocks {
        let blk_sect_idx = match blk {
            PreviewBlock::Paragraph(p) => p.section_index,
            PreviewBlock::Table(t) => t.section_index,
        };
        let sect = out
            .sections
            .iter()
            .find(|s| s.section_index == blk_sect_idx)
            .unwrap_or(&out.sections[0]);
        let printable_w =
            (sect.width_mm - sect.margin_left_mm - sect.margin_right_mm).max(40.0);

        match blk {
            PreviewBlock::Paragraph(p) => {
                let is_protected = matches!(
                    p.role.as_str(),
                    "adminHeader" | "docTypeHeading" | "docSubject" | "signature" | "recipient"
                );
                if let Some(ref f) = ov.font_name {
                    for s in &mut p.spans {
                        if !s.font_family.eq_ignore_ascii_case("Symbol")
                            && !s.font_family.eq_ignore_ascii_case("Wingdings")
                        {
                            s.font_family = f.clone();
                        }
                    }
                }
                if let Some(sz) = ov.font_size_pt {
                    if !is_protected && !p.role.starts_with("heading") {
                        for s in &mut p.spans {
                            s.font_size_pt = sz;
                        }
                    }
                }
                if !is_protected && !p.role.starts_with("heading") && p.role != "empty" {
                    if let Some(ref al) = ov.body_alignment {
                        p.alignment = al.clone();
                    }
                    if let Some(ls) = ov.line_spacing_mult {
                        p.line_spacing_mult = ls;
                        p.exact_line_pt = None;
                    }
                    if let Some(sb) = ov.space_before_pt {
                        p.space_before_pt = sb;
                    }
                    if let Some(sa) = ov.space_after_pt {
                        p.space_after_pt = sa;
                    }
                    if let Some(ind_cm) = ov.first_line_indent_cm {
                        p.first_line_indent_mm = (ind_cm * 10.0 * 10.0).round() / 10.0;
                    }
                }
                for img in &mut p.images {
                    img.overflows_printable_area = img.width_mm > printable_w + 1.5;
                }
                p.estimated_height_mm = estimate_paragraph_height_mm(p, printable_w);
            }
            PreviewBlock::Table(t) => {
                if let Some(ref f) = ov.font_name {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            for p in &mut cell.paragraphs {
                                for s in &mut p.spans {
                                    s.font_family = f.clone();
                                }
                            }
                        }
                    }
                }
                if t.table_role == "dataTable" {
                    if let Some(tsz) = ov.table_font_size_pt {
                        for row in &mut t.rows {
                            for cell in &mut row.cells {
                                for p in &mut cell.paragraphs {
                                    for s in &mut p.spans {
                                        s.font_size_pt = tsz;
                                    }
                                    p.estimated_height_mm =
                                        estimate_paragraph_height_mm(p, (cell.width_mm - 3.5).max(12.0));
                                }
                            }
                        }
                    }
                }
                t.overflows_printable_width = (t.total_width_mm + t.indent_left_mm) > (printable_w + 1.5);
            }
        }
    }

    out
}

pub fn build_transformed_package_in_memory(
    src_path: &Path,
    req: &PreviewBuildRequest,
    explicit_org: Option<&OrganizationProfile>,
    cancel: Option<&CancelToken>,
) -> Result<(OpenXmlPackage, ExtractedPreviewDoc, String), String> {
    let ov = &req.overrides;

    if let Some(tok) = cancel {
        if tok.is_cancelled() {
            return Err("CANCELLED".to_string());
        }
    }

    let resolved_org = ov
        .org_profile_id
        .as_deref()
        .and_then(org_profile::get_org_profile)
        .or_else(|| explicit_org.cloned());

    let mut pkg = if let Some(ref tpl_id) = ov.template_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let store = TemplateStore::default_store();
        let tpl_req = TemplateApplyRequest {
            template_id: tpl_id.to_string(),
            org_profile_id: ov.org_profile_id.clone(),
            org_profile_override: resolved_org.clone(),
            target_mode: TemplateTargetMode::ApplyToExistingDocx,
            target_path: Some(src_path.to_string_lossy().to_string()),
            output_mode: OutputMode::NewCopy,
            insert_framing_on_existing: true,
            ..Default::default()
        };
        let (_, tpl_pkg, _) = template_engine::build_preview_and_package(&store, &tpl_req)?;
        tpl_pkg
    } else {
        OpenXmlPackage::open(src_path)?
    };

    if let Some(tok) = cancel {
        if tok.is_cancelled() {
            return Err("CANCELLED".to_string());
        }
    }

    if let (Some(mode), Some(hdr_cfg)) = (ov.apply_header_mode, ov.header_config.as_ref()) {
        let org = hdr_cfg
            .org_profile_id
            .as_deref()
            .and_then(org_profile::get_org_profile)
            .or_else(|| resolved_org.clone());
        let mut logs = Vec::new();
        let mut warns = Vec::new();
        header_builder::apply_header_to_package(
            &mut pkg,
            hdr_cfg,
            mode,
            org.as_ref(),
            &mut logs,
            &mut warns,
        )?;
    }

    let mut logs = Vec::new();
    if ov.apply_vn_cleaner {
        let cleaner_cfg = req
            .process_config
            .as_ref()
            .map(|c| c.cleaner.clone())
            .unwrap_or_default();
        vn_cleaner::clean_vietnamese_document(&mut pkg, &cleaner_cfg, &mut logs)?;
    }

    if ov.apply_std_group_a {
        let view = pkg.to_selective_view()?;
        let ir = DocumentIr::build(src_path, &view, req.scope, resolved_org.as_ref())?;
        let std_rep = std_engine::evaluate_ir(&ir, resolved_org.as_ref(), &[]);
        let std_req = StdExecutionRequest {
            path: src_path.to_string_lossy().to_string(),
            scope: req.scope,
            output_mode: OutputMode::NewCopy,
            apply_all_group_a: true,
            ..Default::default()
        };
        let _ = fixer::apply_selected_fixes(&mut pkg, &ir, &std_rep, &std_req)?;
    }

    // Apply ProcessConfig + live overrides formatting if configured
    if req.process_config.is_some()
        || (ov.enabled
            && (ov.font_name.is_some()
                || ov.font_size_pt.is_some()
                || ov.line_spacing_mult.is_some()
                || ov.space_before_pt.is_some()
                || ov.space_after_pt.is_some()
                || ov.first_line_indent_cm.is_some()
                || ov.paper_size.is_some()
                || ov.margin_top_cm.is_some()
                || ov.margin_bottom_cm.is_some()
                || ov.margin_left_cm.is_some()
                || ov.margin_right_cm.is_some()
                || ov.orientation_override.is_some()))
    {
        let mut fmt = req
            .process_config
            .as_ref()
            .map(|p| p.format.clone())
            .unwrap_or_else(FormatConfig::default);
        let mut ptoc = req
            .process_config
            .as_ref()
            .map(|p| p.page_toc.clone())
            .unwrap_or_else(PageTocConfig::default);

        if let Some(ref f) = ov.font_name {
            fmt.font_name = f.clone();
        }
        if let Some(sz) = ov.font_size_pt {
            fmt.font_size_pt = sz;
        }
        if let Some(tsz) = ov.table_font_size_pt {
            fmt.table_font_size_pt = tsz;
        }
        if let Some(ls) = ov.line_spacing_mult {
            fmt.line_spacing = ls;
        }
        if let Some(sb) = ov.space_before_pt {
            fmt.space_before_pt = sb;
        }
        if let Some(sa) = ov.space_after_pt {
            fmt.space_after_pt = sa;
        }
        if let Some(ind) = ov.first_line_indent_cm {
            fmt.first_line_indent_cm = ind;
        }
        if let Some(ps) = ov.paper_size {
            fmt.paper_size = ps;
        }
        if let Some(mt) = ov.margin_top_cm {
            fmt.margin_top_cm = mt;
        }
        if let Some(mb) = ov.margin_bottom_cm {
            fmt.margin_bottom_cm = mb;
        }
        if let Some(ml) = ov.margin_left_cm {
            fmt.margin_left_cm = ml;
        }
        if let Some(mr) = ov.margin_right_cm {
            fmt.margin_right_cm = mr;
        }
        if let Some(ref ori) = ov.orientation_override {
            ptoc.orientation_mode = match ori.as_str() {
                "allPortrait" => OrientationMode::AllPortrait,
                "allLandscape" => OrientationMode::AllLandscape,
                _ => OrientationMode::Keep,
            };
        }
        ptoc.auto_swap_binding_margin = ov.auto_swap_binding_margin;

        formatter::apply_formatting(&mut pkg, &fmt, &mut logs)?;
        page_toc::apply_page_and_toc(&mut pkg, &ptoc, &fmt, &mut logs)?;
    }

    let bytes = pkg.to_bytes()?;
    let sha_after = sha256_hex(&bytes);
    let after_view = pkg.to_selective_view()?;
    let mut extracted = extract_from_selective_view(&after_view, cancel)?;
    if ov.enabled && ov.body_alignment.is_some() {
        let mut align_only = PreviewOverrideConfig::default();
        align_only.enabled = true;
        align_only.body_alignment = ov.body_alignment.clone();
        extracted = apply_incremental_overrides_to_extracted(&extracted, &align_only);
    }

    Ok((pkg, extracted, sha_after))
}

#[allow(dead_code)]
fn _ensure_cleaner_type_linked(_: CleanerConfig, _: StdDocScope) {}
