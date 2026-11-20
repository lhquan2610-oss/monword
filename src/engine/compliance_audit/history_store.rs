use crate::models::{
    AppliedRuleRecord, AuditHistoryEntry, AuditRunComparison, ComplianceAuditReport,
    FindingDisposition, RedactedFindingSummary,
};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_HISTORY_ENTRIES: usize = 300;

#[derive(Debug, Clone)]
pub struct AuditHistoryStore {
    file_path: PathBuf,
}

impl AuditHistoryStore {
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
            file_path: base
                .join("MonWord")
                .join("audit_history")
                .join("history.json"),
        }
    }

    #[allow(dead_code)]
    pub fn with_root(root: &Path) -> Self {
        Self {
            file_path: root.join("history.json"),
        }
    }

    pub fn list_entries(&self) -> Result<Vec<AuditHistoryEntry>, String> {
        if !self.file_path.is_file() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&self.file_path)
            .map_err(|e| format!("Không thể đọc tệp lịch sử kiểm tra: {}", e))?;
        let cleaned = raw.trim_start_matches('\u{feff}');
        serde_json::from_str::<Vec<AuditHistoryEntry>>(cleaned)
            .map_err(|e| format!("Dữ liệu lịch sử kiểm tra bị hỏng JSON: {}", e))
    }

    fn save_entries(&self, entries: &[AuditHistoryEntry]) -> Result<(), String> {
        crate::engine::reliability::safe_save::save_json_atomic(&self.file_path, &entries)?;
        Ok(())
    }

    /// Converts a report into a **privacy-redacted** history entry and persists it.
    /// NEVER stores document body text, excerpts, signer names, or document number values!
    pub fn record_report(
        &self,
        report: &ComplianceAuditReport,
        fixed_finding_ids: Vec<String>,
        skipped_finding_ids: Vec<String>,
    ) -> Result<AuditHistoryEntry, String> {
        let mut entries = self.list_entries().unwrap_or_default();

        // Assign monotonic document version label based on unique doc_sha256 for this filename
        let mut distinct_hashes: Vec<String> = Vec::new();
        for e in entries
            .iter()
            .filter(|x| x.file_name.eq_ignore_ascii_case(&report.file_info.file_name))
        {
            if !distinct_hashes.contains(&e.doc_sha256) {
                distinct_hashes.push(e.doc_sha256.clone());
            }
        }
        let doc_ver_num = match distinct_hashes.iter().position(|h| h == &report.file_info.doc_sha256) {
            Some(pos) => pos + 1,
            None => distinct_hashes.len() + 1,
        };
        let short_hash: String = report.file_info.doc_sha256.chars().take(8).collect();
        let doc_version_label = format!("Bản #{} (sha:{})", doc_ver_num, short_hash);

        let applied_rules: Vec<AppliedRuleRecord> = report
            .matrix
            .iter()
            .filter_map(|m| {
                m.evaluation_status.map(|st| AppliedRuleRecord {
                    rule_id: m.rule_id.clone(),
                    rule_version: m.rule_version.clone(),
                    obligation: m.obligation,
                    status: st,
                    finding_count: m.finding_count,
                })
            })
            .collect();

        // Redact text payloads: keep ONLY structural locator (`section · element_path`), rule ID, severity, disposition
        let detected_findings: Vec<RedactedFindingSummary> = report
            .traceable_findings
            .iter()
            .map(|f| {
                let disp = if fixed_finding_ids.contains(&f.finding_id) {
                    FindingDisposition::AutoFixed
                } else if skipped_finding_ids.contains(&f.finding_id) {
                    FindingDisposition::Skipped
                } else {
                    f.disposition
                };
                RedactedFindingSummary {
                    finding_id: f.finding_id.clone(),
                    rule_id: f.rule_id.clone(),
                    rule_version: f.rule_version.clone(),
                    obligation: f.obligation,
                    severity: f.severity,
                    structural_locator: if f.element_path.trim().is_empty() {
                        "document-level".to_string()
                    } else {
                        f.element_path.clone()
                    },
                    disposition: disp,
                }
            })
            .collect();

        let advanced_issue_codes: Vec<String> = report
            .advanced_issues
            .iter()
            .map(|i| i.issue_id.clone())
            .collect();

        let entry = AuditHistoryEntry {
            audit_id: report.audit_id.clone(),
            checked_at: report.checked_at.clone(),
            file_name: report.file_info.file_name.clone(),
            file_size_bytes: report.file_info.file_size_bytes,
            doc_sha256: report.file_info.doc_sha256.clone(),
            doc_version_label,
            pack_id: report.rule_profile.pack_id.clone(),
            pack_version: report.rule_profile.pack_version.clone(),
            effective_scope: report.file_info.effective_scope,
            detected_doc_type: report.file_info.detected_doc_type.clone(),
            technical_quality_index: report.consistency.technical_quality_index,
            applied_rules,
            detected_findings,
            advanced_issue_codes,
            fixed_finding_ids,
            skipped_finding_ids,
            recheck: report.recheck.clone(),
        };

        entries.insert(0, entry.clone());
        if entries.len() > MAX_HISTORY_ENTRIES {
            entries.truncate(MAX_HISTORY_ENTRIES);
        }
        self.save_entries(&entries)?;
        Ok(entry)
    }

    pub fn delete_entry(&self, audit_id: &str) -> Result<Vec<AuditHistoryEntry>, String> {
        let mut entries = self.list_entries()?;
        entries.retain(|e| e.audit_id != audit_id);
        self.save_entries(&entries)?;
        Ok(entries)
    }

    pub fn clear_all(&self) -> Result<(), String> {
        self.save_entries(&[])
    }
}

pub fn compare_entries(
    older: &AuditHistoryEntry,
    newer: &AuditHistoryEntry,
) -> AuditRunComparison {
    let key_of = |f: &RedactedFindingSummary| format!("{}@{}", f.rule_id, f.structural_locator);
    let old_set: BTreeSet<String> = older.detected_findings.iter().map(key_of).collect();
    let new_set: BTreeSet<String> = newer.detected_findings.iter().map(key_of).collect();

    let resolved_rule_locations: Vec<String> = old_set.difference(&new_set).cloned().collect();
    let persistent_rule_locations: Vec<String> = old_set.intersection(&new_set).cloned().collect();
    let newly_detected_rule_locations: Vec<String> = new_set.difference(&old_set).cloned().collect();

    let old_vers: HashMap<&str, &str> = older
        .applied_rules
        .iter()
        .map(|r| (r.rule_id.as_str(), r.rule_version.as_str()))
        .collect();
    let mut rule_version_diffs = Vec::new();
    for nr in &newer.applied_rules {
        if let Some(&ov) = old_vers.get(nr.rule_id.as_str()) {
            if ov != nr.rule_version {
                rule_version_diffs.push(format!(
                    "{}: v{} -> v{}",
                    nr.rule_id, ov, nr.rule_version
                ));
            }
        }
    }

    AuditRunComparison {
        older_audit_id: older.audit_id.clone(),
        newer_audit_id: newer.audit_id.clone(),
        older_checked_at: older.checked_at.clone(),
        newer_checked_at: newer.checked_at.clone(),
        same_document_hash: older.doc_sha256 == newer.doc_sha256,
        older_doc_sha256: older.doc_sha256.clone(),
        newer_doc_sha256: newer.doc_sha256.clone(),
        pack_version_changed: older.pack_version != newer.pack_version,
        older_pack_version: older.pack_version.clone(),
        newer_pack_version: newer.pack_version.clone(),
        quality_index_delta: newer.technical_quality_index as i16
            - older.technical_quality_index as i16,
        resolved_rule_locations,
        persistent_rule_locations,
        newly_detected_rule_locations,
        skipped_count_delta: newer.skipped_finding_ids.len() as isize
            - older.skipped_finding_ids.len() as isize,
        rule_version_diffs,
    }
}
