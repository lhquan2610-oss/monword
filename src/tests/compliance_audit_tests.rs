use crate::engine::compliance_audit::{
    compare_audit_history_entries, execute_autofix_with_recheck, export_compliance_report,
    preview_compliance_matrix, reset_rule_pack, run_compliance_audit, save_rule_pack,
    AuditHistoryStore, ComplianceRulePackStore, NON_LEGAL_DISCLAIMER_VI,
};
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::models::{
    AuditExportFormat, ComplianceAuditReport, ComplianceAuditRequest,
    ComplianceAutoFixAndRecheckRequest, ComplianceBasisKind, ComplianceObligation, OrgSector,
    OrganizationProfile, OutputMode, RuleCheckStatus, StdDocScope, StdExecutionRequest,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::CompressionMethod;

fn make_temp_audit_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "monword_audit_test_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn build_audit_docx(
    path: &Path,
    body_inner_xml: &str,
    sect_pr_xml: Option<&str>,
) {
    let mut pkg = OpenXmlPackage {
        entries: Vec::new(),
    };

    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
</Types>"#;
    pkg.set_bytes(
        "[Content_Types].xml",
        content_types.as_bytes().to_vec(),
        CompressionMethod::Deflated,
    );

    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    pkg.set_bytes(
        "_rels/.rels",
        root_rels.as_bytes().to_vec(),
        CompressionMethod::Deflated,
    );

    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
</Relationships>"#;
    pkg.set_bytes(
        "word/_rels/document.xml.rels",
        doc_rels.as_bytes().to_vec(),
        CompressionMethod::Deflated,
    );

    let default_sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    {}
    {}
  </w:body>
</w:document>"#,
        body_inner_xml,
        sect_pr_xml.unwrap_or(default_sect)
    );
    pkg.set_bytes(
        "word/document.xml",
        doc_xml.into_bytes(),
        CompressionMethod::Deflated,
    );

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Heading1"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Heading2"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/><w:i/><w:sz w:val="26"/></w:rPr></w:style>
</w:styles>"#;
    pkg.set_bytes(
        "word/styles.xml",
        styles_xml.as_bytes().to_vec(),
        CompressionMethod::Deflated,
    );

    let settings_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
    pkg.set_bytes(
        "word/settings.xml",
        settings_xml.as_bytes().to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.save_to(path).unwrap();
}

fn tnr_run(text: &str, sz_hp: &str, extra_rpr: &str) -> String {
    format!(
        r#"<w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/>{}<w:sz w:val="{}"/><w:szCs w:val="{}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        extra_rpr, sz_hp, sz_hp, text
    )
}

fn custom_font_run(font: &str, text: &str, sz_hp: &str, extra_rpr: &str) -> String {
    format!(
        r#"<w:r><w:rPr><w:rFonts w:ascii="{}" w:hAnsi="{}" w:cs="{}"/>{}<w:sz w:val="{}"/><w:szCs w:val="{}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        font, font, font, extra_rpr, sz_hp, sz_hp, text
    )
}

fn encode_ps_utf16_b64(script: &str) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let utf16_bytes: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let mut out = String::with_capacity((utf16_bytes.len() + 2) / 3 * 4);
    for chunk in utf16_bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn state_agency_profile(abbr: &str) -> OrganizationProfile {
    OrganizationProfile {
        id: "org-state-test".to_string(),
        profile_name: "Sở Tài nguyên và Môi trường (Test)".to_string(),
        sector: OrgSector::StateAgency,
        governing_org: "ỦY BAN NHÂN DÂN TỈNH".to_string(),
        issuing_org: "SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG".to_string(),
        agency_abbreviation: abbr.to_string(),
        default_location: "Hà Nội".to_string(),
        use_nd30_layout_for_enterprise: true,
        ..Default::default()
    }
}

#[test]
fn audit_case_01_compliant_document_and_non_legal_disclaimer() {
    let dir = make_temp_audit_dir("c01_compliant");
    let src = dir.join("compliant_nd30.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    let body = format!(
        r#"
      <w:tbl>
        <w:tr>
          <w:tc>
            <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
            <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
          </w:tc>
          <w:tc>
            <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
            <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
            <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
          </w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:before="120" w:after="60"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:before="60" w:after="120"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:tbl>
        <w:tr>
          <w:tc><w:p>{}</w:p><w:p>{}</w:p></w:tc>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
    "#,
        tnr_run("SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG", "26", "<w:b/>"),
        tnr_run("Số: 05/QĐ-STNMT", "26", ""),
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Độc lập - Tự do - Hạnh phúc", "28", "<w:b/>"),
        tnr_run("Hà Nội, ngày 15 tháng 03 năm 2026", "26", "<w:i/>"),
        tnr_run("QUYẾT ĐỊNH", "28", "<w:b/>"),
        tnr_run("Về việc ban hành Quy chế chuẩn hóa văn bản điện tử", "28", "<w:b/>"),
        tnr_run("Căn cứ Luật Tổ chức chính quyền địa phương ngày 19 tháng 6 năm 2015;", "28", "<w:i/>"),
        tnr_run("Ủy ban nhân dân và các đơn vị trực thuộc triển khai thực hiện thống nhất theo quy định.", "28", ""),
        tnr_run("Nơi nhận:", "24", "<w:b/><w:i/>"),
        tnr_run("- Lưu: VT, HC.", "22", ""),
        tnr_run("GIÁM ĐỐC", "28", "<w:b/>"),
        tnr_run("Nguyễn Văn An", "28", "<w:b/>"),
    );
    build_audit_docx(&src, &body, None);

    let req = ComplianceAuditRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        doc_type_hint: Some("quyetDinh".to_string()),
        org_profile: Some(state_agency_profile("STNMT")),
        disabled_rule_ids: Vec::new(),
        record_history: true,
    };
    let report = run_compliance_audit(&req, &pack_store, &history_store).unwrap();

    assert_eq!(report.file_info.effective_scope, StdDocScope::AdminNd30);
    assert_eq!(report.file_info.doc_sha256.len(), 64);
    assert_eq!(report.legal_disclaimer, NON_LEGAL_DISCLAIMER_VI);
    assert!(report.legal_disclaimer.contains("KHÔNG phải"));

    // Must separate Mandatory legal rules vs Recommended technical rules
    assert!(report.rule_profile.mandatory_count > 0);
    assert!(report.rule_profile.recommended_count > 0);
    assert!(report
        .matrix
        .iter()
        .any(|m| m.obligation == ComplianceObligation::Mandatory && m.basis_kind == ComplianceBasisKind::LegalRegulation));
    assert!(report
        .matrix
        .iter()
        .any(|m| m.obligation == ComplianceObligation::Recommended && m.basis_kind == ComplianceBasisKind::TechnicalRecommendation));

    let mandatory_failures: Vec<_> = report
        .traceable_findings
        .iter()
        .filter(|f| f.obligation == ComplianceObligation::Mandatory && f.check_result == RuleCheckStatus::Failed)
        .collect();
    assert!(
        mandatory_failures.is_empty(),
        "Văn bản NĐ30 hợp lệ không được có lỗi Bắt buộc: {:?}",
        mandatory_failures
    );

    // Report payload outside disclaimer must NEVER claim document is legally valid
    let stripped_json = serde_json::to_string(&report)
        .unwrap()
        .replace(NON_LEGAL_DISCLAIMER_VI, "");
    let lower = stripped_json.to_lowercase();
    assert!(!lower.contains("có giá trị pháp lý"));
    assert!(!lower.contains("văn bản hợp pháp"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_02_multi_error_traceability_and_repeated_clusters() {
    let dir = make_temp_audit_dir("c02_trace_cluster");
    let src = dir.join("multi_error.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    let wrong_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
    "#,
        custom_font_run("Arial", "CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "24", "<w:b/>"),
        custom_font_run("Arial", "Đoạn nội dung thứ nhất sai font Arial cỡ 12pt và không căn đều hai bên.", "24", ""),
        custom_font_run("Arial", "Đoạn nội dung thứ hai tiếp tục lặp lại lỗi font Arial cỡ 12pt.", "24", ""),
        custom_font_run("Arial", "Đoạn nội dung thứ ba lặp lại định dạng sai chuẩn cần gom cụm.", "24", ""),
        custom_font_run("Arial", "Đoạn nội dung thứ tư lặp lại cùng một mã quy tắc.", "24", ""),
    );
    build_audit_docx(&src, &body, Some(wrong_sect));

    let req = ComplianceAuditRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        record_history: false,
        ..Default::default()
    };
    let report = run_compliance_audit(&req, &pack_store, &history_store).unwrap();

    assert!(report.traceable_findings.len() >= 5);
    for f in &report.traceable_findings {
        assert!(!f.finding_id.is_empty());
        assert!(!f.rule_id.is_empty());
        assert!(!f.rule_name.is_empty());
        assert!(!f.rule_version.is_empty());
        assert!(!f.pack_version.is_empty());
        assert!(!f.legal_basis.is_empty());
        assert!(!f.applicable_scope.is_empty());
        assert!(!f.location.is_empty());
        assert!(!f.element_path.is_empty());
        assert!(!f.remediation_method.is_empty());
        assert_ne!(f.check_result, RuleCheckStatus::Passed);
    }

    assert!(
        !report.repeated_clusters.is_empty(),
        "Phải gom cụm lỗi định dạng lặp lại từ >= 3 đoạn"
    );
    assert!(report
        .repeated_clusters
        .iter()
        .any(|c| c.occurrence_count >= 3 && !c.sample_locations.is_empty() && !c.rule_version.is_empty()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_03_not_applicable_rules_and_obligation_separation() {
    let dir = make_temp_audit_dir("c03_not_applicable");
    let pack_store = ComplianceRulePackStore::with_root(&dir);

    let ent_no_nd30 = OrganizationProfile {
        id: "org-ent-flex".to_string(),
        profile_name: "Công ty TNHH Công nghệ ABC".to_string(),
        sector: OrgSector::Enterprise,
        use_nd30_layout_for_enterprise: false,
        ..Default::default()
    };

    let matrix_flex = preview_compliance_matrix(
        &pack_store,
        StdDocScope::EnterpriseInternal,
        "thongBao",
        Some(&ent_no_nd30),
    );
    for nd30_only in ["PAGE-02", "PARA-01", "PARA-02", "PGNUM-01", "CAP-01", "ABBR-01", "STRUCT-01"] {
        let row = matrix_flex.iter().find(|r| r.rule_id == nd30_only).unwrap();
        assert_eq!(
            row.obligation,
            ComplianceObligation::NotApplicable,
            "Doanh nghiệp tắt NĐ30 phải xếp {} vào NotApplicable",
            nd30_only
        );
        assert_eq!(row.is_condition_met_on_doc, Some(false));
    }

    let ent_with_nd30 = OrganizationProfile {
        use_nd30_layout_for_enterprise: true,
        ..ent_no_nd30
    };
    let matrix_nd30_ent = preview_compliance_matrix(
        &pack_store,
        StdDocScope::EnterpriseInternal,
        "quyetDinh",
        Some(&ent_with_nd30),
    );
    for cond_rule in ["PAGE-02", "PARA-01", "PARA-02"] {
        let row = matrix_nd30_ent.iter().find(|r| r.rule_id == cond_rule).unwrap();
        assert_eq!(row.obligation, ComplianceObligation::Conditional);
        assert_eq!(row.is_condition_met_on_doc, Some(true));
    }

    let matrix_freeform =
        preview_compliance_matrix(&pack_store, StdDocScope::Freeform, "vanBanChung", None);
    for state_rule in ["PAGE-01", "PAGE-02", "PARA-01", "STRUCT-01", "ABBR-01"] {
        let row = matrix_freeform.iter().find(|r| r.rule_id == state_rule).unwrap();
        assert_eq!(
            row.obligation,
            ComplianceObligation::NotApplicable,
            "Tài liệu tự do không áp dụng quy tắc công quyền {}",
            state_rule
        );
    }
    let page03_row = matrix_freeform.iter().find(|r| r.rule_id == "PAGE-03").unwrap();
    assert_eq!(page03_row.obligation, ComplianceObligation::Recommended);
    let preserve_row = matrix_freeform.iter().find(|r| r.rule_id == "PRESERVE-01").unwrap();
    assert_eq!(preserve_row.obligation, ComplianceObligation::Mandatory);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_04_missing_content_cross_checks_config_conflicts_and_anomalies() {
    let dir = make_temp_audit_dir("c04_advanced");
    let src_admin = dir.join("advanced_anomalies.docx");
    let src_party_sig = dir.join("party_and_tm.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    // Doc 1: XCHK-01 (BÁO CÁO vs /QĐ-), XCHK-02 (-ABC vs XYZ), XCHK-03 (mentions Phụ lục with no AppendixBlock),
    // CCFG-01 (Enterprise use_nd30=false + AdminNd30), CCFG-02 (disabled PAGE-01),
    // MISS-02 (missing full date), MISS-03 (no DocSubject), MISS-04 (no signature), MISS-05 (no Nơi nhận), MISS-06 (placeholder),
    // ANOM-H-* (Heading1 -> Heading3 jump)
    let body1 = format!(
        r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:pStyle w:val="Heading3"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
    "#,
        tnr_run("SỞ TÀI CHÍNH", "26", "<w:b/>"),
        tnr_run("Số: 12/QĐ-ABC", "26", ""),
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Độc lập - Tự do - Hạnh phúc", "28", "<w:b/>"),
        tnr_run("BÁO CÁO", "28", "<w:b/>"),
        tnr_run("I. PHẦN THỨ NHẤT", "30", "<w:b/>"),
        tnr_run("1.1. Mục nhảy cóc trực tiếp xuống cấp 3", "26", "<w:b/><w:i/>"),
        tnr_run("Số liệu tổng hợp được trình bày chi tiết tại Phụ lục kèm theo văn bản này.", "28", ""),
        tnr_run("Người phụ trách: [CHƯA NHẬP: Họ và tên] và mã {{ma_ho_so}}.", "28", ""),
    );
    build_audit_docx(&src_admin, &body1, None);

    let conflicting_org = OrganizationProfile {
        id: "org-ent-conflict".to_string(),
        profile_name: "Công ty XYZ".to_string(),
        sector: OrgSector::Enterprise,
        agency_abbreviation: "XYZ".to_string(),
        use_nd30_layout_for_enterprise: false,
        ..Default::default()
    };
    let req1 = ComplianceAuditRequest {
        path: src_admin.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        org_profile: Some(conflicting_org),
        disabled_rule_ids: vec!["PAGE-01".to_string()],
        record_history: false,
        ..Default::default()
    };
    let rep1 = run_compliance_audit(&req1, &pack_store, &history_store).unwrap();
    let ids1: Vec<&str> = rep1.advanced_issues.iter().map(|i| i.issue_id.as_str()).collect();

    for expected_id in ["XCHK-01", "XCHK-02", "XCHK-03", "CCFG-01", "CCFG-02", "MISS-02", "MISS-03", "MISS-04", "MISS-05", "MISS-06"] {
        assert!(
            ids1.contains(&expected_id),
            "Thiếu cảnh báo nâng cao {} trong {:?}",
            expected_id,
            ids1
        );
    }
    assert!(
        ids1.iter().any(|id| id.starts_with("ANOM-H-")),
        "Phải phát hiện nhảy cấp tiêu đề ANOM-H-* trong {:?}",
        ids1
    );
    assert!(!rep1.consistency.heading_hierarchy_valid);
    assert!(!rep1.consistency.cross_component_consistent);

    // Doc 2: XCHK-05 (single-head agency "SỞ ..." + "TM."), CCFG-03 ("ĐẢNG CỘNG SẢN VIỆT NAM" checked as AdminNd30)
    let body2 = format!(
        r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/></w:pPr>{}</w:p>
      <w:tbl>
        <w:tr>
          <w:tc><w:p>{}</w:p></w:tc>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
    "#,
        tnr_run("ĐẢNG CỘNG SẢN VIỆT NAM", "26", "<w:b/>"),
        tnr_run("SỞ GIÁO DỤC VÀ ĐÀO TẠO", "26", "<w:b/>"),
        tnr_run("Số: /CV-SGD", "26", ""),
        tnr_run("Hà Nội, ngày 01 tháng 02 năm 2026", "26", "<w:i/>"),
        tnr_run("Văn bản kiểm tra chéo tiêu ngữ Đảng và thẩm quyền ký TM.", "28", ""),
        tnr_run("Nơi nhận:", "24", "<w:b/><w:i/>"),
        tnr_run("TM. SỞ GIÁO DỤC", "28", "<w:b/>"),
    );
    build_audit_docx(&src_party_sig, &body2, None);

    let req2 = ComplianceAuditRequest {
        path: src_party_sig.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        record_history: false,
        ..Default::default()
    };
    let rep2 = run_compliance_audit(&req2, &pack_store, &history_store).unwrap();
    let ids2: Vec<&str> = rep2.advanced_issues.iter().map(|i| i.issue_id.as_str()).collect();
    assert!(ids2.contains(&"XCHK-05"), "Thiếu XCHK-05 trong {:?}", ids2);
    assert!(ids2.contains(&"CCFG-03"), "Thiếu CCFG-03 trong {:?}", ids2);
    assert!(ids2.contains(&"MISS-01"), "Thiếu MISS-01 (trống số phát hành) trong {:?}", ids2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_05_conditional_rules_1page_vs_multipage_and_congvan() {
    let dir = make_temp_audit_dir("c05_conditional");
    let one_page_cv = dir.join("one_page_congvan.docx");
    let multi_page_qd = dir.join("multi_page_quyetdinh.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    let body_1p = format!(
        r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/></w:pPr>{}</w:p>
    "#,
        tnr_run("SỞ XÂY DỰNG", "26", "<w:b/>"),
        tnr_run("Số: 10/SXD-VP", "26", ""),
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Độc lập - Tự do - Hạnh phúc", "28", "<w:b/>"),
        tnr_run("V/v hướng dẫn triển khai kế hoạch ngắn hạn", "26", ""),
        tnr_run("Công văn một trang không có bảng dữ liệu.", "28", ""),
    );
    build_audit_docx(&one_page_cv, &body_1p, None);

    let rep_1p = run_compliance_audit(
        &ComplianceAuditRequest {
            path: one_page_cv.to_string_lossy().to_string(),
            scope: StdDocScope::AdminNd30,
            doc_type_hint: Some("congVan".to_string()),
            record_history: false,
            ..Default::default()
        },
        &pack_store,
        &history_store,
    )
    .unwrap();
    assert_eq!(rep_1p.file_info.estimated_page_count, 1);
    for cid in ["PGNUM-01", "TBL-01", "TBL-03", "ABBR-01"] {
        let row = rep_1p.matrix.iter().find(|r| r.rule_id == cid).unwrap();
        assert_eq!(row.obligation, ComplianceObligation::Conditional);
        assert_eq!(
            row.is_condition_met_on_doc,
            Some(false),
            "Văn bản Công văn 1 trang không bảng phải có {} is_condition_met_on_doc=false",
            cid
        );
    }

    // Multi-page Quyet Dinh (>= 60 long paragraphs + 1 data table, missing header page field)
    let mut long_paras = String::new();
    for i in 1..=65 {
        long_paras.push_str(&format!(
            r#"<w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>"#,
            tnr_run(
                &format!("Điều {}. Nội dung quy định chi tiết về quy trình chuẩn hóa thể thức và kiểm toán tài liệu dài nhiều trang nhằm đảm bảo ước tính phân trang vượt ngưỡng một trang A4 theo tiêu chuẩn hiển thị thực tế.", i),
                "28",
                ""
            )
        ));
    }
    let body_mp = format!(
        r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p>{}</w:p><w:p>{}</w:p></w:tc>
          <w:tc><w:p>{}</w:p><w:p>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      {}
      <w:tbl>
        <w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="6000"/></w:tblGrid>
        <w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>
        <w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>
      </w:tbl>
    "#,
        tnr_run("BỘ XÂY DỰNG", "26", "<w:b/>"),
        tnr_run("Số: 88/QĐ-BXD", "26", ""),
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Hà Nội, ngày 20 tháng 05 năm 2026", "26", "<w:i/>"),
        tnr_run("QUYẾT ĐỊNH", "28", "<w:b/>"),
        long_paras,
        tnr_run("STT", "26", "<w:b/>"),
        tnr_run("Hạng mục kiểm tra", "26", "<w:b/>"),
        tnr_run("1", "26", ""),
        tnr_run("Khổ giấy A4 &amp; Số trang", "26", ""),
    );
    build_audit_docx(&multi_page_qd, &body_mp, None);

    let rep_mp = run_compliance_audit(
        &ComplianceAuditRequest {
            path: multi_page_qd.to_string_lossy().to_string(),
            scope: StdDocScope::AdminNd30,
            doc_type_hint: Some("quyetDinh".to_string()),
            record_history: false,
            ..Default::default()
        },
        &pack_store,
        &history_store,
    )
    .unwrap();
    assert!(rep_mp.file_info.estimated_page_count >= 2);
    for cid in ["PGNUM-01", "TBL-01", "TBL-03", "ABBR-01"] {
        let row = rep_mp.matrix.iter().find(|r| r.rule_id == cid).unwrap();
        assert_eq!(row.obligation, ComplianceObligation::Conditional);
        assert_eq!(
            row.is_condition_met_on_doc,
            Some(true),
            "Quyết định nhiều trang có bảng phải kích hoạt điều kiện {} = true",
            cid
        );
    }
    assert!(rep_mp.advanced_issues.iter().any(|i| i.issue_id == "XCHK-04"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_06_hot_update_rule_pack_version_without_engine_rewrite() {
    let dir = make_temp_audit_dir("c06_rule_pack_hot_update");
    let src = dir.join("pack_test.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    let wrong_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"<w:p><w:pPr><w:jc w:val="both"/></w:pPr>{}</w:p>"#,
        tnr_run("Văn bản kiểm tra cập nhật phiên bản bộ quy tắc.", "28", "")
    );
    build_audit_docx(&src, &body, Some(wrong_sect));

    let mut custom_pack = ComplianceRulePackStore::builtin_pack();
    custom_pack.pack_version = "2027.2-amendment".to_string();
    for r in &mut custom_pack.rules {
        if r.rule_id == "PAGE-01" {
            r.rule_version = "2.1.0".to_string();
            r.legal_citation = "Nghị định sửa đổi 2027 (Hot-Update Test)".to_string();
        } else if r.rule_id == "FONT-05" {
            r.enabled = false;
        }
    }
    save_rule_pack(&pack_store, &custom_pack).unwrap();

    let rep = run_compliance_audit(
        &ComplianceAuditRequest {
            path: src.to_string_lossy().to_string(),
            scope: StdDocScope::AdminNd30,
            record_history: true,
            ..Default::default()
        },
        &pack_store,
        &history_store,
    )
    .unwrap();

    assert_eq!(rep.rule_profile.pack_version, "2027.2-amendment");
    let p01_row = rep.matrix.iter().find(|r| r.rule_id == "PAGE-01").unwrap();
    assert_eq!(p01_row.rule_version, "2.1.0");
    assert_eq!(p01_row.legal_basis, "Nghị định sửa đổi 2027 (Hot-Update Test)");

    let f05_row = rep.matrix.iter().find(|r| r.rule_id == "FONT-05").unwrap();
    assert_eq!(f05_row.obligation, ComplianceObligation::NotApplicable);

    let p01_finding = rep
        .traceable_findings
        .iter()
        .find(|f| f.rule_id == "PAGE-01")
        .unwrap();
    assert_eq!(p01_finding.rule_version, "2.1.0");
    assert_eq!(p01_finding.pack_version, "2027.2-amendment");
    assert_eq!(p01_finding.legal_basis, "Nghị định sửa đổi 2027 (Hot-Update Test)");

    let entries = history_store.list_entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].pack_version, "2027.2-amendment");

    let reset_p = reset_rule_pack(&pack_store).unwrap();
    assert_eq!(reset_p.pack_version, ComplianceRulePackStore::builtin_pack().pack_version);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_07_audit_history_privacy_no_sensitive_content_and_comparison() {
    let dir = make_temp_audit_dir("c07_history_privacy");
    let doc_path = dir.join("hop_dong_noi_bo.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    let secret_body = "BI_MAT_KINH_DOANH_987654321";
    let secret_signer = "NGUYEN_VAN_MAT_001";

    // Version 1: wrong font Arial + US Letter page size + sensitive text
    let v1_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body_v1 = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/></w:pPr>{}</w:p>
    "#,
        custom_font_run("Arial", &format!("Nội dung mật: {}", secret_body), "24", ""),
        custom_font_run("Arial", &format!("Người ký: {}", secret_signer), "24", ""),
    );
    build_audit_docx(&doc_path, &body_v1, Some(v1_sect));

    let rep1 = run_compliance_audit(
        &ComplianceAuditRequest {
            path: doc_path.to_string_lossy().to_string(),
            scope: StdDocScope::AdminNd30,
            record_history: true,
            ..Default::default()
        },
        &pack_store,
        &history_store,
    )
    .unwrap();

    // Version 2: fix to Times New Roman 14pt + A4 while retaining same sensitive text
    let body_v2 = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
    "#,
        tnr_run(&format!("Nội dung mật: {}", secret_body), "28", ""),
        tnr_run(&format!("Người ký: {}", secret_signer), "28", ""),
    );
    build_audit_docx(&doc_path, &body_v2, None);

    let rep2 = run_compliance_audit(
        &ComplianceAuditRequest {
            path: doc_path.to_string_lossy().to_string(),
            scope: StdDocScope::AdminNd30,
            record_history: true,
            ..Default::default()
        },
        &pack_store,
        &history_store,
    )
    .unwrap();

    let raw_history = fs::read_to_string(dir.join("history.json")).unwrap();
    assert!(
        !raw_history.contains(secret_body),
        "history.json TUYỆT ĐỐI KHÔNG được lưu văn bản nhạy cảm!"
    );
    assert!(
        !raw_history.contains(secret_signer),
        "history.json TUYỆT ĐỐI KHÔNG được lưu tên người ký!"
    );

    let entries = history_store.list_entries().unwrap();
    assert_eq!(entries.len(), 2);
    // Most recent entry inserted at index 0
    let newer = &entries[0];
    let older = &entries[1];
    assert_eq!(older.audit_id, rep1.audit_id);
    assert_eq!(newer.audit_id, rep2.audit_id);
    assert_ne!(older.doc_sha256, newer.doc_sha256);
    assert!(older.doc_version_label.starts_with("Bản #1"));
    assert!(newer.doc_version_label.starts_with("Bản #2"));
    assert!(older
        .detected_findings
        .iter()
        .all(|f| !f.structural_locator.is_empty()));

    let cmp = compare_audit_history_entries(&history_store, &older.audit_id, &newer.audit_id).unwrap();
    assert!(!cmp.same_document_hash);
    assert!(!cmp.resolved_rule_locations.is_empty());
    assert!(cmp.quality_index_delta >= 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_08_autofix_and_post_fix_recheck_integration() {
    let dir = make_temp_audit_dir("c08_autofix_recheck");
    let src = dir.join("needs_autofix.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    let wrong_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
    "#,
        custom_font_run("Arial", "QUYẾT ĐỊNH", "28", "<w:b/>"),
        custom_font_run("Arial", "Đoạn nội dung thứ nhất cần tự động chuẩn hóa font và căn lề.", "24", ""),
        custom_font_run("Arial", "Đoạn nội dung thứ hai cần giữ nguyên 100% chữ sau sửa.", "24", ""),
    );
    build_audit_docx(&src, &body, Some(wrong_sect));

    let pre = run_compliance_audit(
        &ComplianceAuditRequest {
            path: src.to_string_lossy().to_string(),
            scope: StdDocScope::AdminNd30,
            record_history: false,
            ..Default::default()
        },
        &pack_store,
        &history_store,
    )
    .unwrap();
    let skipped: Vec<String> = pre
        .traceable_findings
        .iter()
        .find(|f| f.rule_id == "PARA-05")
        .map(|f| vec![f.finding_id.clone()])
        .unwrap_or_default();

    let res = execute_autofix_with_recheck(
        &ComplianceAutoFixAndRecheckRequest {
            exec_request: StdExecutionRequest {
                path: src.to_string_lossy().to_string(),
                scope: StdDocScope::AdminNd30,
                output_mode: OutputMode::NewCopy,
                apply_all_group_a: true,
                skipped_finding_ids: skipped.clone(),
                ..Default::default()
            },
            doc_type_hint: Some("quyetDinh".to_string()),
            record_history: true,
        },
        &pack_store,
        &history_store,
    );

    assert!(res.std_execution.success, "Err: {:?}", res.std_execution.error);
    assert!(res.audit_before.is_some());
    let after = res.audit_after.as_ref().unwrap();
    let recheck = after.recheck.as_ref().expect("Phải có tóm tắt hậu kiểm");

    assert!(recheck.fixed_count >= 1);
    assert!(recheck.after_findings < recheck.before_findings);
    assert!(recheck.preservation_passed);
    assert_eq!(recheck.new_anomalies_introduced, 0);
    assert_eq!(recheck.skipped_count, skipped.len());

    let hist = res.history_entry.as_ref().expect("Phải lưu lịch sử sau Auto-Fix");
    assert!(hist.recheck.is_some());
    assert!(!hist.fixed_finding_ids.is_empty());
    assert_eq!(hist.skipped_finding_ids, skipped);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_case_09_export_docx_pdf_csv_and_real_microsoft_word_com() {
    let dir = make_temp_audit_dir("c09_export_word_com");
    let src = dir.join("export_source.docx");
    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let history_store = AuditHistoryStore::with_root(&dir);

    let wrong_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:pStyle w:val="Heading3"/></w:pPr>{}</w:p>
    "#,
        tnr_run("BÁO CÁO", "28", "<w:b/>"),
        custom_font_run("Arial", "Số: 09/QĐ-TEST", "24", ""),
        custom_font_run("Arial", "Đoạn sai font Arial cần kiểm toán và xuất báo cáo.", "24", ""),
        custom_font_run("Arial", "Chi tiết tại Phụ lục kèm theo [CHƯA NHẬP: Mã số].", "24", ""),
        tnr_run("I. PHẦN MỞ ĐẦU", "30", "<w:b/>"),
        tnr_run("1.1. Mục nhảy cấp", "26", "<w:b/><w:i/>"),
    );
    build_audit_docx(&src, &body, Some(wrong_sect));

    let res = execute_autofix_with_recheck(
        &ComplianceAutoFixAndRecheckRequest {
            exec_request: StdExecutionRequest {
                path: src.to_string_lossy().to_string(),
                scope: StdDocScope::AdminNd30,
                output_mode: OutputMode::NewCopy,
                apply_all_group_a: true,
                ..Default::default()
            },
            doc_type_hint: None,
            record_history: true,
        },
        &pack_store,
        &history_store,
    );
    assert!(res.std_execution.success);
    let report = res.audit_after.unwrap();

    // 1. CSV (UTF-8 BOM + Disclaimer)
    let csv_path = dir.join("audit_report.csv");
    let csv_res = export_compliance_report(&report, AuditExportFormat::Csv, &csv_path);
    assert!(csv_res.success, "CSV export err: {:?}", csv_res.error);
    let csv_bytes = fs::read(&csv_path).unwrap();
    assert_eq!(&csv_bytes[..3], &[0xEF, 0xBB, 0xBF], "CSV phải có UTF-8 BOM");
    let csv_text = String::from_utf8_lossy(&csv_bytes[3..]);
    assert!(csv_text.contains(NON_LEGAL_DISCLAIMER_VI));
    assert!(csv_text.contains("MaQuyTac,PhienBanQT"));

    // 2. JSON round-trip
    let json_path = dir.join("audit_report.json");
    let json_res = export_compliance_report(&report, AuditExportFormat::Json, &json_path);
    assert!(json_res.success, "JSON export err: {:?}", json_res.error);
    let parsed_rep: ComplianceAuditReport =
        serde_json::from_str(&fs::read_to_string(&json_path).unwrap()).unwrap();
    assert_eq!(parsed_rep.audit_id, report.audit_id);
    assert_eq!(parsed_rep.file_info.doc_sha256, report.file_info.doc_sha256);
    assert_eq!(parsed_rep.legal_disclaimer, NON_LEGAL_DISCLAIMER_VI);

    // 3. DOCX + real Microsoft Word 16.0 COM open verification
    let docx_path = dir.join("audit_report.docx");
    let docx_res = export_compliance_report(&report, AuditExportFormat::Docx, &docx_path);
    assert!(docx_res.success, "DOCX export err: {:?}", docx_res.error);
    assert!(docx_res.size_bytes > 2_000);
    let _pkg_check = OpenXmlPackage::open(&docx_path).expect("DOCX báo cáo phải là OOXML hợp lệ");

    // 4. PDF via Word COM
    let pdf_path = dir.join("audit_report.pdf");
    let pdf_res = export_compliance_report(&report, AuditExportFormat::Pdf, &pdf_path);
    assert!(pdf_res.success, "PDF export err: {:?}", pdf_res.error);
    let pdf_bytes = fs::read(&pdf_path).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"), "PDF xuất ra phải đúng chuẩn %PDF-");
    assert!(pdf_bytes.len() > 5_000);

    // Open exported DOCX directly inside real Word.Application COM and verify tables + disclaimer
    let ps = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $word.AutomationSecurity = 3
  $d = $word.Documents.Open('{}', $false, $true)
  try {{
    $txt = $d.Content.Text
    if (-not $txt.Contains('BÁO CÁO KIỂM TOÁN TUÂN THỦ')) {{ throw 'Thieu tieu de bao cao trong DOCX' }}
    if (-not $txt.Contains('KIỂM TRA CHÉO')) {{ throw 'Thieu muc kiem tra cheo trong DOCX' }}
    if (-not $txt.Contains('KẾT QUẢ KIỂM TRA LẠI SAU TỰ ĐỘNG SỬA')) {{ throw 'Thieu muc recheck trong DOCX' }}
    if (-not $txt.Contains('KHÔNG phải là kết luận thẩm định tính hợp pháp')) {{ throw 'Thieu tuyen bo mien tru phap ly trong DOCX' }}
    if ($d.Tables.Count -lt 1) {{ throw "Thieu bang thong tin trong bao cao: $($d.Tables.Count)" }}
    Write-Output ("WORD_OK|audit_report.docx|tbl=" + $d.Tables.Count + "|len=" + $txt.Length)
  }} finally {{
    $d.Close([ref]$false)
  }}
}} finally {{
  if ($word -ne $null) {{
    $word.Quit([ref]$false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
  }}
}}
"#,
        docx_path.to_string_lossy().replace('\'', "''")
    );
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encode_ps_utf16_b64(&ps),
        ])
        .output()
        .expect("PowerShell Word COM thất bại");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("WORD_OK|"),
        "Microsoft Word COM kiểm tra file báo cáo DOCX thất bại: stdout={} stderr={}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    println!("{}", stdout.trim());

    let _ = fs::remove_dir_all(&dir);
}
