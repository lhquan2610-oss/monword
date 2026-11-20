use crate::engine::std_engine;
use crate::models::{
    ComplianceBasisKind, ComplianceMatrixRow, ComplianceObligation, ComplianceRulePack,
    OrgSector, OrganizationProfile, RuleVersionEntry, StdAnalysisReport, StdDocScope,
    StdRuleDescriptor,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const BUILTIN_PACK_JSON: &str = include_str!("../../../ui/src/data/complianceRulePack.json");

#[derive(Debug, Clone)]
pub struct ComplianceRulePackStore {
    override_path: PathBuf,
}

impl ComplianceRulePackStore {
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
            override_path: base
                .join("MonWord")
                .join("compliance")
                .join("rule_pack_override.json"),
        }
    }

    #[allow(dead_code)]
    pub fn with_root(root: &Path) -> Self {
        Self {
            override_path: root.join("rule_pack_override.json"),
        }
    }

    pub fn builtin_pack() -> ComplianceRulePack {
        serde_json::from_str::<ComplianceRulePack>(BUILTIN_PACK_JSON)
            .expect("complianceRulePack.json phai hop le")
    }

    pub fn load_active_pack(&self) -> ComplianceRulePack {
        if let Ok(raw) = fs::read_to_string(&self.override_path) {
            let cleaned = raw.trim_start_matches('\u{feff}');
            if let Ok(pack) = serde_json::from_str::<ComplianceRulePack>(cleaned) {
                if !pack.rules.is_empty() {
                    return pack;
                }
            }
        }
        Self::builtin_pack()
    }

    pub fn save_pack_override(&self, pack: &ComplianceRulePack) -> Result<ComplianceRulePack, String> {
        if pack.pack_version.trim().is_empty() || pack.rules.is_empty() {
            return Err(
                "[ERR_IPC_INVALID] Bộ quy tắc phải có phiên bản và ít nhất 1 quy tắc hợp lệ."
                    .to_string(),
            );
        }
        crate::engine::reliability::safe_save::save_json_atomic(&self.override_path, pack)?;
        Ok(pack.clone())
    }

    pub fn reset_to_builtin(&self) -> Result<ComplianceRulePack, String> {
        if self.override_path.is_file() {
            fs::remove_file(&self.override_path)
                .map_err(|e| format!("Không thể xóa bản ghi đè quy tắc: {}", e))?;
        }
        Ok(Self::builtin_pack())
    }
}

pub fn scope_key(scope: StdDocScope) -> &'static str {
    match scope {
        StdDocScope::AdminNd30 | StdDocScope::AutoDetect => "adminNd30",
        StdDocScope::PartyDocument => "partyDocument",
        StdDocScope::EnterpriseInternal => "enterpriseInternal",
        StdDocScope::SpecializedLegal => "specializedLegal",
        StdDocScope::AcademicThesis => "academicThesis",
        StdDocScope::Freeform | StdDocScope::UnknownRequiresSelection => "freeform",
    }
}

pub fn doc_scope_label_vi(scope: StdDocScope) -> &'static str {
    match scope {
        StdDocScope::AutoDetect => "Tự động nhận diện",
        StdDocScope::AdminNd30 => "Văn bản hành chính (NĐ 30/2020/NĐ-CP)",
        StdDocScope::EnterpriseInternal => "Văn bản nội bộ Doanh nghiệp",
        StdDocScope::SpecializedLegal => "Văn bản quy phạm / chuyên ngành",
        StdDocScope::AcademicThesis => "Luận văn / Báo cáo học thuật",
        StdDocScope::PartyDocument => "Văn bản Đảng (36-HD/VPTW)",
        StdDocScope::Freeform => "Tài liệu tự do",
        StdDocScope::UnknownRequiresSelection => "Chưa xác định phạm vi",
    }
}

pub fn build_compliance_matrix(
    pack: &ComplianceRulePack,
    scope: StdDocScope,
    detected_doc_type: &str,
    org_profile: Option<&OrganizationProfile>,
    disabled_rule_ids: &[String],
    std_report: Option<&StdAnalysisReport>,
    estimated_page_count: usize,
    data_table_count: usize,
    has_legal_basis_can_cu: bool,
) -> Vec<ComplianceMatrixRow> {
    let std_catalog = std_engine::get_std_rules_catalog();
    let pack_by_id: HashMap<&str, &RuleVersionEntry> = pack
        .rules
        .iter()
        .map(|r| (r.rule_id.as_str(), r))
        .collect();
    let disabled_set: HashSet<&str> = disabled_rule_ids.iter().map(String::as_str).collect();
    let eval_by_id: HashMap<&str, _> = std_report
        .map(|r| {
            r.rule_summaries
                .iter()
                .map(|s| (s.rule_id.as_str(), s))
                .collect()
        })
        .unwrap_or_default();

    let skey = scope_key(scope);
    let org_ent_nd30 = org_profile
        .map(|o| o.sector == OrgSector::Enterprise && o.use_nd30_layout_for_enterprise)
        .unwrap_or(false);

    let mut rows = Vec::with_capacity(std_catalog.len());
    for desc in &std_catalog {
        let pack_meta = pack_by_id.get(desc.rule_id.as_str()).copied();
        let rule_version = pack_meta
            .map(|m| m.rule_version.clone())
            .unwrap_or_else(|| "1.0.0".to_string());
        let form_component = pack_meta
            .map(|m| m.form_component.clone())
            .unwrap_or_else(|| "general".to_string());
        let basis_kind = pack_meta
            .map(|m| m.basis_kind)
            .unwrap_or(ComplianceBasisKind::TechnicalRecommendation);
        let legal_basis = pack_meta
            .map(|m| m.legal_citation.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| desc.legal_basis.clone());
        let condition_detail = pack_meta
            .map(|m| m.condition_description.clone())
            .unwrap_or_else(|| desc.description.clone());

        let is_enabled = !disabled_set.contains(desc.rule_id.as_str())
            && pack_meta.map(|m| m.enabled).unwrap_or(true);

        let mut obligation = if !is_enabled {
            ComplianceObligation::NotApplicable
        } else if scope == StdDocScope::UnknownRequiresSelection {
            let scope_agnostic = matches!(
                desc.rule_id.as_str(),
                "FONT-01" | "FONT-05" | "PAGE-03" | "PGNUM-02" | "PRESERVE-01"
            );
            if scope_agnostic {
                pack_meta
                    .and_then(|m| m.default_obligations.get(skey).copied())
                    .unwrap_or(ComplianceObligation::Recommended)
            } else {
                ComplianceObligation::Conditional
            }
        } else {
            pack_meta
                .and_then(|m| m.default_obligations.get(skey).copied())
                .unwrap_or(ComplianceObligation::NotApplicable)
        };

        // Respect Enterprise Organization Profile configuration without imposing state decrees when disabled
        if is_enabled && scope == StdDocScope::EnterpriseInternal && !org_ent_nd30 {
            if matches!(
                desc.rule_id.as_str(),
                "PAGE-02" | "PARA-01" | "PARA-02" | "PGNUM-01" | "CAP-01" | "ABBR-01" | "STRUCT-01"
            ) {
                obligation = ComplianceObligation::NotApplicable;
            }
        }

        // Document type exception check (e.g., CongVan has no centered DocType heading or type abbr)
        let is_doc_type_excepted = pack_meta
            .map(|m| {
                m.doc_type_exceptions
                    .iter()
                    .any(|ex| ex.eq_ignore_ascii_case(detected_doc_type))
            })
            .unwrap_or(false);

        let is_condition_met_on_doc = if obligation == ComplianceObligation::NotApplicable {
            Some(false)
        } else if obligation == ComplianceObligation::Conditional {
            match desc.rule_id.as_str() {
                "PGNUM-01" => Some(estimated_page_count >= 2),
                "TBL-01" | "TBL-03" => Some(data_table_count > 0),
                "STRUCT-01" => Some(!is_doc_type_excepted || has_legal_basis_can_cu),
                "ABBR-01" => Some(!is_doc_type_excepted),
                _ => {
                    if scope == StdDocScope::EnterpriseInternal {
                        Some(org_ent_nd30)
                    } else {
                        Some(true)
                    }
                }
            }
        } else {
            None
        };

        let eval = eval_by_id.get(desc.rule_id.as_str()).copied();
        let org_config_note = match org_profile {
            Some(o) if o.sector == OrgSector::Enterprise => {
                if o.use_nd30_layout_for_enterprise {
                    format!("Hồ sơ \"{}\" bật tương thích NĐ30", o.profile_name)
                } else {
                    format!("Hồ sơ \"{}\" dùng chuẩn nội bộ linh hoạt", o.profile_name)
                }
            }
            Some(o) => format!("Hồ sơ \"{}\"", o.profile_name),
            None => "Mặc định hệ thống".to_string(),
        };

        rows.push(ComplianceMatrixRow {
            rule_id: desc.rule_id.clone(),
            rule_name: desc.rule_name.clone(),
            rule_version,
            format_group: desc.category,
            form_component,
            basis_kind,
            legal_basis,
            applicable_doc_types: applicable_scope_labels(desc),
            scope,
            org_config_note,
            obligation,
            condition_detail,
            is_condition_met_on_doc,
            evaluation_status: eval.map(|e| e.status),
            finding_count: eval.map(|e| e.finding_count).unwrap_or(0),
            auto_fix_supported: desc.auto_fix_supported,
        });
    }

    rows
}

fn applicable_scope_labels(desc: &StdRuleDescriptor) -> Vec<String> {
    desc.applicable_document_types
        .iter()
        .map(|s| doc_scope_label_vi(*s).to_string())
        .collect()
}
