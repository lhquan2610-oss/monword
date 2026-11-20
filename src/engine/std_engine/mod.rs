pub mod differ;
pub mod fixer;
pub mod ir;
pub mod rules;

use crate::engine::openxml_arch::{OpenXmlPackage, SelectivePackageView};
use crate::engine::reliability::backup_manager;
use crate::engine::reliability::error_code::{classify_error_message, AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::preservation::{PreservationPolicy, PreservationSnapshot};
use crate::engine::reliability::safe_save;
use crate::engine::{self, system_office};
use crate::models::{
    OrganizationProfile, OutputMode, ParagraphLocationPreview, PostSaveVerification,
    RuleCheckStatus, RuleEvalSummary, StdAnalysisReport, StdDocScope, StdExecutionRequest,
    StdExecutionResult, StdRuleDescriptor, StdSeverity, StdSeverityCounts, StdStatusCounts,
};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn get_std_rules_catalog() -> Vec<StdRuleDescriptor> {
    rules::build_rule_registry()
        .into_iter()
        .map(|r| r.descriptor)
        .collect()
}

pub fn analyze_file(
    path: &Path,
    scope: StdDocScope,
    org_profile: Option<&OrganizationProfile>,
) -> Result<(StdAnalysisReport, ir::DocumentIr), String> {
    if !path.is_file() {
        return Err(format!("Không tìm thấy tệp văn bản: {}", path.display()));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut temp_docx: Option<PathBuf> = None;
    let working_path = if ext == "doc" {
        let tmp = env::temp_dir().join(format!(
            "monword_std_ana_{}_{}.docx",
            std::process::id(),
            chrono::Local::now().timestamp_millis()
        ));
        system_office::word_com_convert_doc_to_docx(path, &tmp)?;
        temp_docx = Some(tmp.clone());
        tmp
    } else if ext == "docx" || ext == "docm" {
        path.to_path_buf()
    } else {
        return Err(format!(
            "Định dạng .{} chưa hỗ trợ kiểm chuẩn tài liệu Word (.docx, .docm, .doc).",
            ext
        ));
    };

    let view_res = OpenXmlPackage::open_selective_for_analysis(&working_path);
    if let Some(t) = temp_docx {
        let _ = fs::remove_file(t);
    }
    let view = view_res?;

    let mut doc_ir = ir::DocumentIr::build(path, &view, scope, org_profile)?;
    doc_ir.file_ext = ext;
    let report = evaluate_ir(&doc_ir, org_profile, &[]);
    Ok((report, doc_ir))
}

/// Shared helper exposing Phase 3 analysis together with Phase 5's 7 Intelligent Maps
/// from a single package read (for Phase 3 UI/consumers & Phase 7 integration).
#[allow(dead_code)]
pub fn analyze_file_with_layout_map(
    path: &Path,
    scope: StdDocScope,
    org_profile: Option<&OrganizationProfile>,
) -> Result<(StdAnalysisReport, ir::DocumentIr, crate::models::IntelligentDocumentMap), String> {
    if !path.is_file() {
        return Err(format!("Không tìm thấy tệp văn bản: {}", path.display()));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let view = OpenXmlPackage::open_selective_for_analysis(path)?;
    let mut doc_ir = ir::DocumentIr::build(path, &view, scope, org_profile)?;
    doc_ir.file_ext = ext;
    let report = evaluate_ir(&doc_ir, org_profile, &[]);
    let map = crate::engine::layout_intelligence::build_map_from_ir(&view, &doc_ir)?;
    Ok((report, doc_ir, map))
}

pub fn evaluate_ir(
    doc_ir: &ir::DocumentIr,
    org_profile: Option<&OrganizationProfile>,
    disabled_rule_ids: &[String],
) -> StdAnalysisReport {
    let registry = rules::build_rule_registry();
    let ctx = rules::RuleEvalContext {
        org_profile,
        disabled_rule_ids,
    };

    let mut rule_summaries = Vec::with_capacity(registry.len());
    let mut findings = Vec::new();
    let mut status_counts = StdStatusCounts::default();
    let mut severity_counts = StdSeverityCounts::default();

    for def in &registry {
        let d = &def.descriptor;
        let (applicable, override_status) = rules::is_rule_applicable_to_doc(d, doc_ir, &ctx);

        let (status, mut rule_findings) = if !applicable {
            (
                override_status.unwrap_or(RuleCheckStatus::NotApplicable),
                Vec::new(),
            )
        } else {
            let out = (def.check_fn)(doc_ir, &ctx);
            (out.status, out.findings)
        };

        match status {
            RuleCheckStatus::Passed => status_counts.passed += 1,
            RuleCheckStatus::Failed => status_counts.failed += 1,
            RuleCheckStatus::NeedsReview => status_counts.needs_review += 1,
            RuleCheckStatus::NotApplicable => status_counts.not_applicable += 1,
            RuleCheckStatus::Indeterminate => status_counts.indeterminate += 1,
        }

        for f in &rule_findings {
            match f.severity {
                StdSeverity::Critical => severity_counts.critical += 1,
                StdSeverity::Error => severity_counts.error += 1,
                StdSeverity::Warning => severity_counts.warning += 1,
                StdSeverity::Information => severity_counts.information += 1,
            }
        }

        rule_summaries.push(RuleEvalSummary {
            rule_id: d.rule_id.clone(),
            rule_name: d.rule_name.clone(),
            category: d.category,
            severity: d.severity,
            status,
            finding_count: rule_findings.len(),
            legal_basis: d.legal_basis.clone(),
            auto_fix_supported: d.auto_fix_supported,
            safety_tier: d.safety_tier,
        });

        findings.append(&mut rule_findings);
    }

    let flagged_paths: HashSet<&str> = findings.iter().map(|f| f.element_path.as_str()).collect();
    let mut paragraph_previews = Vec::new();
    for (idx, p) in doc_ir.paragraphs.iter().enumerate() {
        if idx < 250 || flagged_paths.contains(p.element_path.as_str()) {
            if paragraph_previews.len() >= 450 && !flagged_paths.contains(p.element_path.as_str()) {
                continue;
            }
            paragraph_previews.push(ParagraphLocationPreview {
                element_path: p.element_path.clone(),
                index: p.global_index,
                role: p.role.label_vi().to_string(),
                section_index: p.section_index,
                excerpt: p.trimmed.chars().take(110).collect(),
                alignment: p.effective_jc.clone(),
                font_name: p.primary_font.clone(),
                font_size_pt: p.primary_size_pt,
            });
        }
    }

    StdAnalysisReport {
        file_path: doc_ir.file_path.clone(),
        file_name: doc_ir.file_name.clone(),
        file_ext: doc_ir.file_ext.clone(),
        requested_scope: doc_ir.requested_scope,
        effective_scope: doc_ir.effective_scope,
        requires_scope_selection: doc_ir.requires_scope_selection,
        scope_reason: doc_ir.scope_reason.clone(),
        sections: doc_ir.sections.iter().map(|s| s.summary.clone()).collect(),
        tables: doc_ir.tables.iter().map(|t| t.summary.clone()).collect(),
        headers_footers: doc_ir.headers_footers.clone(),
        paragraph_count: doc_ir.paragraphs.len(),
        paragraph_previews,
        assets: doc_ir.assets.clone(),
        rule_summaries,
        findings,
        status_counts,
        severity_counts,
    }
}

fn resolve_std_output_path(input_path: &Path, output_mode: OutputMode) -> PathBuf {
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
            parent.join(format!("{}_ChuanTheThuc.{}", stem, target_ext))
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

pub(crate) struct LoadedStdContext {
    pub view: SelectivePackageView,
    pub ir: ir::DocumentIr,
    pub report: StdAnalysisReport,
}

pub fn execute_standardization(req: &StdExecutionRequest) -> StdExecutionResult {
    execute_standardization_returning_views(req).0
}

pub(crate) fn execute_standardization_returning_views(
    req: &StdExecutionRequest,
) -> (
    StdExecutionResult,
    Option<LoadedStdContext>,
    Option<LoadedStdContext>,
) {
    let input_path = Path::new(&req.path);
    let timer = OpTimer::start("std_engine", "execute_standardization", Some(input_path));
    let input_str = input_path.to_string_lossy().to_string();

    if !input_path.is_file() {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrFileRead,
            format!("Tệp không tồn tại: {}", req.path),
        )
        .format_user_message();
        timer.finish_err("ERR_FILE_READ", &msg);
        return (
            fail_exec_with_code(input_str, None, msg, "ERR_FILE_READ"),
            None,
            None,
        );
    }

    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut temp_docx: Option<PathBuf> = None;
    let working_path = if ext == "doc" {
        let tmp = env::temp_dir().join(format!(
            "monword_std_work_{}_{}.docx",
            std::process::id(),
            chrono::Local::now().timestamp_millis()
        ));
        if let Err(e) = system_office::word_com_convert_doc_to_docx(input_path, &tmp) {
            let classified = classify_error_message(&e, AppErrorCode::ErrDocConvert);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return (fail_exec_with_code(input_str, None, msg, &code), None, None);
        }
        temp_docx = Some(tmp.clone());
        tmp
    } else if ext == "docx" || ext == "docm" {
        input_path.to_path_buf()
    } else {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            format!("Không hỗ trợ chuẩn hóa định dạng .{}.", ext),
        )
        .format_user_message();
        timer.finish_err("ERR_OPENXML_CORRUPT", &msg);
        return (
            fail_exec_with_code(input_str, None, msg, "ERR_OPENXML_CORRUPT"),
            None,
            None,
        );
    };

    // 1. Single-pass open when scope is explicit (avoid reading & decompressing ZIP twice);
    // keep selective pre-read only when AutoDetect might reject before loading binary media.
    let (mut pkg_opt, view_before) = if req.scope == StdDocScope::AutoDetect {
        match OpenXmlPackage::open_selective_for_analysis(&working_path) {
            Ok(v) => (None, v),
            Err(e) => {
                cleanup_tmp(temp_docx);
                let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
                let code = classified.code.as_str().to_string();
                let msg = classified.format_user_message();
                timer.finish_err(&code, &msg);
                return (fail_exec_with_code(input_str, None, msg, &code), None, None);
            }
        }
    } else {
        match OpenXmlPackage::open(&working_path) {
            Ok(p) => match p.to_selective_view() {
                Ok(v) => (Some(p), v),
                Err(e) => {
                    cleanup_tmp(temp_docx);
                    let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
                    let code = classified.code.as_str().to_string();
                    let msg = classified.format_user_message();
                    timer.finish_err(&code, &msg);
                    return (fail_exec_with_code(input_str, None, msg, &code), None, None);
                }
            },
            Err(e) => {
                cleanup_tmp(temp_docx);
                let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
                let code = classified.code.as_str().to_string();
                let msg = classified.format_user_message();
                timer.finish_err(&code, &msg);
                return (fail_exec_with_code(input_str, None, msg, &code), None, None);
            }
        }
    };

    let mut ir_before = match ir::DocumentIr::build(
        input_path,
        &view_before,
        req.scope,
        req.org_profile.as_ref(),
    ) {
        Ok(ir) => ir,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return (fail_exec_with_code(input_str, None, msg, &code), None, None);
        }
    };
    ir_before.file_ext = ext.clone();
    let report_before = evaluate_ir(
        &ir_before,
        req.org_profile.as_ref(),
        &req.disabled_rule_ids,
    );

    if report_before.requires_scope_selection && req.scope == StdDocScope::AutoDetect {
        cleanup_tmp(temp_docx);
        let msg = AppStructuredError::new(
            AppErrorCode::ErrIpcInvalid,
            "Chưa xác định chắc chắn loại tài liệu. Vui lòng chọn Phạm vi áp dụng quy tắc trước khi thực hiện sửa đổi.",
        )
        .format_user_message();
        timer.finish_err("ERR_IPC_INVALID", &msg);
        let before_ctx = LoadedStdContext {
            view: view_before,
            ir: ir_before,
            report: report_before.clone(),
        };
        return (
            StdExecutionResult {
                success: false,
                input_path: input_str,
                output_path: String::new(),
                backup_path: None,
                error: Some(msg),
                error_code: Some("ERR_IPC_INVALID".to_string()),
                logs: vec![],
                actions_applied: vec![],
                report_before: Some(report_before),
                report_after: None,
                diff: None,
                verification: None,
            },
            Some(before_ctx),
            None,
        );
    }

    let output_path = resolve_std_output_path(input_path, req.output_mode);

    // 2. Backup original if overwriting (or output == input)
    let backup_path = if req.output_mode == OutputMode::OverwriteWithBackup || output_path == input_path {
        match backup_manager::create_versioned_backup(input_path, "std_engine") {
            Ok(rec) => Some(PathBuf::from(rec.backup_path)),
            Err(e) => {
                cleanup_tmp(temp_docx);
                let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
                let code = classified.code.as_str().to_string();
                let msg = classified.format_user_message();
                timer.finish_err(&code, &msg);
                return (
                    fail_exec_with_code(input_str, Some(report_before), msg, &code),
                    None,
                    None,
                );
            }
        }
    } else {
        None
    };
    let backup_str = backup_path
        .as_ref()
        .map(|b| b.to_string_lossy().to_string());

    // 3. Reuse already-opened OpenXmlPackage if present, else open now
    let mut pkg = match pkg_opt.take() {
        Some(p) => p,
        None => match OpenXmlPackage::open(&working_path) {
            Ok(p) => p,
            Err(e) => {
                cleanup_tmp(temp_docx);
                let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
                let code = classified.code.as_str().to_string();
                let msg = classified.format_user_message();
                timer.finish_err(&code, &msg);
                return (
                    fail_exec_with_code(input_str, Some(report_before), msg, &code),
                    None,
                    None,
                );
            }
        },
    };

    let snapshot = match PreservationSnapshot::capture(&pkg) {
        Ok(s) => s,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return (
                fail_exec_with_code(input_str, Some(report_before), msg, &code),
                None,
                None,
            );
        }
    };

    let fix_out = match fixer::apply_selected_fixes(&mut pkg, &ir_before, &report_before, req) {
        Ok(o) => o,
        Err(e) => {
            cleanup_tmp(temp_docx);
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return (
                fail_exec_with_code(input_str, Some(report_before), msg, &code),
                None,
                None,
            );
        }
    };

    if let Err(e) = safe_save::save_openxml_verified(
        &pkg,
        &output_path,
        Some(&snapshot),
        &PreservationPolicy::default(),
    ) {
        cleanup_tmp(temp_docx);
        let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
        let code = classified.code.as_str().to_string();
        let msg = classified.format_user_message();
        timer.finish_err(&code, &msg);
        return (
            fail_exec_with_code(input_str, Some(report_before), msg, &code),
            None,
            None,
        );
    }
    cleanup_tmp(temp_docx);

    // 4. Mandatory Disk Post-Save Re-open & Preservation Verification (Section 10)
    let output_exists = output_path.is_file();
    let output_size_bytes = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

    let (reopened_from_disk, view_after_opt, ir_after_opt, report_after_opt, reopen_err) = if output_exists {
        match OpenXmlPackage::open_selective_for_analysis(&output_path) {
            Ok(view_after) => match ir::DocumentIr::build(
                &output_path,
                &view_after,
                report_before.effective_scope,
                req.org_profile.as_ref(),
            ) {
                Ok(mut ir_a) => {
                    ir_a.file_ext = ext;
                    let rep_a = evaluate_ir(
                        &ir_a,
                        req.org_profile.as_ref(),
                        &req.disabled_rule_ids,
                    );
                    (true, Some(view_after), Some(ir_a), Some(rep_a), None)
                }
                Err(e) => (false, None, None, None, Some(e)),
            },
            Err(e) => (false, None, None, None, Some(e)),
        }
    } else {
        (
            false,
            None,
            None,
            None,
            Some("Tệp đầu ra không tồn tại sau khi lưu.".to_string()),
        )
    };

    let mut report_before_annotated = report_before.clone();
    report_before_annotated.findings = fix_out.updated_findings;

    let (diff_opt, verification, final_ok, err_msg, err_code) =
        match (ir_after_opt.as_ref(), report_after_opt.as_ref()) {
            (Some(ir_after), Some(rep_after)) => {
                let diff = differ::compute_document_diff(&ir_before, ir_after, &fix_out.actions);
                let ver = differ::build_post_save_verification(
                    output_exists,
                    output_size_bytes,
                    reopened_from_disk,
                    Some(rep_after),
                    &diff.preservation,
                );
                let ok = ver.output_exists && ver.xml_valid && ver.preservation_passed;
                let (err, code) = if ok {
                    (None, None)
                } else {
                    let msg = AppStructuredError::new(
                        AppErrorCode::ErrPostSaveVerify,
                        ver.summary_message.clone(),
                    )
                    .format_user_message();
                    (Some(msg), Some("ERR_POST_SAVE_VERIFY".to_string()))
                };
                (Some(diff), ver, ok, err, code)
            }
            _ => {
                let raw = reopen_err
                    .clone()
                    .unwrap_or_else(|| "Không mở lại được tệp đã lưu để kiểm chứng.".to_string());
                let classified = classify_error_message(&raw, AppErrorCode::ErrPostSaveVerify);
                let ver = PostSaveVerification {
                    output_exists,
                    output_size_bytes,
                    reopened_from_disk,
                    xml_valid: false,
                    preservation_passed: false,
                    remaining_critical_or_error: 0,
                    summary_message: classified.format_user_message(),
                };
                (
                    None,
                    ver.clone(),
                    false,
                    Some(ver.summary_message),
                    Some(classified.code.as_str().to_string()),
                )
            }
        };

    if !final_ok {
        if let Some(ref bak) = backup_path {
            if req.output_mode == OutputMode::OverwriteWithBackup && output_path == input_path {
                let _ = engine::restore_backup(input_path, bak);
            }
        }
        timer.finish_err(
            err_code.as_deref().unwrap_or("ERR_POST_SAVE_VERIFY"),
            err_msg.as_deref().unwrap_or("Hậu kiểm chuẩn hóa thất bại"),
        );
    } else {
        timer.finish_ok(&format!(
            "Chuẩn hóa {} thao tác, hậu kiểm đạt ({} bytes)",
            fix_out.actions.len(),
            output_size_bytes
        ));
    }

    let mut logs = fix_out.logs;
    logs.push(verification.summary_message.clone());

    let before_ctx = LoadedStdContext {
        view: view_before,
        ir: ir_before,
        report: report_before,
    };
    let after_ctx = match (view_after_opt, ir_after_opt, report_after_opt.clone()) {
        (Some(v_a), Some(ir_a), Some(r_a)) => Some(LoadedStdContext {
            view: v_a,
            ir: ir_a,
            report: r_a,
        }),
        _ => None,
    };

    (
        StdExecutionResult {
            success: final_ok,
            input_path: input_str,
            output_path: output_path.to_string_lossy().to_string(),
            backup_path: backup_str,
            error: err_msg,
            error_code: err_code,
            logs,
            actions_applied: fix_out.actions,
            report_before: Some(report_before_annotated),
            report_after: report_after_opt,
            diff: diff_opt,
            verification: Some(verification),
        },
        Some(before_ctx),
        after_ctx,
    )
}

fn cleanup_tmp(tmp: Option<PathBuf>) {
    if let Some(t) = tmp {
        let _ = fs::remove_file(t);
    }
}

fn fail_exec_with_code(
    input_path: String,
    report_before: Option<StdAnalysisReport>,
    err: String,
    code: &str,
) -> StdExecutionResult {
    StdExecutionResult {
        success: false,
        input_path,
        output_path: String::new(),
        backup_path: None,
        error: Some(err),
        error_code: Some(code.to_string()),
        logs: vec![],
        actions_applied: vec![],
        report_before,
        report_after: None,
        diff: None,
        verification: None,
    }
}
