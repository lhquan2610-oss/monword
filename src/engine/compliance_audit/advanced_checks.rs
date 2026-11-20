use crate::engine::formatter::collapse_spaces_upper;
use crate::engine::std_engine::ir::{DocumentIr, ParagraphRole};
use crate::models::{
    AdvancedAuditIssue, AdvancedIssueKind, ComplianceBasisKind, ComplianceMatrixRow,
    ComplianceObligation, DocumentConsistencyMetrics, IntelligentDocumentMap, OrgSector,
    OrganizationProfile, RecognizedStructureKind, RepeatedErrorCluster, StdAnalysisReport,
    StdDocScope, StdSeverity,
};
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

fn re_doc_number_symbol() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)S[ốo]\s*:?\s*([^/\s]*)\s*/\s*([A-ZĐa-zđ0-9]+)(?:\s*-\s*([A-ZĐa-zđ0-9.\-]+))?")
            .expect("valid")
    })
}

fn re_full_date_hint() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)ngày\s+\d{1,2}\s+tháng\s+\d{1,2}\s+năm\s+\d{4}").expect("valid")
    })
}

fn re_blank_placeholder() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\[CHƯA NHẬP:[^\]]*\]|\{\{[^}]+\}\})").expect("valid"))
}

fn expected_abbr_for_doc_type_upper(upper_title: &str) -> Option<&'static str> {
    match upper_title.trim() {
        "NGHỊ QUYẾT" => Some("NQ"),
        "QUYẾT ĐỊNH" => Some("QĐ"),
        "CHỈ THỊ" => Some("CT"),
        "QUY CHẾ" => Some("QC"),
        "QUY ĐỊNH" => Some("QyĐ"),
        "THÔNG CÁO" => Some("TC"),
        "THÔNG BÁO" => Some("TB"),
        "HƯỚNG DẪN" => Some("HD"),
        "CHƯƠNG TRÌNH" => Some("CTr"),
        "KẾ HOẠCH" => Some("KH"),
        "PHƯƠNG ÁN" => Some("PA"),
        "ĐỀ ÁN" => Some("ĐA"),
        "DỰ ÁN" => Some("DA"),
        "BÁO CÁO" => Some("BC"),
        "BIÊN BẢN" => Some("BB"),
        "TỜ TRÌNH" => Some("TTr"),
        "HỢP ĐỒNG" => Some("HĐ"),
        "CÔNG ĐIỆN" => Some("CĐ"),
        _ => None,
    }
}

pub fn infer_detected_doc_type(ir: &DocumentIr, doc_type_hint: Option<&str>) -> String {
    if let Some(h) = doc_type_hint.map(str::trim).filter(|s| !s.is_empty()) {
        return h.to_string();
    }
    if let Some(dt_p) = ir
        .paragraphs
        .iter()
        .find(|p| p.role == ParagraphRole::DocTypeHeading)
    {
        let u = collapse_spaces_upper(&dt_p.trimmed);
        return match u.as_str() {
            "QUYẾT ĐỊNH" => "quyetDinh".to_string(),
            "TỜ TRÌNH" => "toTrinh".to_string(),
            "BÁO CÁO" => "baoCao".to_string(),
            "THÔNG BÁO" => "thongBao".to_string(),
            "KẾ HOẠCH" => "keHoach".to_string(),
            "BIÊN BẢN" => "bienBan".to_string(),
            "CHỈ THỊ" => "chiThi".to_string(),
            "NGHỊ QUYẾT" => "nghiQuyet".to_string(),
            "ĐỀ ÁN" => "deAn".to_string(),
            "QUY CHẾ" => "quyChe".to_string(),
            "HỢP ĐỒNG" => "hopDong".to_string(),
            other => other.to_string(),
        };
    }
    // Check header text for "/CV-" or "V/v" without DocTypeHeading -> likely CongVan
    let head_blob: String = ir
        .paragraphs
        .iter()
        .take(15)
        .map(|p| p.trimmed.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let hu = collapse_spaces_upper(&head_blob);
    if hu.contains("V/V") || hu.contains("KÍNH GỬI") || hu.contains("SỐ:") {
        if !hu.contains("/QĐ") && !hu.contains("/BC") && !hu.contains("/TTR") {
            return "congVan".to_string();
        }
    }
    "vanBanChung".to_string()
}

pub fn run_advanced_checks(
    ir: &DocumentIr,
    layout_map: &IntelligentDocumentMap,
    std_report: &StdAnalysisReport,
    matrix: &[ComplianceMatrixRow],
    org_profile: Option<&OrganizationProfile>,
    disabled_rule_ids: &[String],
    detected_doc_type: &str,
) -> (
    Vec<AdvancedAuditIssue>,
    Vec<RepeatedErrorCluster>,
    DocumentConsistencyMetrics,
) {
    let mut issues: Vec<AdvancedAuditIssue> = Vec::new();
    let scope = ir.effective_scope;
    let is_formal_admin_or_party = matches!(
        scope,
        StdDocScope::AdminNd30 | StdDocScope::PartyDocument | StdDocScope::SpecializedLegal
    ) || (scope == StdDocScope::EnterpriseInternal
        && org_profile
            .map(|o| o.use_nd30_layout_for_enterprise)
            .unwrap_or(false));

    // Extract header block text (first 16 paragraphs including header table cells)
    let header_zone_paras: Vec<_> = ir
        .paragraphs
        .iter()
        .filter(|p| p.global_index <= 16 || p.role == ParagraphRole::AdminHeaderCell)
        .collect();
    let header_joined = header_zone_paras
        .iter()
        .map(|p| p.trimmed.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let header_upper = collapse_spaces_upper(&header_joined);

    let doc_type_para = ir
        .paragraphs
        .iter()
        .find(|p| p.role == ParagraphRole::DocTypeHeading);

    // ------------------------------------------------------------------------
    // 1. CROSS-COMPONENT MISMATCHES (XCHK-*)
    // ------------------------------------------------------------------------
    let sym_caps = re_doc_number_symbol().captures(&header_joined);
    if let (Some(dt_p), Some(caps)) = (doc_type_para, &sym_caps) {
        let dt_upper = collapse_spaces_upper(&dt_p.trimmed);
        let sym_type = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        if dt_upper == "CÔNG VĂN" {
            issues.push(AdvancedAuditIssue {
                issue_id: "XCHK-01".to_string(),
                kind: AdvancedIssueKind::CrossComponentMismatch,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::LegalRegulation,
                legal_basis: "Khoản 5 Mục II Phần I Phụ lục I & Phụ lục III NĐ 30/2020/NĐ-CP"
                    .to_string(),
                title: "Công văn không trình bày dòng Tên loại văn bản độc lập".to_string(),
                explanation: format!(
                    "Văn bản trình bày dòng \"{}\" nhưng theo Phụ lục I NĐ 30/2020, Công văn chỉ đặt trích yếu \"V/v ...\" dưới Số/Ký hiệu.",
                    dt_p.trimmed
                ),
                location: format!("Đoạn #{} (Tên loại VB)", dt_p.global_index + 1),
                element_path: dt_p.element_path.clone(),
                remediation: "Bỏ dòng tiêu đề \"CÔNG VĂN\" hoặc đổi lại tên loại tương ứng."
                    .to_string(),
            });
        } else if let Some(exp_abbr) = expected_abbr_for_doc_type_upper(&dt_upper) {
            if !sym_type.eq_ignore_ascii_case(exp_abbr) {
                issues.push(AdvancedAuditIssue {
                    issue_id: "XCHK-01".to_string(),
                    kind: AdvancedIssueKind::CrossComponentMismatch,
                    severity: StdSeverity::Error,
                    basis_kind: ComplianceBasisKind::LegalRegulation,
                    legal_basis: "Khoản 3, 5 Mục II Phụ lục I & Phụ lục III NĐ 30/2020/NĐ-CP"
                        .to_string(),
                    title: "Tên loại văn bản mâu thuẫn với chữ viết tắt trong Ký hiệu".to_string(),
                    explanation: format!(
                        "Tên loại là \"{}\" (ký hiệu chuẩn \"{}\") nhưng dòng Số/Ký hiệu đang ghi \"/{}\".",
                        dt_p.trimmed,
                        exp_abbr,
                        sym_type
                    ),
                    location: "Phần đầu văn bản (Số/Ký hiệu ↔ Tên loại)".to_string(),
                    element_path: dt_p.element_path.clone(),
                    remediation: format!(
                        "Kiểm tra đồng bộ lại tên loại \"{}\" hoặc ký hiệu \"{}\" cho thống nhất.",
                        dt_p.trimmed, exp_abbr
                    ),
                });
            }
        }
    }

    if let (Some(org), Some(caps)) = (org_profile, &sym_caps) {
        let org_abbr = org.agency_abbreviation.trim();
        let sym_org = caps.get(3).map(|m| m.as_str().trim()).unwrap_or("");
        if !org_abbr.is_empty() && !sym_org.is_empty() && !sym_org.eq_ignore_ascii_case(org_abbr) {
            issues.push(AdvancedAuditIssue {
                issue_id: "XCHK-02".to_string(),
                kind: AdvancedIssueKind::CrossComponentMismatch,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::OrganizationPolicy,
                legal_basis: "Khoản 3 Mục II Phụ lục I NĐ 30/2020/NĐ-CP & Hồ sơ Tổ chức"
                    .to_string(),
                title: "Ký hiệu viết tắt cơ quan lệch với Hồ sơ Tổ chức đang chọn".to_string(),
                explanation: format!(
                    "Ký hiệu trong văn bản ghi \"-{}\" trong khi viết tắt của hồ sơ \"{}\" là \"-{}\".",
                    sym_org, org.profile_name, org_abbr
                ),
                location: "Khối Số và ký hiệu".to_string(),
                element_path: "body/header[0]".to_string(),
                remediation: format!(
                    "Xác nhận lại hồ sơ đơn vị ban hành hoặc cập nhật viết tắt về \"{}\".",
                    org_abbr
                ),
            });
        }
    }

    // XCHK-03: Body mentions "phụ lục" while DocumentMap has 0 AppendixBlock
    let mentions_appendix_in_body = ir.paragraphs.iter().any(|p| {
        p.role == ParagraphRole::BodyProse && {
            let u = collapse_spaces_upper(&p.trimmed);
            u.contains("PHỤ LỤC KÈM THEO")
                || u.contains("THEO PHỤ LỤC")
                || u.contains("TẠI PHỤ LỤC")
        }
    });
    let has_appendix_block = layout_map
        .document_map
        .iter()
        .any(|n| n.kind == RecognizedStructureKind::AppendixBlock);
    if mentions_appendix_in_body && !has_appendix_block {
        issues.push(AdvancedAuditIssue {
            issue_id: "XCHK-03".to_string(),
            kind: AdvancedIssueKind::CrossComponentMismatch,
            severity: StdSeverity::Warning,
            basis_kind: ComplianceBasisKind::TechnicalRecommendation,
            legal_basis: "Mục IV Phụ lục I NĐ 30/2020/NĐ-CP (Đối chiếu viện dẫn Phụ lục)"
                .to_string(),
            title: "Nội dung có viện dẫn Phụ lục nhưng tài liệu thiếu khối Phụ lục".to_string(),
            explanation: "Lời văn có nhắc tới phụ lục kèm theo nhưng chưa nhận diện thấy phần mở đầu \"PHỤ LỤC...\" ở cuối văn bản.".to_string(),
            location: "Lời văn chính ↔ Phần cuối văn bản".to_string(),
            element_path: "body".to_string(),
            remediation: "Bổ sung phần Phụ lục cuối văn bản hoặc tách tiêu đề \"PHỤ LỤC...\" đúng cấu trúc.".to_string(),
        });
    }

    // XCHK-04: Multi-page document (>= 2 pages) vs missing PAGE dynamic field
    let est_pages = layout_map.page_layout_map.len().max(1);
    if is_formal_admin_or_party
        && est_pages >= 2
        && !ir.headers_footers.iter().any(|hf| hf.has_page_field)
    {
        issues.push(AdvancedAuditIssue {
            issue_id: "XCHK-04".to_string(),
            kind: AdvancedIssueKind::CrossComponentMismatch,
            severity: StdSeverity::Warning,
            basis_kind: ComplianceBasisKind::LegalRegulation,
            legal_basis: "Khoản 3 Điều 9 & Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
            title: "Văn bản nhiều trang ước tính nhưng chưa có trường số trang PAGE".to_string(),
            explanation: format!(
                "Bản đồ bố cục ước tính tài liệu dài {} trang (≥ 2 trang) nhưng các phần Header/Footer không chứa trường động PAGE.",
                est_pages
            ),
            location: "Header / Footer ↔ PageLayoutMap".to_string(),
            element_path: "word/header*.xml".to_string(),
            remediation: "Đánh số trang tự động giữa trên (13–14pt, bỏ qua trang 1)."
                .to_string(),
        });
    }

    // XCHK-05: Signature authority ("TM.") vs single-head agency ("SỞ", "PHÒNG", "CỤC")
    let has_tm_sig = ir.paragraphs.iter().any(|p| {
        matches!(p.role, ParagraphRole::SignatureBlock)
            && collapse_spaces_upper(&p.trimmed).starts_with("TM.")
    });
    let is_single_head_agency = (header_upper.contains("SỞ ")
        || header_upper.contains("PHÒNG ")
        || header_upper.contains("CỤC "))
        && !header_upper.contains("UBND")
        && !header_upper.contains("ỦY BAN NHÂN DÂN")
        && !header_upper.contains("HỘI ĐỒNG");
    if is_formal_admin_or_party && has_tm_sig && is_single_head_agency {
        issues.push(AdvancedAuditIssue {
            issue_id: "XCHK-05".to_string(),
            kind: AdvancedIssueKind::CrossComponentMismatch,
            severity: StdSeverity::Information,
            basis_kind: ComplianceBasisKind::LegalRegulation,
            legal_basis: "Điều 13 & Mục II.7 Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
            title: "Đối chiếu thẩm quyền ký \"TM.\" với mô hình cơ quan thủ trưởng"
                .to_string(),
            explanation: "Phần chữ ký ghi \"TM.\" (Thay mặt tập thể) trong khi tên cơ quan ban hành có dạng đơn vị chế độ thủ trưởng (Sở/Phòng/Cục).".to_string(),
            location: "Khối Cơ quan ban hành ↔ Khối Chữ ký".to_string(),
            element_path: "body/signature".to_string(),
            remediation: "Người soạn rà soát lại quyền hạn ký theo quy chế làm việc thực tế (hệ thống không tự đổi quyền hạn ký).".to_string(),
        });
    }

    // ------------------------------------------------------------------------
    // 2. CONFIGURATION CONFLICTS (CCFG-*)
    // ------------------------------------------------------------------------
    if let Some(org) = org_profile {
        if org.sector == OrgSector::Enterprise
            && !org.use_nd30_layout_for_enterprise
            && scope == StdDocScope::AdminNd30
        {
            issues.push(AdvancedAuditIssue {
                issue_id: "CCFG-01".to_string(),
                kind: AdvancedIssueKind::ConfigurationConflict,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::OrganizationPolicy,
                legal_basis: "Cấu hình Hồ sơ Tổ chức ↔ Phạm vi Kiểm chuẩn".to_string(),
                title: "Mâu thuẫn giữa Hồ sơ Doanh nghiệp (tắt NĐ30) và Phạm vi Hành chính Nhà nước".to_string(),
                explanation: format!(
                    "Hồ sơ \"{}\" đang tắt tương thích NĐ 30/2020 nhưng phiên kiểm tra lại đang chọn phạm vi AdminNd30.",
                    org.profile_name
                ),
                location: "Cấu hình Tổ chức & Phạm vi".to_string(),
                element_path: "config/orgProfile".to_string(),
                remediation: "Chuyển phạm vi sang EnterpriseInternal hoặc bật useNd30LayoutForEnterprise.".to_string(),
            });
        }
    }

    if !disabled_rule_ids.is_empty() {
        let disabled_mandatory: Vec<&str> = matrix
            .iter()
            .filter(|m| {
                disabled_rule_ids.contains(&m.rule_id)
                    && m.basis_kind == ComplianceBasisKind::LegalRegulation
            })
            .map(|m| m.rule_id.as_str())
            .collect();
        if !disabled_mandatory.is_empty() {
            issues.push(AdvancedAuditIssue {
                issue_id: "CCFG-02".to_string(),
                kind: AdvancedIssueKind::ConfigurationConflict,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::OrganizationPolicy,
                legal_basis: "Kiểm soát cấu hình bộ quy tắc kiểm toán".to_string(),
                title: format!(
                    "{} quy tắc thể thức bị vô hiệu hóa trong phiên kiểm tra",
                    disabled_mandatory.len()
                ),
                explanation: format!(
                    "Các mã quy tắc [{}] đang bị tắt thủ công, có thể bỏ sót lỗi so với phạm vi đang áp dụng.",
                    disabled_mandatory.join(", ")
                ),
                location: "Cấu hình disabledRuleIds".to_string(),
                element_path: "config/disabledRuleIds".to_string(),
                remediation: "Bật lại các quy tắc bắt buộc khi kiểm tra phát hành chính thức."
                    .to_string(),
            });
        }
    }

    if header_upper.contains("ĐẢNG CỘNG SẢN VIỆT NAM") && scope != StdDocScope::PartyDocument {
        issues.push(AdvancedAuditIssue {
            issue_id: "CCFG-03".to_string(),
            kind: AdvancedIssueKind::ConfigurationConflict,
            severity: StdSeverity::Warning,
            basis_kind: ComplianceBasisKind::OrganizationPolicy,
            legal_basis: "Hướng dẫn 36-HD/VPTW vs Nghị định 30/2020/NĐ-CP".to_string(),
            title: "Tài liệu có tiêu ngữ Đảng nhưng đang kiểm tra ở phạm vi ngoài Văn bản Đảng"
                .to_string(),
            explanation: "Phát hiện cụm \"ĐẢNG CỘNG SẢN VIỆT NAM\" ở phần đầu nhưng phạm vi kiểm tra không phải PartyDocument.".to_string(),
            location: "Phần đầu ↔ Phạm vi kiểm tra".to_string(),
            element_path: "config/scope".to_string(),
            remediation: "Chọn phạm vi Văn bản Đảng (36-HD/VPTW) để đánh giá chuẩn xác.".to_string(),
        });
    }

    // ------------------------------------------------------------------------
    // 3. MISSING REQUIRED CONTENT & PLACEHOLDERS (MISS-*)
    // ------------------------------------------------------------------------
    if is_formal_admin_or_party {
        let has_so_num = sym_caps
            .as_ref()
            .and_then(|c| c.get(1))
            .map(|m| !m.as_str().trim().is_empty() && !m.as_str().contains('.'))
            .unwrap_or(false);
        if !header_upper.contains("SỐ:") || !has_so_num {
            issues.push(AdvancedAuditIssue {
                issue_id: "MISS-01".to_string(),
                kind: AdvancedIssueKind::MissingRequiredContent,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::LegalRegulation,
                legal_basis: "Khoản 3 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
                title: "Thiếu hoặc để trống Số và ký hiệu văn bản".to_string(),
                explanation: "Không phát hiện số phát hành đầy đủ theo cấu trúc \"Số: XX/YY-ZZ\" ở phần đầu văn bản.".to_string(),
                location: "Phần đầu văn bản".to_string(),
                element_path: "body/header/number".to_string(),
                remediation: "Cấp số văn bản và điền ký hiệu trước khi phát hành chính thức.".to_string(),
            });
        }

        if !re_full_date_hint().is_match(&header_joined) {
            issues.push(AdvancedAuditIssue {
                issue_id: "MISS-02".to_string(),
                kind: AdvancedIssueKind::MissingRequiredContent,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::LegalRegulation,
                legal_basis: "Khoản 4 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
                title: "Thiếu hoặc chưa điền đủ Địa danh và ngày, tháng, năm".to_string(),
                explanation: "Chưa nhận diện thấy ngày/tháng/năm hoàn chỉnh (4 chữ số năm) tại vùng đầu văn bản.".to_string(),
                location: "Phần đầu văn bản".to_string(),
                element_path: "body/header/date".to_string(),
                remediation: "Bổ sung đầy đủ định dạng \"<Địa danh>, ngày .. tháng .. năm 20..\"."
                    .to_string(),
            });
        }

        let has_subject = ir.paragraphs.iter().any(|p| p.role == ParagraphRole::DocSubject);
        if !has_subject && detected_doc_type != "hopDong" && detected_doc_type != "bienBan" {
            issues.push(AdvancedAuditIssue {
                issue_id: "MISS-03".to_string(),
                kind: AdvancedIssueKind::MissingRequiredContent,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::LegalRegulation,
                legal_basis: "Khoản 5 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
                title: "Thiếu trích yếu nội dung văn bản".to_string(),
                explanation: "Chưa tìm thấy dòng trích yếu nội dung sau Tên loại văn bản hoặc khối Số/Ký hiệu.".to_string(),
                location: "Khối Tên loại / Trích yếu".to_string(),
                element_path: "body/subject".to_string(),
                remediation: "Bổ sung trích yếu ngắn gọn, khái quát rõ nội dung văn bản."
                    .to_string(),
            });
        }

        let has_signature = ir
            .paragraphs
            .iter()
            .any(|p| p.role == ParagraphRole::SignatureBlock);
        if !has_signature {
            issues.push(AdvancedAuditIssue {
                issue_id: "MISS-04".to_string(),
                kind: AdvancedIssueKind::MissingRequiredContent,
                severity: StdSeverity::Error,
                basis_kind: ComplianceBasisKind::LegalRegulation,
                legal_basis: "Khoản 7 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
                title: "Thiếu khối Chữ ký của người có thẩm quyền".to_string(),
                explanation: "Không phát hiện khối chức vụ / quyền hạn ký ở phần cuối văn bản."
                    .to_string(),
                location: "Phần cuối văn bản".to_string(),
                element_path: "body/signature".to_string(),
                remediation: "Bổ sung chức vụ người ký, khoảng trống con dấu và họ tên."
                    .to_string(),
            });
        }

        let has_recipient = ir
            .paragraphs
            .iter()
            .any(|p| p.role == ParagraphRole::RecipientNoiNhan);
        if !has_recipient {
            issues.push(AdvancedAuditIssue {
                issue_id: "MISS-05".to_string(),
                kind: AdvancedIssueKind::MissingRequiredContent,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::LegalRegulation,
                legal_basis: "Khoản 8 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
                title: "Thiếu khối Nơi nhận văn bản".to_string(),
                explanation: "Văn bản hành chính/Đảng không có thành phần \"Nơi nhận:\" ở cuối văn bản.".to_string(),
                location: "Phần cuối văn bản".to_string(),
                element_path: "body/recipients".to_string(),
                remediation: "Bổ sung khối Nơi nhận (cỡ chữ 11–12pt, cuối cùng là \"- Lưu: VT, ...\").".to_string(),
            });
        }
    }

    // MISS-06: Unfilled placeholder tokens ([CHƯA NHẬP: ...] or {{...}}) across ANY scope
    let placeholder_paras: Vec<_> = ir
        .paragraphs
        .iter()
        .filter(|p| re_blank_placeholder().is_match(&p.trimmed))
        .collect();
    if !placeholder_paras.is_empty() {
        let first = placeholder_paras[0];
        issues.push(AdvancedAuditIssue {
            issue_id: "MISS-06".to_string(),
            kind: AdvancedIssueKind::MissingRequiredContent,
            severity: StdSeverity::Error,
            basis_kind: ComplianceBasisKind::TechnicalRecommendation,
            legal_basis: "Kiểm tra hoàn thiện dữ liệu trường động trước phát hành".to_string(),
            title: format!(
                "Còn {} đoạn chứa trường dữ liệu chưa điền",
                placeholder_paras.len()
            ),
            explanation: format!(
                "Phát hiện thẻ đánh dấu chưa nhập liệu tại {} và {} vị trí khác.",
                first.element_path,
                placeholder_paras.len().saturating_sub(1)
            ),
            location: format!("Đoạn #{}", first.global_index + 1),
            element_path: first.element_path.clone(),
            remediation: "Điền đủ toàn bộ dữ liệu động còn thiếu trước khi phát hành.".to_string(),
        });
    }

    // ------------------------------------------------------------------------
    // 4. STRUCTURAL ANOMALIES (ANOM-*) & WHOLE-DOCUMENT CONSISTENCY
    // ------------------------------------------------------------------------
    let heading_levels: Vec<(u8, &str, usize)> = layout_map
        .paragraph_map
        .iter()
        .filter(|p| p.in_table_id.is_none() && p.in_text_box_id.is_none())
        .filter_map(|p| {
            let lvl = match p.kind {
                RecognizedStructureKind::HeadingLevel1 => 1,
                RecognizedStructureKind::HeadingLevel2 => 2,
                RecognizedStructureKind::HeadingLevel3 => 3,
                RecognizedStructureKind::HeadingLevel4 => 4,
                _ => return None,
            };
            Some((lvl, p.locator.element_path.as_str(), p.global_index))
        })
        .collect();

    let mut heading_hierarchy_valid = true;
    for pair in heading_levels.windows(2) {
        let (l1, _, _) = pair[0];
        let (l2, path2, g2) = pair[1];
        if l2 > l1 + 1 {
            heading_hierarchy_valid = false;
            issues.push(AdvancedAuditIssue {
                issue_id: format!("ANOM-H-{}", g2 + 1),
                kind: AdvancedIssueKind::StructuralAnomaly,
                severity: StdSeverity::Warning,
                basis_kind: ComplianceBasisKind::TechnicalRecommendation,
                legal_basis: "Tính phân cấp logic hệ thống đề mục tài liệu".to_string(),
                title: format!("Nhảy cóc cấp tiêu đề (Cấp {} -> Cấp {})", l1, l2),
                explanation: format!(
                    "Tiêu đề tại đoạn #{} nhảy từ cấp {} xuống thẳng cấp {} làm đứt mạch Mục lục.",
                    g2 + 1,
                    l1,
                    l2
                ),
                location: format!("Đoạn #{}", g2 + 1),
                element_path: path2.to_string(),
                remediation: format!(
                    "Bổ sung đề mục cấp {} trung gian hoặc điều chỉnh lại cấp tiêu đề.",
                    l1 + 1
                ),
            });
            break;
        }
    }

    // Consecutive non-table prose overflowing table/image count from Layout Map
    let overflow_count = layout_map
        .table_map
        .iter()
        .filter(|t| t.overflows_width && t.table_role == "dataTable")
        .count()
        + layout_map
            .visual_objects
            .iter()
            .filter(|v| v.overflows_printable_area)
            .count();
    if overflow_count > 0 {
        issues.push(AdvancedAuditIssue {
            issue_id: "ANOM-OVERFLOW".to_string(),
            kind: AdvancedIssueKind::StructuralAnomaly,
            severity: StdSeverity::Error,
            basis_kind: ComplianceBasisKind::TechnicalRecommendation,
            legal_basis: "Bố cục trang in & Layout Intelligence (Phase 5)".to_string(),
            title: format!("{} đối tượng bảng/ảnh vượt khổ vùng in", overflow_count),
            explanation: "Có bảng dữ liệu hoặc hình ảnh tràn khỏi giới hạn lề trang của Section."
                .to_string(),
            location: "Layout Map (Bảng / Hình ảnh)".to_string(),
            element_path: "body".to_string(),
            remediation: "Dùng chức năng Phân tích Bố cục để co bảng/ảnh hoặc xoay trang ngang."
                .to_string(),
        });
    }

    // Repeated formatting error clusters (>= 3 occurrences of same rule + expected target)
    let mut grouped: BTreeMap<(String, String), (String, Vec<String>)> = BTreeMap::new();
    for f in &std_report.findings {
        let key = (f.rule_id.clone(), f.expected_value.clone());
        let entry = grouped
            .entry(key)
            .or_insert_with(|| (f.rule_name.clone(), Vec::new()));
        entry.1.push(f.location.clone());
    }
    let rule_ver_map: HashMap<&str, &str> = matrix
        .iter()
        .map(|m| (m.rule_id.as_str(), m.rule_version.as_str()))
        .collect();
    let mut repeated_clusters = Vec::new();
    let mut cluster_seq = 0usize;
    for ((rule_id, expected_val), (rule_name, locs)) in grouped {
        if locs.len() >= 3 {
            cluster_seq += 1;
            repeated_clusters.push(RepeatedErrorCluster {
                cluster_id: format!("CLU-{:02}", cluster_seq),
                rule_id: rule_id.clone(),
                rule_name,
                rule_version: rule_ver_map
                    .get(rule_id.as_str())
                    .copied()
                    .unwrap_or("1.0.0")
                    .to_string(),
                occurrence_count: locs.len(),
                sample_locations: locs.into_iter().take(5).collect(),
                systemic_cause: format!(
                    "Lỗi lặp lại nhiều đoạn cùng hướng chuẩn \"{}\", thường do dán văn bản đa nguồn hoặc lệch Style mặc định.",
                    expected_val
                ),
                recommended_batch_action: format!(
                    "Chạy Auto-Fix hàng loạt cho quy tắc {} thay vì sửa thủ công từng đoạn.",
                    rule_id
                ),
            });
        }
    }

    // Consistency metrics over non-empty body prose paragraphs
    let body_prose: Vec<_> = ir
        .paragraphs
        .iter()
        .filter(|p| p.role == ParagraphRole::BodyProse && p.in_table_idx.is_none() && !p.trimmed.is_empty())
        .collect();
    let (font_uniformity_pct, size_uniformity_pct, spacing_uniformity_pct) =
        compute_body_uniformity(&body_prose);

    let cross_component_consistent = !issues
        .iter()
        .any(|i| i.kind == AdvancedIssueKind::CrossComponentMismatch);

    // Purely technical presentation index (0..100), explicitly NOT legal validity!
    let mandatory_fails = std_report
        .findings
        .iter()
        .filter(|f| {
            matrix
                .iter()
                .find(|m| m.rule_id == f.rule_id)
                .map(|m| m.obligation == ComplianceObligation::Mandatory)
                .unwrap_or(false)
        })
        .count();
    let rec_fails = std_report.findings.len().saturating_sub(mandatory_fails);
    let penalty = (mandatory_fails * 6 + rec_fails * 2 + issues.len() * 4).min(95);
    let technical_quality_index = (100usize.saturating_sub(penalty)) as u8;

    let consistency = DocumentConsistencyMetrics {
        font_uniformity_pct,
        size_uniformity_pct,
        spacing_uniformity_pct,
        heading_hierarchy_valid,
        cross_component_consistent,
        technical_quality_index,
    };

    (issues, repeated_clusters, consistency)
}

fn compute_body_uniformity(paras: &[&crate::engine::std_engine::ir::ParagraphInfo]) -> (f64, f64, f64) {
    if paras.is_empty() {
        return (100.0, 100.0, 100.0);
    }
    let mut font_counts: HashMap<String, usize> = HashMap::new();
    let mut size_counts: HashMap<i32, usize> = HashMap::new();
    let mut spacing_counts: HashMap<(i32, i32), usize> = HashMap::new();

    for p in paras {
        let f = if p.primary_font.is_empty() {
            "NormalDefault".to_string()
        } else {
            p.primary_font.to_ascii_lowercase()
        };
        *font_counts.entry(f).or_insert(0) += 1;

        let sz_half = p.primary_size_pt.map(|s| (s * 2.0).round() as i32).unwrap_or(28);
        *size_counts.entry(sz_half).or_insert(0) += 1;

        let line_bucket = (p.line_spacing / 20) * 20;
        let before_bucket = (p.spacing_before / 40) * 40;
        *spacing_counts.entry((line_bucket, before_bucket)).or_insert(0) += 1;
    }

    let n = paras.len() as f64;
    let round1 = |v: f64| (v * 10.0).round() / 10.0;
    let top_font = font_counts.values().copied().max().unwrap_or(0) as f64;
    let top_size = size_counts.values().copied().max().unwrap_or(0) as f64;
    let top_space = spacing_counts.values().copied().max().unwrap_or(0) as f64;

    (
        round1((top_font / n) * 100.0),
        round1((top_size / n) * 100.0),
        round1((top_space / n) * 100.0),
    )
}
