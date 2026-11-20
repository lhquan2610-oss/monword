use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::backup_manager;
use crate::engine::reliability::error_code::{classify_error_message, AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::preservation::{PreservationPolicy, PreservationSnapshot};
use crate::engine::reliability::safe_save;
use crate::engine::vn_cleaner::is_removable_empty_paragraph;
use crate::engine::xml_helpers::{
    XmlDocument, XmlElement, XmlNode, P_PR_ORDER, SECT_PR_ORDER, TBL_PR_ORDER, TC_PR_ORDER,
    TR_PR_ORDER,
};
use crate::engine::{self, system_office};
use crate::models::{
    LayoutExecutionRequest, LayoutExecutionResult, LayoutProposal, LayoutProposalTier,
    OrganizationProfile, OutputMode, RecognizedStructureKind, SmartProposalKind, StdDocScope,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use super::{analyze_view_and_ir, LayoutIrBundle};

pub fn resolve_layout_output_path(input_path: &Path, output_mode: OutputMode) -> PathBuf {
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("docx")
        .to_ascii_lowercase();
    let target_ext = if ext == "doc" { "docx" } else { ext.as_str() };
    match output_mode {
        OutputMode::NewCopy => {
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document");
            let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}_layout.{}", stem, target_ext))
        }
        OutputMode::OverwriteWithBackup => {
            if ext == "doc" {
                input_path.with_extension("docx")
            } else {
                input_path.to_path_buf()
            }
        }
    }
}

pub fn execute_layout_proposals(
    req: &LayoutExecutionRequest,
    org_profile: Option<&OrganizationProfile>,
) -> LayoutExecutionResult {
    let input_path = Path::new(&req.path);
    let timer = OpTimer::start("layout_intelligence", "execute_layout_proposals", Some(input_path));
    let input_str = input_path.to_string_lossy().to_string();
    let mut logs: Vec<String> = Vec::new();

    if !input_path.is_file() {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrFileRead,
            format!("Không tìm thấy tệp văn bản: {}", req.path),
        )
        .format_user_message();
        timer.finish_err("ERR_FILE_READ", &msg);
        return fail_exec_with_code(input_str, msg, "ERR_FILE_READ");
    }

    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut temp_docx: Option<PathBuf> = None;
    let working_path = if ext == "doc" {
        let tmp = env::temp_dir().join(format!(
            "monword_layout_exec_{}_{}.docx",
            std::process::id(),
            chrono::Local::now().timestamp_millis()
        ));
        if let Err(e) = system_office::word_com_convert_doc_to_docx(input_path, &tmp) {
            let classified = classify_error_message(&e, AppErrorCode::ErrDocConvert);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
        temp_docx = Some(tmp.clone());
        tmp
    } else if ext == "docx" || ext == "docm" {
        input_path.to_path_buf()
    } else {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            format!("Định dạng .{} chưa hỗ trợ điều chỉnh bố cục.", ext),
        )
        .format_user_message();
        timer.finish_err("ERR_OPENXML_CORRUPT", &msg);
        return fail_exec_with_code(input_str, msg, "ERR_OPENXML_CORRUPT");
    };

    let mut pkg = match OpenXmlPackage::open(&working_path) {
        Ok(p) => p,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
    };

    let snapshot = match PreservationSnapshot::capture(&pkg) {
        Ok(s) => s,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
    };

    let scope = req.scope.unwrap_or(StdDocScope::AutoDetect);
    let view_before = match pkg.to_selective_view() {
        Ok(v) => v,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
    };
    let bundle = match LayoutIrBundle::from_view(input_path, &view_before, scope, org_profile) {
        Ok(b) => b,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
    };
    let baseline_report = match analyze_view_and_ir(&view_before, &bundle.ir) {
        Ok(r) => r,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
    };

    let doc_xml_raw = match pkg.get_xml("word/document.xml") {
        Some(x) => x,
        None => {
            cleanup_tmp(temp_docx);
            let msg = AppStructuredError::new(
                AppErrorCode::ErrOpenXmlCorrupt,
                "Tệp thiếu word/document.xml.",
            )
            .format_user_message();
            timer.finish_err("ERR_OPENXML_CORRUPT", &msg);
            return fail_exec_with_code(input_str, msg, "ERR_OPENXML_CORRUPT");
        }
    };
    let mut doc = match XmlDocument::parse(&doc_xml_raw) {
        Ok(d) => d,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
    };

    // Baseline snapshots for strict content & relationship preservation verification
    let baseline_texts = collect_all_non_empty_paragraph_texts(&doc.root);
    let baseline_rels = pkg.get_xml("word/_rels/document.xml.rels");
    let baseline_media_entries: Vec<String> = pkg
        .entries
        .iter()
        .filter(|e| e.name.to_ascii_lowercase().starts_with("word/media/"))
        .map(|e| e.name.clone())
        .collect();

    let accepted: HashSet<&str> = req
        .accepted_proposal_ids
        .iter()
        .map(String::as_str)
        .collect();
    let rejected: HashSet<&str> = req
        .rejected_proposal_ids
        .iter()
        .map(String::as_str)
        .collect();

    // Index baseline paragraphs, tables, sections, visuals for deterministic lookup
    let para_lookup: HashMap<&str, (usize, RecognizedStructureKind)> = baseline_report
        .maps
        .paragraph_map
        .iter()
        .filter(|p| p.in_table_id.is_none() && p.in_text_box_id.is_none())
        .filter_map(|p| p.locator.body_child_index.map(|b| (p.para_id.as_str(), (b, p.kind))))
        .collect();
    let tbl_lookup: HashMap<&str, (usize, i32)> = baseline_report
        .maps
        .table_map
        .iter()
        .filter_map(|t| {
            let sec_print_tw = bundle
                .ir
                .sections
                .iter()
                .find(|s| s.summary.section_index == t.locator.section_index)
                .map(|s| s.printable_width_twips)
                .unwrap_or(9355);
            t.locator.body_child_index.map(|b| (t.table_id.as_str(), (b, sec_print_tw)))
        })
        .collect();
    let vis_lookup: HashMap<&str, (usize, Option<String>, f64, f64)> = baseline_report
        .maps
        .visual_objects
        .iter()
        .filter_map(|v| {
            let sec = bundle
                .ir
                .sections
                .iter()
                .find(|s| s.summary.section_index == v.locator.section_index);
            let pw_mm = sec
                .map(|s| super::mapper::twips_to_mm(s.printable_width_twips))
                .unwrap_or(165.0);
            let ph_mm = sec
                .map(|s| {
                    super::mapper::twips_to_mm((s.pg_h_twips - s.top_twips - s.bottom_twips).max(3000))
                })
                .unwrap_or(257.0);
            v.locator
                .body_child_index
                .map(|b| (v.object_id.as_str(), (b, v.relationship_id.clone(), pw_mm, ph_mm)))
        })
        .collect();

    let mut applied_proposal_ids = Vec::new();
    let mut skipped_proposal_ids = Vec::new();
    let mut empty_indices_to_remove: HashSet<usize> = HashSet::new();

    {
        let body = match doc.root.child_mut("w:body") {
            Some(b) => b,
            None => {
                cleanup_tmp(temp_docx);
                let msg = AppStructuredError::new(
                    AppErrorCode::ErrXmlMalformed,
                    "word/document.xml thiếu thẻ w:body.",
                )
                .format_user_message();
                timer.finish_err("ERR_XML_MALFORMED", &msg);
                return fail_exec_with_code(input_str, msg, "ERR_XML_MALFORMED");
            }
        };

        for prop in &baseline_report.proposals {
            if !should_apply_proposal(prop, req.apply_high_confidence_auto, &accepted, &rejected) {
                skipped_proposal_ids.push(prop.proposal_id.clone());
                continue;
            }

            let changed = match prop.kind {
                SmartProposalKind::NormalizeSpacing => {
                    apply_normalize_spacing(body, &prop.target_element_ids, &para_lookup)
                }
                SmartProposalKind::AlignHeadings => {
                    apply_align_headings(body, &prop.target_element_ids, &para_lookup)
                }
                SmartProposalKind::UnifyParagraphFormat => {
                    apply_unify_body_paragraphs(body, &prop.target_element_ids, &para_lookup)
                }
                SmartProposalKind::AdjustTableLayout => {
                    apply_adjust_tables(body, &prop.target_element_ids, &tbl_lookup)
                }
                SmartProposalKind::NormalizeSectionSpacing => {
                    if prop.param_value.as_deref() == Some("collapseStackedEmptyParagraphs") {
                        collect_empty_removals(
                            body,
                            &prop.target_element_ids,
                            &para_lookup,
                            &mut empty_indices_to_remove,
                        )
                    } else {
                        apply_keep_next_lines(body, &prop.target_element_ids, &para_lookup, true)
                    }
                }
                SmartProposalKind::ResolveOverflow => {
                    if let Some(vis_id) = prop
                        .param_value
                        .as_deref()
                        .and_then(|v| v.strip_prefix("fitInlineImage:"))
                    {
                        apply_fit_inline_image(body, vis_id, &vis_lookup)
                    } else {
                        apply_fit_table_widths(body, &prop.target_element_ids, &tbl_lookup)
                    }
                }
                SmartProposalKind::RecommendOrientation => {
                    if let Some(sec_str) = prop
                        .param_value
                        .as_deref()
                        .and_then(|v| v.strip_prefix("landscapeSection:"))
                    {
                        if let Ok(sec_idx) = sec_str.parse::<usize>() {
                            apply_landscape_section(body, sec_idx)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                SmartProposalKind::RebalanceLayout => {
                    if let Some(pid) = prop
                        .param_value
                        .as_deref()
                        .and_then(|v| v.strip_prefix("removePageBreakBefore:"))
                    {
                        apply_remove_page_break_before(body, pid, &para_lookup)
                    } else {
                        apply_keep_next_lines(body, &prop.target_element_ids, &para_lookup, false)
                    }
                }
            };

            if changed {
                applied_proposal_ids.push(prop.proposal_id.clone());
                logs.push(format!("Đã áp dụng [{}]: {}", prop.proposal_id, prop.title));
            } else {
                skipped_proposal_ids.push(prop.proposal_id.clone());
            }
        }

        // Perform safe empty-paragraph removal last so all prior body_child_index lookups stay invariant
        if !empty_indices_to_remove.is_empty() {
            let mut idx_counter = 0usize;
            let mut removed_count = 0usize;
            body.children.retain(|_| {
                let keep = !empty_indices_to_remove.contains(&idx_counter);
                if !keep {
                    removed_count += 1;
                }
                idx_counter += 1;
                keep
            });
            logs.push(format!("Đã thu gọn {} đoạn Enter trống dư thừa.", removed_count));
        }
    }

    // Strict verification BEFORE saving: 0% text loss & 100% rel/media preservation
    let after_texts = collect_all_non_empty_paragraph_texts(&doc.root);
    let content_preserved_verified = after_texts == baseline_texts;
    if !content_preserved_verified {
        cleanup_tmp(temp_docx);
        let msg = AppStructuredError::new(
            AppErrorCode::ErrPostSaveVerify,
            "Hủy ghi tệp: phát hiện sai lệch nội dung văn bản (Smart Formatting bị từ chối để bảo toàn 100% chữ).",
        )
        .format_user_message();
        timer.finish_err("ERR_POST_SAVE_VERIFY", &msg);
        return LayoutExecutionResult {
            success: false,
            output_path: String::new(),
            backup_path: None,
            error: Some(msg),
            error_code: Some("ERR_POST_SAVE_VERIFY".to_string()),
            applied_proposal_ids: Vec::new(),
            skipped_proposal_ids,
            content_preserved_verified: false,
            relationships_preserved_verified: true,
            post_save_report: None,
            logs,
        };
    }

    let serialized_doc = match doc.to_xml_string() {
        Ok(s) => s,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_exec_with_code(input_str, msg, &code);
        }
    };
    pkg.set_xml("word/document.xml", serialized_doc);

    let after_rels = pkg.get_xml("word/_rels/document.xml.rels");
    let after_media: Vec<String> = pkg
        .entries
        .iter()
        .filter(|e| e.name.to_ascii_lowercase().starts_with("word/media/"))
        .map(|e| e.name.clone())
        .collect();
    let relationships_preserved_verified =
        after_rels == baseline_rels && after_media == baseline_media_entries;

    if !relationships_preserved_verified {
        cleanup_tmp(temp_docx);
        let msg = AppStructuredError::new(
            AppErrorCode::ErrPostSaveVerify,
            "Hủy ghi tệp: phát hiện thay đổi ngoài ý muốn ở relationships hoặc word/media/*.",
        )
        .format_user_message();
        timer.finish_err("ERR_POST_SAVE_VERIFY", &msg);
        return LayoutExecutionResult {
            success: false,
            output_path: String::new(),
            backup_path: None,
            error: Some(msg),
            error_code: Some("ERR_POST_SAVE_VERIFY".to_string()),
            applied_proposal_ids: Vec::new(),
            skipped_proposal_ids,
            content_preserved_verified,
            relationships_preserved_verified: false,
            post_save_report: None,
            logs,
        };
    }

    let output_path = resolve_layout_output_path(input_path, req.output_mode);
    let backup_path = if req.output_mode == OutputMode::OverwriteWithBackup || output_path == input_path {
        match backup_manager::create_versioned_backup(input_path, "layout_intelligence") {
            Ok(rec) => {
                logs.push(format!("Đã tạo bản sao lưu an toàn: {}", rec.backup_path));
                Some(PathBuf::from(rec.backup_path))
            }
            Err(e) => {
                cleanup_tmp(temp_docx);
                let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
                let code = classified.code.as_str().to_string();
                let msg = classified.format_user_message();
                timer.finish_err(&code, &msg);
                return fail_exec_with_code(input_str, msg, &code);
            }
        }
    } else {
        None
    };

    if let Err(e) = safe_save::save_openxml_verified(
        &pkg,
        &output_path,
        Some(&snapshot),
        &PreservationPolicy::default(),
    ) {
        if let Some(ref bak) = backup_path {
            if req.output_mode == OutputMode::OverwriteWithBackup && output_path == input_path {
                let _ = engine::restore_backup(input_path, bak);
            }
        }
        cleanup_tmp(temp_docx);
        let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
        let code = classified.code.as_str().to_string();
        let msg = classified.format_user_message();
        timer.finish_err(&code, &msg);
        return fail_exec_with_code(input_str, msg, &code);
    }

    cleanup_tmp(temp_docx);

    // Re-open saved output from disk to verify XML integrity & refresh 7 maps
    let post_save_report = match super::analyze_file(&output_path, scope, org_profile) {
        Ok(r) => {
            logs.push(format!(
                "Hậu kiểm từ đĩa OK · {} cảnh báo còn lại · {} trang ước tính.",
                r.summary.warning_count, r.summary.estimated_page_count
            ));
            Some(r)
        }
        Err(e) => {
            if let Some(ref bak) = backup_path {
                if req.output_mode == OutputMode::OverwriteWithBackup && output_path == input_path {
                    let _ = engine::restore_backup(input_path, bak);
                }
            }
            let classified = classify_error_message(&e, AppErrorCode::ErrPostSaveVerify);
            let code = classified.code.as_str().to_string();
            let msg = AppStructuredError::new(
                classified.code,
                format!("Lỗi mở lại tệp sau khi lưu: {}", classified.cause),
            )
            .format_user_message();
            timer.finish_err(&code, &msg);
            return LayoutExecutionResult {
                success: false,
                output_path: output_path.to_string_lossy().to_string(),
                backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
                error: Some(msg),
                error_code: Some(code),
                applied_proposal_ids,
                skipped_proposal_ids,
                content_preserved_verified,
                relationships_preserved_verified,
                post_save_report: None,
                logs,
            };
        }
    };

    timer.finish_ok(&format!(
        "Áp dụng {} đề xuất bố cục, bảo toàn 100%",
        applied_proposal_ids.len()
    ));

    LayoutExecutionResult {
        success: true,
        output_path: output_path.to_string_lossy().to_string(),
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        error: None,
        error_code: None,
        applied_proposal_ids,
        skipped_proposal_ids,
        content_preserved_verified,
        relationships_preserved_verified,
        post_save_report,
        logs,
    }
}

fn should_apply_proposal(
    prop: &LayoutProposal,
    apply_high_confidence_auto: bool,
    accepted: &HashSet<&str>,
    rejected: &HashSet<&str>,
) -> bool {
    if rejected.contains(prop.proposal_id.as_str()) {
        return false;
    }
    if prop.affects_pagination
        || prop.affects_section
        || prop.affects_table
        || prop.affects_signature
        || prop.tier == LayoutProposalTier::RequiresConfirmation
    {
        return accepted.contains(prop.proposal_id.as_str());
    }
    apply_high_confidence_auto || accepted.contains(prop.proposal_id.as_str())
}

fn apply_normalize_spacing(
    body: &mut XmlElement,
    target_ids: &[String],
    para_lookup: &HashMap<&str, (usize, RecognizedStructureKind)>,
) -> bool {
    let mut touched = false;
    for id in target_ids {
        let Some(&(b_idx, kind)) = para_lookup.get(id.as_str()) else {
            continue;
        };
        if let Some(XmlNode::Element(p)) = body.children.get_mut(b_idx) {
            if p.name != "w:p" {
                continue;
            }
            let p_pr = p.ensure_child_first("w:pPr");
            let sp = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
            let is_heading = matches!(
                kind,
                RecognizedStructureKind::HeadingLevel1
                    | RecognizedStructureKind::HeadingLevel2
                    | RecognizedStructureKind::HeadingLevel3
                    | RecognizedStructureKind::HeadingLevel4
            );
            sp.set_attr("w:before", if is_heading { "160" } else { "120" });
            sp.set_attr("w:after", "120");
            sp.set_attr("w:line", "276");
            sp.set_attr("w:lineRule", "auto");
            touched = true;
        }
    }
    touched
}

fn apply_align_headings(
    body: &mut XmlElement,
    target_ids: &[String],
    para_lookup: &HashMap<&str, (usize, RecognizedStructureKind)>,
) -> bool {
    let mut touched = false;
    for id in target_ids {
        let Some(&(b_idx, kind)) = para_lookup.get(id.as_str()) else {
            continue;
        };
        if let Some(XmlNode::Element(p)) = body.children.get_mut(b_idx) {
            if p.name != "w:p" {
                continue;
            }
            let p_pr = p.ensure_child_first("w:pPr");
            let align = match kind {
                RecognizedStructureKind::DocTypeTitle
                | RecognizedStructureKind::DocSubject
                | RecognizedStructureKind::MainTitle
                | RecognizedStructureKind::HeadingLevel1 => "center",
                _ => "left",
            };
            let jc = p_pr.ensure_child_ordered("w:jc", P_PR_ORDER);
            jc.set_attr("w:val", align);
            let ind = p_pr.ensure_child_ordered("w:ind", P_PR_ORDER);
            ind.set_attr("w:firstLine", "0");
            ind.remove_attr("w:firstLineChars");
            if align == "center" {
                ind.set_attr("w:left", "0");
            }
            touched = true;
        }
    }
    touched
}

fn apply_unify_body_paragraphs(
    body: &mut XmlElement,
    target_ids: &[String],
    para_lookup: &HashMap<&str, (usize, RecognizedStructureKind)>,
) -> bool {
    let mut touched = false;
    for id in target_ids {
        let Some(&(b_idx, kind)) = para_lookup.get(id.as_str()) else {
            continue;
        };
        if kind != RecognizedStructureKind::MainBody {
            continue;
        }
        if let Some(XmlNode::Element(p)) = body.children.get_mut(b_idx) {
            if p.name != "w:p" {
                continue;
            }
            let p_pr = p.ensure_child_first("w:pPr");
            let wc = p_pr.ensure_child_ordered("w:widowControl", P_PR_ORDER);
            wc.remove_attr("w:val");
            let jc = p_pr.ensure_child_ordered("w:jc", P_PR_ORDER);
            jc.set_attr("w:val", "both");
            let existing_fl: i32 = p_pr
                .child("w:ind")
                .and_then(|i| i.get_attr("w:firstLine"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if existing_fl <= 100 {
                let ind = p_pr.ensure_child_ordered("w:ind", P_PR_ORDER);
                ind.set_attr("w:firstLine", "720");
            }
            touched = true;
        }
    }
    touched
}

fn apply_adjust_tables(
    body: &mut XmlElement,
    target_ids: &[String],
    tbl_lookup: &HashMap<&str, (usize, i32)>,
) -> bool {
    let mut touched = false;
    for id in target_ids {
        let Some(&(b_idx, _)) = tbl_lookup.get(id.as_str()) else {
            continue;
        };
        if let Some(XmlNode::Element(tbl)) = body.children.get_mut(b_idx) {
            if tbl.name != "w:tbl" {
                continue;
            }
            let tbl_pr = tbl.ensure_child_ordered("w:tblPr", TBL_PR_ORDER);
            let jc = tbl_pr.ensure_child_ordered("w:jc", TBL_PR_ORDER);
            jc.set_attr("w:val", "center");
            let ind = tbl_pr.ensure_child_ordered("w:tblInd", TBL_PR_ORDER);
            ind.set_attr("w:w", "0");
            ind.set_attr("w:type", "dxa");

            let row_count = tbl.children_elements().filter(|c| c.name == "w:tr").count();
            let mut r_idx = 0usize;
            for node in &mut tbl.children {
                let XmlNode::Element(tr) = node else {
                    continue;
                };
                if tr.name != "w:tr" {
                    continue;
                }
                let row_len: usize = tr
                    .children_elements()
                    .filter(|c| c.name == "w:tc")
                    .map(|c| c.extract_text().chars().count())
                    .sum();
                if r_idx == 0 && row_count >= 5 {
                    let tr_pr = tr.ensure_child_ordered("w:trPr", TR_PR_ORDER);
                    tr_pr.ensure_child_ordered("w:tblHeader", TR_PR_ORDER);
                }
                if row_len > 130 {
                    if let Some(tr_pr) = tr.child_mut("w:trPr") {
                        tr_pr.remove_children_by_name("w:cantSplit");
                    }
                }
                r_idx += 1;
            }
            touched = true;
        }
    }
    touched
}

fn apply_fit_table_widths(
    body: &mut XmlElement,
    target_ids: &[String],
    tbl_lookup: &HashMap<&str, (usize, i32)>,
) -> bool {
    let mut touched = false;
    for id in target_ids {
        let Some(&(b_idx, printable_twips)) = tbl_lookup.get(id.as_str()) else {
            continue;
        };
        let target_total = (printable_twips - 40).max(4000);
        if let Some(XmlNode::Element(tbl)) = body.children.get_mut(b_idx) {
            if tbl.name != "w:tbl" {
                continue;
            }
            // Scale w:tblGrid/w:gridCol cleanly
            let mut new_grid_cols: Vec<i32> = Vec::new();
            if let Some(grid) = tbl.child_mut("w:tblGrid") {
                let old_cols: Vec<i32> = grid
                    .children_elements()
                    .filter(|e| e.name == "w:gridCol")
                    .filter_map(|e| e.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()))
                    .collect();
                let old_sum: i32 = old_cols.iter().sum();
                if !old_cols.is_empty() && old_sum > 0 {
                    let mut running = 0i32;
                    for (i, &w) in old_cols.iter().enumerate() {
                        let scaled = if i + 1 == old_cols.len() {
                            (target_total - running).max(360)
                        } else {
                            let s = ((w as i64 * target_total as i64) / old_sum as i64) as i32;
                            s.max(360)
                        };
                        running += scaled;
                        new_grid_cols.push(scaled);
                    }
                    let mut gc_idx = 0usize;
                    for sub in grid.children_elements_mut() {
                        if sub.name == "w:gridCol" {
                            if let Some(&nw) = new_grid_cols.get(gc_idx) {
                                sub.set_attr("w:w", nw.to_string());
                            }
                            gc_idx += 1;
                        }
                    }
                }
            }

            let tbl_pr = tbl.ensure_child_ordered("w:tblPr", TBL_PR_ORDER);
            let tbl_w = tbl_pr.ensure_child_ordered("w:tblW", TBL_PR_ORDER);
            tbl_w.set_attr("w:w", target_total.to_string());
            tbl_w.set_attr("w:type", "dxa");
            let tbl_ind = tbl_pr.ensure_child_ordered("w:tblInd", TBL_PR_ORDER);
            tbl_ind.set_attr("w:w", "0");
            tbl_ind.set_attr("w:type", "dxa");

            if !new_grid_cols.is_empty() {
                for tr in tbl.children_elements_mut().filter(|e| e.name == "w:tr") {
                    let mut col_cursor = 0usize;
                    for tc in tr.children_elements_mut().filter(|e| e.name == "w:tc") {
                        let span: usize = tc
                            .child("w:tcPr")
                            .and_then(|tp| tp.child("w:gridSpan"))
                            .and_then(|g| g.get_attr("w:val"))
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1)
                            .max(1);
                        let sum_span: i32 = (col_cursor..(col_cursor + span).min(new_grid_cols.len()))
                            .filter_map(|ci| new_grid_cols.get(ci))
                            .sum();
                        col_cursor += span;
                        if sum_span > 0 {
                            let tc_pr = tc.ensure_child_ordered("w:tcPr", TC_PR_ORDER);
                            let tc_w = tc_pr.ensure_child_ordered("w:tcW", TC_PR_ORDER);
                            tc_w.set_attr("w:w", sum_span.to_string());
                            tc_w.set_attr("w:type", "dxa");
                        }
                    }
                }
            }
            touched = true;
        }
    }
    touched
}

fn apply_fit_inline_image(
    body: &mut XmlElement,
    vis_id: &str,
    vis_lookup: &HashMap<&str, (usize, Option<String>, f64, f64)>,
) -> bool {
    let Some((b_idx, rid_opt, pw_mm, ph_mm)) = vis_lookup.get(vis_id) else {
        return false;
    };
    let Some(XmlNode::Element(el)) = body.children.get_mut(*b_idx) else {
        return false;
    };
    let max_cx = ((*pw_mm - 2.0).max(40.0) * 36_000.0).round() as i64;
    let max_cy = ((*ph_mm - 5.0).max(40.0) * 36_000.0).round() as i64;
    scale_inline_image_recursive(el, rid_opt.as_deref(), max_cx, max_cy)
}

fn scale_inline_image_recursive(
    el: &mut XmlElement,
    target_rid: Option<&str>,
    max_cx: i64,
    max_cy: i64,
) -> bool {
    if el.name == "mc:AlternateContent" {
        if let Some(choice) = el.child_mut("mc:Choice") {
            return scale_inline_image_recursive(choice, target_rid, max_cx, max_cy);
        }
        return false;
    }
    if el.name == "w:txbxContent" {
        return false;
    }
    if el.name == "wp:inline" {
        let mut blips = Vec::new();
        collect_blip_rids_readonly(el, &mut blips);
        if let Some(trid) = target_rid {
            if !blips.iter().any(|r| r == trid) {
                return false;
            }
        }
        if let Some(ext) = el.child_mut("wp:extent") {
            let cx: i64 = ext.get_attr("cx").and_then(|v| v.parse().ok()).unwrap_or(0);
            let cy: i64 = ext.get_attr("cy").and_then(|v| v.parse().ok()).unwrap_or(0);
            if cx > 0 && cy > 0 && (cx > max_cx || cy > max_cy) {
                let scale = (max_cx as f64 / cx as f64)
                    .min(max_cy as f64 / cy as f64)
                    .min(1.0);
                let new_cx = ((cx as f64 * scale).round() as i64).max(360_000);
                let new_cy = ((cy as f64 * scale).round() as i64).max(360_000);
                ext.set_attr("cx", new_cx.to_string());
                ext.set_attr("cy", new_cy.to_string());
                update_nested_xfrm_ext(el, new_cx, new_cy);
                return true;
            }
        }
        return false;
    }
    let mut changed = false;
    for sub in el.children_elements_mut() {
        if scale_inline_image_recursive(sub, target_rid, max_cx, max_cy) {
            changed = true;
        }
    }
    changed
}

fn collect_blip_rids_readonly(el: &XmlElement, out: &mut Vec<String>) {
    if el.name == "a:blip" {
        if let Some(r) = el.get_attr("r:embed").or_else(|| el.get_attr("r:link")) {
            out.push(r.to_string());
        }
    }
    for sub in el.children_elements() {
        collect_blip_rids_readonly(sub, out);
    }
}

fn update_nested_xfrm_ext(el: &mut XmlElement, new_cx: i64, new_cy: i64) {
    if el.name == "a:xfrm" {
        if let Some(ext) = el.child_mut("a:ext") {
            ext.set_attr("cx", new_cx.to_string());
            ext.set_attr("cy", new_cy.to_string());
        }
        return;
    }
    for sub in el.children_elements_mut() {
        update_nested_xfrm_ext(sub, new_cx, new_cy);
    }
}

fn apply_landscape_section(body: &mut XmlElement, target_sec_idx: usize) -> bool {
    let mut cur_sec = 0usize;
    for node in &mut body.children {
        let XmlNode::Element(el) = node else {
            continue;
        };
        if el.name == "w:p" {
            if let Some(p_pr) = el.child_mut("w:pPr") {
                if let Some(sect) = p_pr.child_mut("w:sectPr") {
                    cur_sec += 1;
                    if cur_sec == target_sec_idx {
                        rotate_sect_pr_to_landscape(sect);
                        return true;
                    }
                }
            }
        }
    }
    if let Some(sect) = body.child_mut("w:sectPr") {
        cur_sec += 1;
        if cur_sec == target_sec_idx {
            rotate_sect_pr_to_landscape(sect);
            return true;
        }
    }
    false
}

fn rotate_sect_pr_to_landscape(sect: &mut XmlElement) {
    let pg_sz = sect.ensure_child_ordered("w:pgSz", SECT_PR_ORDER);
    let w: i32 = pg_sz
        .get_attr("w:w")
        .and_then(|v| v.parse().ok())
        .unwrap_or(11906);
    let h: i32 = pg_sz
        .get_attr("w:h")
        .and_then(|v| v.parse().ok())
        .unwrap_or(16838);
    pg_sz.set_attr("w:w", w.max(h).to_string());
    pg_sz.set_attr("w:h", w.min(h).to_string());
    pg_sz.set_attr("w:orient", "landscape");

    let pg_mar = sect.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
    pg_mar.set_attr("w:top", "1701");
    pg_mar.set_attr("w:right", "850");
    pg_mar.set_attr("w:bottom", "1134");
    pg_mar.set_attr("w:left", "1134");
}

fn apply_keep_next_lines(
    body: &mut XmlElement,
    target_ids: &[String],
    para_lookup: &HashMap<&str, (usize, RecognizedStructureKind)>,
    keep_all_next: bool,
) -> bool {
    let mut touched = false;
    let len = target_ids.len();
    for (i, id) in target_ids.iter().enumerate() {
        let Some(&(b_idx, _)) = para_lookup.get(id.as_str()) else {
            continue;
        };
        if let Some(XmlNode::Element(p)) = body.children.get_mut(b_idx) {
            if p.name != "w:p" {
                continue;
            }
            let p_pr = p.ensure_child_first("w:pPr");
            p_pr.ensure_child_ordered("w:keepLines", P_PR_ORDER);
            if keep_all_next || i + 1 < len {
                p_pr.ensure_child_ordered("w:keepNext", P_PR_ORDER);
            }
            touched = true;
        }
    }
    touched
}

fn apply_remove_page_break_before(
    body: &mut XmlElement,
    para_id: &str,
    para_lookup: &HashMap<&str, (usize, RecognizedStructureKind)>,
) -> bool {
    let Some(&(b_idx, _)) = para_lookup.get(para_id) else {
        return false;
    };
    if let Some(XmlNode::Element(p)) = body.children.get_mut(b_idx) {
        if let Some(p_pr) = p.child_mut("w:pPr") {
            let had = p_pr.child("w:pageBreakBefore").is_some();
            p_pr.remove_children_by_name("w:pageBreakBefore");
            return had;
        }
    }
    false
}

fn collect_empty_removals(
    body: &XmlElement,
    target_ids: &[String],
    para_lookup: &HashMap<&str, (usize, RecognizedStructureKind)>,
    out: &mut HashSet<usize>,
) -> bool {
    let mut added = false;
    for id in target_ids {
        let Some(&(b_idx, _)) = para_lookup.get(id.as_str()) else {
            continue;
        };
        if let Some(XmlNode::Element(p)) = body.children.get(b_idx) {
            if p.name == "w:p" && is_removable_empty_paragraph(p) {
                let has_sect = p
                    .child("w:pPr")
                    .and_then(|pp| pp.child("w:sectPr"))
                    .is_some();
                if !has_sect {
                    out.insert(b_idx);
                    added = true;
                }
            }
        }
    }
    added
}

/// Collects every non-empty paragraph's normalized text across body, tables AND Text Boxes (`mc:Choice`/`w:txbxContent`)
/// so we verify 0% content mutation before writing.
pub fn collect_all_non_empty_paragraph_texts(root: &XmlElement) -> Vec<String> {
    let mut out = Vec::new();
    collect_para_texts_recursive(root, &mut out);
    out
}

fn collect_para_texts_recursive(el: &XmlElement, out: &mut Vec<String>) {
    if el.name == "mc:AlternateContent" {
        if let Some(choice) = el.child("mc:Choice") {
            collect_para_texts_recursive(choice, out);
        } else if let Some(fallback) = el.child("mc:Fallback") {
            collect_para_texts_recursive(fallback, out);
        }
        return;
    }
    if el.name == "w:p" {
        let t = el.extract_text().trim().to_string();
        if !t.is_empty() {
            out.push(t);
        }
        // Also collect any paragraphs inside nested w:txbxContent
        for sub in el.children_elements() {
            if matches!(
                sub.name.as_str(),
                "mc:AlternateContent" | "w:drawing" | "w:pict" | "w:r" | "w:sdt" | "w:sdtContent"
            ) {
                collect_para_texts_recursive(sub, out);
            }
        }
        return;
    }
    for sub in el.children_elements() {
        collect_para_texts_recursive(sub, out);
    }
}

fn cleanup_tmp(tmp: Option<PathBuf>) {
    if let Some(t) = tmp {
        let _ = fs::remove_file(t);
    }
}

fn fail_exec_with_code(input_path: String, err: String, code: &str) -> LayoutExecutionResult {
    let _ = input_path;
    LayoutExecutionResult {
        success: false,
        output_path: String::new(),
        backup_path: None,
        error: Some(err),
        error_code: Some(code.to_string()),
        applied_proposal_ids: Vec::new(),
        skipped_proposal_ids: Vec::new(),
        content_preserved_verified: true,
        relationships_preserved_verified: true,
        post_save_report: None,
        logs: Vec::new(),
    }
}
