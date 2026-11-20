pub mod advanced_checks;
pub mod exporter;
pub mod history_store;
pub mod matrix_store;
pub mod sha256_util;

use crate::engine::formatter::AdminTableKind;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::std_engine::{self, ir::ParagraphRole};
use crate::engine::{layout_intelligence, system_office};
use crate::models::{
    AuditExportFormat, AuditExportResult, AuditFileDigest, AuditHistoryEntry,
    AuditRecheckSummary, AuditRuleProfileUsed, AuditRunComparison, ComplianceAuditReport,
    ComplianceAuditRequest, ComplianceAutoFixAndRecheckRequest,
    ComplianceAutoFixAndRecheckResult, ComplianceBasisKind, ComplianceMatrixRow,
    ComplianceObligation, ComplianceRulePack, OrganizationProfile, RuleVersionEntry,
    StdDocScope, TraceableFinding,
};
use chrono::Local;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub use exporter::NON_LEGAL_DISCLAIMER_VI;
pub use history_store::AuditHistoryStore;
pub use matrix_store::ComplianceRulePackStore;

pub fn get_active_rule_pack(store: &ComplianceRulePackStore) -> ComplianceRulePack {
    store.load_active_pack()
}

pub fn save_rule_pack(
    store: &ComplianceRulePackStore,
    pack: &ComplianceRulePack,
) -> Result<ComplianceRulePack, String> {
    store.save_pack_override(pack)
}

pub fn reset_rule_pack(store: &ComplianceRulePackStore) -> Result<ComplianceRulePack, String> {
    store.reset_to_builtin()
}

pub fn preview_compliance_matrix(
    store: &ComplianceRulePackStore,
    scope: StdDocScope,
    doc_type: &str,
    org_profile: Option<&OrganizationProfile>,
) -> Vec<ComplianceMatrixRow> {
    let pack = store.load_active_pack();
    matrix_store::build_compliance_matrix(
        &pack,
        scope,
        doc_type,
        org_profile,
        &[],
        None,
        2,
        1,
        true,
    )
}

pub fn run_compliance_audit(
    req: &ComplianceAuditRequest,
    pack_store: &ComplianceRulePackStore,
    history_store: &AuditHistoryStore,
) -> Result<ComplianceAuditReport, String> {
    let input_path = Path::new(&req.path);
    let timer = crate::engine::reliability::logger::OpTimer::start(
        "compliance_audit",
        "run_compliance_audit",
        Some(input_path),
    );
    let report = match build_compliance_report_internal(
        input_path,
        req.scope,
        req.doc_type_hint.as_deref(),
        req.org_profile.as_ref(),
        &req.disabled_rule_ids,
        pack_store,
        None,
    ) {
        Ok(r) => r,
        Err(e) => {
            let classified = crate::engine::reliability::error_code::classify_error_message(
                &e,
                crate::engine::reliability::error_code::AppErrorCode::ErrOpenXmlCorrupt,
            );
            let msg = classified.format_user_message();
            timer.finish_err(classified.code.as_str(), &msg);
            return Err(msg);
        }
    };

    if req.record_history {
        if let Err(e) = history_store.record_report(&report, Vec::new(), Vec::new()) {
            let classified = crate::engine::reliability::error_code::classify_error_message(
                &e,
                crate::engine::reliability::error_code::AppErrorCode::ErrFileWrite,
            );
            let msg = classified.format_user_message();
            timer.finish_err(classified.code.as_str(), &msg);
            return Err(msg);
        }
    }

    timer.finish_ok(&format!(
        "Kiểm toán {} quy tắc (chỉ số {}/100)",
        report.matrix.len(),
        report.consistency.technical_quality_index
    ));
    Ok(report)
}

pub fn execute_autofix_with_recheck(
    req: &ComplianceAutoFixAndRecheckRequest,
    pack_store: &ComplianceRulePackStore,
    history_store: &AuditHistoryStore,
) -> ComplianceAutoFixAndRecheckResult {
    let input_path = Path::new(&req.exec_request.path);
    let pack = pack_store.load_active_pack();
    let mut combined_disabled = req.exec_request.disabled_rule_ids.clone();
    for r in &pack.rules {
        if !r.enabled && !combined_disabled.contains(&r.rule_id) {
            combined_disabled.push(r.rule_id.clone());
        }
    }

    // Read raw compressed bytes BEFORE execution so SHA-256 remains accurate even in OverwriteWithBackup mode
    let before_digest_opt = if input_path.is_file() {
        fs::read(input_path)
            .ok()
            .map(|raw| (raw.len() as u64, sha256_util::sha256_hex(&raw)))
    } else {
        None
    };

    let mut adjusted_exec = req.exec_request.clone();
    adjusted_exec.disabled_rule_ids = combined_disabled.clone();
    let (std_execution, before_ctx_opt, after_ctx_opt) =
        std_engine::execute_standardization_returning_views(&adjusted_exec);

    let audit_before = match (before_ctx_opt, before_digest_opt) {
        (Some(before_ctx), Some((before_size, before_sha))) => {
            build_compliance_report_from_loaded(
                input_path,
                before_size,
                before_sha,
                before_ctx.ir.file_ext.clone(),
                &before_ctx.view,
                &before_ctx.ir,
                &before_ctx.report,
                req.doc_type_hint.as_deref(),
                req.exec_request.org_profile.as_ref(),
                &combined_disabled,
                pack_store,
                None,
            )
            .ok()
        }
        _ => None,
    };

    let mut fixed_finding_ids = Vec::new();
    for act in &std_execution.actions_applied {
        if let Some(ref rep_b) = audit_before {
            if let Some(tf) = rep_b
                .traceable_findings
                .iter()
                .find(|f| f.rule_id == act.rule_id && f.element_path == act.element_path)
            {
                if !fixed_finding_ids.contains(&tf.finding_id) {
                    fixed_finding_ids.push(tf.finding_id.clone());
                }
            }
        }
    }

    let skipped_ids = req.exec_request.skipped_finding_ids.clone();

    let audit_after = if std_execution.success && !std_execution.output_path.is_empty() {
        let out_p = Path::new(&std_execution.output_path);
        let recheck_meta = audit_before.as_ref().map(|before| AuditRecheckContext {
            before_failed_rules: before
                .matrix
                .iter()
                .filter(|m| m.evaluation_status == Some(crate::models::RuleCheckStatus::Failed))
                .count(),
            before_findings: before.traceable_findings.len(),
            fixed_count: std_execution.actions_applied.len().max(fixed_finding_ids.len()),
            skipped_count: skipped_ids.len(),
            preservation_passed: std_execution
                .verification
                .as_ref()
                .map(|v| v.preservation_passed)
                .unwrap_or(false),
            before_anomaly_count: before.advanced_issues.len(),
        });

        match (after_ctx_opt, fs::read(out_p).ok()) {
            (Some(after_ctx), Some(out_raw)) => build_compliance_report_from_loaded(
                out_p,
                out_raw.len() as u64,
                sha256_util::sha256_hex(&out_raw),
                after_ctx.ir.file_ext.clone(),
                &after_ctx.view,
                &after_ctx.ir,
                &after_ctx.report,
                req.doc_type_hint.as_deref(),
                req.exec_request.org_profile.as_ref(),
                &combined_disabled,
                pack_store,
                recheck_meta,
            )
            .ok(),
            _ => build_compliance_report_internal(
                out_p,
                req.exec_request.scope,
                req.doc_type_hint.as_deref(),
                req.exec_request.org_profile.as_ref(),
                &combined_disabled,
                pack_store,
                recheck_meta,
            )
            .ok(),
        }
    } else {
        None
    };

    let history_entry = if req.record_history {
        let target_rep = audit_after.as_ref().or(audit_before.as_ref());
        target_rep.and_then(|rep| {
            history_store
                .record_report(rep, fixed_finding_ids, skipped_ids)
                .ok()
        })
    } else {
        None
    };

    ComplianceAutoFixAndRecheckResult {
        std_execution,
        audit_before,
        audit_after,
        history_entry,
    }
}

struct AuditRecheckContext {
    before_failed_rules: usize,
    before_findings: usize,
    fixed_count: usize,
    skipped_count: usize,
    preservation_passed: bool,
    before_anomaly_count: usize,
}

fn build_compliance_report_internal(
    path: &Path,
    scope: StdDocScope,
    doc_type_hint: Option<&str>,
    org_profile: Option<&OrganizationProfile>,
    disabled_rule_ids: &[String],
    pack_store: &ComplianceRulePackStore,
    recheck_ctx: Option<AuditRecheckContext>,
) -> Result<ComplianceAuditReport, String> {
    if !path.is_file() {
        return Err(format!("Không tìm thấy tệp cần kiểm tra: {}", path.display()));
    }

    let raw_bytes = fs::read(path)
        .map_err(|e| format!("Không thể đọc dữ liệu tệp '{}': {}", path.display(), e))?;
    let file_size_bytes = raw_bytes.len() as u64;
    let doc_sha256 = sha256_util::sha256_hex(&raw_bytes);

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut temp_docx: Option<PathBuf> = None;
    let working_path = if ext == "doc" {
        let tmp = env::temp_dir().join(format!(
            "monword_comp_{}_{}.docx",
            std::process::id(),
            Local::now().timestamp_millis()
        ));
        system_office::word_com_convert_doc_to_docx(path, &tmp)?;
        temp_docx = Some(tmp.clone());
        tmp
    } else if ext == "docx" || ext == "docm" {
        path.to_path_buf()
    } else {
        return Err(format!(
            "Định dạng .{} chưa hỗ trợ kiểm toán tuân thủ (.docx, .docm, .doc).",
            ext
        ));
    };

    let view_res = OpenXmlPackage::open_selective_for_analysis(&working_path);
    if let Some(t) = temp_docx {
        let _ = fs::remove_file(t);
    }
    let view = view_res?;

    let mut ir = std_engine::ir::DocumentIr::build(path, &view, scope, org_profile)?;
    ir.file_ext = ext.clone();

    let pack = pack_store.load_active_pack();
    let mut effective_disabled: Vec<String> = disabled_rule_ids.to_vec();
    for r in &pack.rules {
        if !r.enabled && !effective_disabled.contains(&r.rule_id) {
            effective_disabled.push(r.rule_id.clone());
        }
    }

    let std_report = std_engine::evaluate_ir(&ir, org_profile, &effective_disabled);
    build_compliance_report_from_loaded(
        path,
        file_size_bytes,
        doc_sha256,
        ext,
        &view,
        &ir,
        &std_report,
        doc_type_hint,
        org_profile,
        &effective_disabled,
        pack_store,
        recheck_ctx,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_compliance_report_from_loaded(
    path: &Path,
    file_size_bytes: u64,
    doc_sha256: String,
    ext: String,
    view: &crate::engine::openxml_arch::SelectivePackageView,
    ir: &std_engine::ir::DocumentIr,
    std_report: &crate::models::StdAnalysisReport,
    doc_type_hint: Option<&str>,
    org_profile: Option<&OrganizationProfile>,
    disabled_rule_ids: &[String],
    pack_store: &ComplianceRulePackStore,
    recheck_ctx: Option<AuditRecheckContext>,
) -> Result<ComplianceAuditReport, String> {
    let pack = pack_store.load_active_pack();
    let mut effective_disabled: Vec<String> = disabled_rule_ids.to_vec();
    for r in &pack.rules {
        if !r.enabled && !effective_disabled.contains(&r.rule_id) {
            effective_disabled.push(r.rule_id.clone());
        }
    }

    let layout_map = layout_intelligence::build_map_from_ir(view, ir)?;
    let detected_doc_type = advanced_checks::infer_detected_doc_type(ir, doc_type_hint);

    let est_pages = layout_map.page_layout_map.len().max(1);
    let data_table_count = ir
        .tables
        .iter()
        .filter(|t| t.admin_kind == AdminTableKind::None)
        .count();
    let has_can_cu = ir
        .paragraphs
        .iter()
        .any(|p| p.role == ParagraphRole::LegalBasisCanCu);

    let matrix = matrix_store::build_compliance_matrix(
        &pack,
        ir.effective_scope,
        &detected_doc_type,
        org_profile,
        &effective_disabled,
        Some(std_report),
        est_pages,
        data_table_count,
        has_can_cu,
    );

    let matrix_by_id: HashMap<&str, &ComplianceMatrixRow> = matrix
        .iter()
        .map(|m| (m.rule_id.as_str(), m))
        .collect();
    let pack_by_id: HashMap<&str, &RuleVersionEntry> = pack
        .rules
        .iter()
        .map(|r| (r.rule_id.as_str(), r))
        .collect();

    let mut traceable_findings = Vec::with_capacity(std_report.findings.len());
    for f in &std_report.findings {
        let m_row = matrix_by_id.get(f.rule_id.as_str()).copied();
        let p_meta = pack_by_id.get(f.rule_id.as_str()).copied();
        let rule_version = m_row
            .map(|m| m.rule_version.clone())
            .unwrap_or_else(|| "1.0.0".to_string());
        let obligation = m_row
            .map(|m| m.obligation)
            .unwrap_or(ComplianceObligation::Recommended);
        let basis_kind = m_row
            .map(|m| m.basis_kind)
            .unwrap_or(ComplianceBasisKind::TechnicalRecommendation);
        let legal_basis = m_row
            .map(|m| m.legal_basis.clone())
            .unwrap_or_else(|| f.legal_basis.clone());
        let form_component = m_row
            .map(|m| m.form_component.clone())
            .unwrap_or_else(|| "general".to_string());
        let remediation_method = if let Some(pm) = p_meta {
            if !pm.remediation_guidance.trim().is_empty() && f.proposal.trim().is_empty() {
                pm.remediation_guidance.clone()
            } else if !pm.remediation_guidance.trim().is_empty() {
                format!("{} ({})", f.proposal, pm.remediation_guidance)
            } else {
                f.proposal.clone()
            }
        } else {
            f.proposal.clone()
        };

        traceable_findings.push(TraceableFinding {
            finding_id: f.finding_id.clone(),
            rule_id: f.rule_id.clone(),
            rule_name: f.rule_name.clone(),
            rule_version,
            pack_version: pack.pack_version.clone(),
            legal_basis,
            basis_kind,
            obligation,
            applicable_scope: matrix_store::doc_scope_label_vi(ir.effective_scope).to_string(),
            format_group: f.category,
            form_component,
            severity: f.severity,
            location: f.location.clone(),
            element_path: f.element_path.clone(),
            check_result: f.status,
            current_value: f.current_value.clone(),
            expected_value: f.expected_value.clone(),
            remediation_method,
            safety_tier: f.safety_tier,
            auto_fix_supported: f.auto_fix_supported,
            disposition: f.disposition,
        });
    }

    let (advanced_issues, repeated_clusters, consistency) = advanced_checks::run_advanced_checks(
        ir,
        &layout_map,
        std_report,
        &matrix,
        org_profile,
        &effective_disabled,
        &detected_doc_type,
    );

    let now_str = Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
    let audit_id = format!(
        "AUD-{}-{}",
        Local::now().format("%Y%m%d%H%M%S"),
        &doc_sha256[..6]
    );

    let mut mandatory_count = 0usize;
    let mut conditional_count = 0usize;
    let mut recommended_count = 0usize;
    let mut not_applicable_count = 0usize;
    for row in &matrix {
        match row.obligation {
            ComplianceObligation::Mandatory => mandatory_count += 1,
            ComplianceObligation::Conditional => conditional_count += 1,
            ComplianceObligation::Recommended => recommended_count += 1,
            ComplianceObligation::NotApplicable => not_applicable_count += 1,
        }
    }

    let file_info = AuditFileDigest {
        file_name: ir.file_name.clone(),
        file_ext: ext,
        file_size_bytes,
        doc_sha256: doc_sha256.clone(),
        detected_doc_type,
        requested_scope: ir.requested_scope,
        effective_scope: ir.effective_scope,
        section_count: ir.sections.len(),
        estimated_page_count: est_pages,
        paragraph_count: ir.paragraphs.len(),
        table_count: ir.tables.len(),
    };

    let rule_profile = AuditRuleProfileUsed {
        pack_id: pack.pack_id.clone(),
        pack_version: pack.pack_version.clone(),
        effective_date: pack.effective_date.clone(),
        org_profile_id: org_profile.map(|o| o.id.clone()),
        org_profile_name: org_profile.map(|o| o.profile_name.clone()),
        mandatory_count,
        conditional_count,
        recommended_count,
        not_applicable_count,
        disabled_rule_ids: effective_disabled,
    };

    let recheck = recheck_ctx.map(|ctx| {
        let after_failed_rules = matrix
            .iter()
            .filter(|m| m.evaluation_status == Some(crate::models::RuleCheckStatus::Failed))
            .count();
        let remaining_mandatory_failures = traceable_findings
            .iter()
            .filter(|f| f.obligation == ComplianceObligation::Mandatory)
            .count();
        let remaining_recommended_warnings = traceable_findings
            .iter()
            .filter(|f| f.obligation != ComplianceObligation::Mandatory)
            .count();
        let new_anomalies_introduced = advanced_issues
            .len()
            .saturating_sub(ctx.before_anomaly_count);

        AuditRecheckSummary {
            rechecked_at: now_str.clone(),
            output_path: path.to_string_lossy().to_string(),
            output_sha256: doc_sha256,
            before_failed_rules: ctx.before_failed_rules,
            after_failed_rules,
            before_findings: ctx.before_findings,
            after_findings: traceable_findings.len(),
            fixed_count: ctx.fixed_count,
            skipped_count: ctx.skipped_count,
            remaining_mandatory_failures,
            remaining_recommended_warnings,
            new_anomalies_introduced,
            preservation_passed: ctx.preservation_passed,
        }
    });

    Ok(ComplianceAuditReport {
        audit_id,
        checked_at: now_str,
        legal_disclaimer: NON_LEGAL_DISCLAIMER_VI.to_string(),
        file_info,
        rule_profile,
        matrix,
        traceable_findings,
        advanced_issues,
        repeated_clusters,
        consistency,
        recheck,
    })
}

pub fn export_compliance_report(
    report: &ComplianceAuditReport,
    format: AuditExportFormat,
    dest_path: &Path,
) -> AuditExportResult {
    exporter::export_report(report, format, dest_path)
}

pub fn compare_audit_history_entries(
    store: &AuditHistoryStore,
    older_id: &str,
    newer_id: &str,
) -> Result<AuditRunComparison, String> {
    let entries = store.list_entries()?;
    let older = entries
        .iter()
        .find(|e| e.audit_id == older_id)
        .ok_or_else(|| format!("Không tìm thấy bản ghi lịch sử '{}'.", older_id))?;
    let newer = entries
        .iter()
        .find(|e| e.audit_id == newer_id)
        .ok_or_else(|| format!("Không tìm thấy bản ghi lịch sử '{}'.", newer_id))?;
    Ok(history_store::compare_entries(older, newer))
}

pub fn list_audit_history(store: &AuditHistoryStore) -> Result<Vec<AuditHistoryEntry>, String> {
    store.list_entries()
}
