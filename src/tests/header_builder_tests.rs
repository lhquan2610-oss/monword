use crate::engine::header_builder::{
    applicable_rules_for, apply_header_builder_to_file, create_standalone_header_docx,
    get_header_rules_catalog, inspect_file_header, restore_initial_header_backup,
};
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::org_profile::{
    delete_org_profile_at, import_org_profiles_json_at, load_org_profiles_from,
    upsert_org_profile_at,
};
use crate::models::{
    AdminDocType, AuditSeverity, HeaderActionMode, HeaderBuilderConfig, HeaderClassification,
    HeaderDateMode, HeaderNumberMode, OrgSector, OrganizationProfile, OutputMode, RuleObligation,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::CompressionMethod;

fn make_temp_hb_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "monword_hb_test_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
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

fn build_fixture_docx(
    path: &Path,
    body_inner_xml: &str,
    extra_entries: Vec<(&str, Vec<u8>)>,
    extra_rels: &str,
    extra_ct: &str,
    macro_enabled: bool,
    omit_default_sect_pr: bool,
) {
    let mut pkg = OpenXmlPackage {
        entries: Vec::new(),
    };

    let doc_main_ct = if macro_enabled {
        "application/vnd.ms-word.document.macroEnabled.main+xml"
    } else {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
    };

    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  <Override PartName="/word/document.xml" ContentType="{}"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
  {}{}
</Types>"#,
        doc_main_ct,
        if macro_enabled {
            r#"<Override PartName="/word/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/>"#
        } else {
            ""
        },
        extra_ct
    );
    pkg.set_bytes(
        "[Content_Types].xml",
        content_types.into_bytes(),
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

    let doc_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
  {}{}
</Relationships>"#,
        if macro_enabled {
            r#"<Relationship Id="rId3" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/>"#
        } else {
            ""
        },
        extra_rels
    );
    pkg.set_bytes(
        "word/_rels/document.xml.rels",
        doc_rels.into_bytes(),
        CompressionMethod::Deflated,
    );

    let default_sect = if omit_default_sect_pr {
        ""
    } else {
        r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#
    };

    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
  <w:body>
    {}
    {}
  </w:body>
</w:document>"#,
        body_inner_xml, default_sect
    );
    pkg.set_bytes(
        "word/document.xml",
        doc_xml.into_bytes(),
        CompressionMethod::Deflated,
    );

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
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

    for (entry_name, bytes) in extra_entries {
        pkg.set_bytes(entry_name, bytes, CompressionMethod::Deflated);
    }

    pkg.save_to(path).unwrap();
}

fn sample_state_org() -> OrganizationProfile {
    OrganizationProfile {
        id: "org_test_state".to_string(),
        profile_name: "Sở TNMT Thử nghiệm".to_string(),
        sector: OrgSector::StateAgency,
        governing_org: "ỦY BAN NHÂN DÂN TỈNH ĐỒNG NAI".to_string(),
        issuing_org: "SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG".to_string(),
        affiliated_unit: "".to_string(),
        department_or_branch: "".to_string(),
        agency_abbreviation: "STNMT".to_string(),
        default_location: "Đồng Nai".to_string(),
        use_nd30_layout_for_enterprise: false,
        show_governing_org_default: true,
        show_affiliated_unit_default: false,
        show_department_default: false,
        updated_at: String::new(),
    }
}

fn minimal_valid_png_1x1() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

#[test]
fn case_01_org_profile_crud_and_empty_start() {
    let dir = make_temp_hb_dir("c01_org_crud");
    let store = dir.join("org_profiles.json");

    // 1. Fresh store must be empty (no fake invented organizations)
    assert!(load_org_profiles_from(&store).is_empty());

    // 2. Reject empty issuing_org
    let mut bad = OrganizationProfile::default();
    bad.profile_name = "Trống tên CQ".to_string();
    assert!(upsert_org_profile_at(&store, bad).is_err());

    // 3. Upsert StateAgency + PartyOrg
    let org1 = sample_state_org();
    let list1 = upsert_org_profile_at(&store, org1.clone()).unwrap();
    assert_eq!(list1.len(), 1);
    assert_eq!(list1[0].issuing_org, "SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG");

    let mut org2 = OrganizationProfile {
        id: "org_test_party".to_string(),
        profile_name: "Đảng ủy Khối".to_string(),
        sector: OrgSector::PartyOrg,
        governing_org: "TỈNH ỦY ĐỒNG NAI".to_string(),
        issuing_org: "ĐẢNG ỦY KHỐI CƠ QUAN".to_string(),
        default_location: "Biên Hòa".to_string(),
        show_governing_org_default: true,
        ..Default::default()
    };
    let list2 = upsert_org_profile_at(&store, org2.clone()).unwrap();
    assert_eq!(list2.len(), 2);

    // 4. Update existing profile
    org2.default_location = "Đồng Nai".to_string();
    let list3 = upsert_org_profile_at(&store, org2).unwrap();
    assert_eq!(list3.len(), 2);
    assert_eq!(
        list3.iter().find(|p| p.id == "org_test_party").unwrap().default_location,
        "Đồng Nai"
    );

    // 5. Import JSON
    let import_json = r#"[{"id":"org_imp_1","profileName":"Công ty Mẫu Nhập","sector":"enterprise","issuingOrg":"CÔNG TY CỔ PHẦN ABC","agencyAbbreviation":"ABC","defaultLocation":"Huế","useNd30LayoutForEnterprise":false}]"#;
    let list4 = import_org_profiles_json_at(&store, import_json).unwrap();
    assert_eq!(list4.len(), 3);

    // 6. Delete
    let list5 = delete_org_profile_at(&store, "org_test_party").unwrap();
    assert_eq!(list5.len(), 2);
    assert!(list5.iter().all(|p| p.id != "org_test_party"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_02_create_new_header_on_unheaded_and_standalone_docx() {
    let dir = make_temp_hb_dir("c02_create_new");
    let src = dir.join("unheaded.docx");
    let footer_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Footer trang gốc</w:t></w:r></w:p></w:ftr>"#;
    let body = r#"
      <w:p><w:r><w:t>Kính gửi: Các phòng ban trực thuộc.</w:t></w:r></w:p>
      <w:p><w:r><w:t>Nội dung báo cáo công tác tháng 11 vẫn phải giữ nguyên vẹn.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(
        &src,
        body,
        vec![("word/footer1.xml", footer_xml.as_bytes().to_vec())],
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>"#,
        r#"<Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#,
        false,
        false,
    );

    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.org_profile_id = Some(org.id.clone());
    cfg.doc_type = AdminDocType::CongVan;
    cfg.show_governing_org = true;
    cfg.number_mode = HeaderNumberMode::ManualInput;
    cfg.doc_number = "15".to_string();
    cfg.doc_symbol = format!("{}-{}", cfg.doc_type.symbol_prefix(), org.agency_abbreviation);
    cfg.date_mode = HeaderDateMode::ManualDate;
    cfg.manual_day = "05".to_string();
    cfg.manual_month = "02".to_string();
    cfg.manual_year = "2026".to_string();
    cfg.subject_text = "V/v triển khai nhiệm vụ năm 2026".to_string();

    let insp = inspect_file_header(&src, &cfg, Some(&org)).unwrap();
    assert_eq!(insp.classification, HeaderClassification::MissingComponents);
    assert!(insp.can_create_new);
    assert!(insp.has_header_footer);

    let res = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::CreateNew, Some(&org));
    assert!(res.success, "Lỗi tạo mới phần đầu: {:?}", res.error);

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains("<w:tbl>"));
    assert!(doc_xml.contains("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM"));
    assert!(doc_xml.contains("Độc lập - Tự do - Hạnh phúc"));
    assert!(doc_xml.contains("ỦY BAN NHÂN DÂN TỈNH ĐỒNG NAI"));
    assert!(doc_xml.contains("SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG"));
    assert!(doc_xml.contains("15/CV-STNMT"));
    assert!(doc_xml.contains("Đồng Nai, ngày 05 tháng 02 năm 2026"));
    assert!(doc_xml.contains("V/v triển khai nhiệm vụ năm 2026"));
    assert!(doc_xml.contains("Kính gửi: Các phòng ban trực thuộc."));
    assert!(doc_xml.contains("Nội dung báo cáo công tác tháng 11 vẫn phải giữ nguyên vẹn."));
    assert!(out_pkg.has_entry("word/footer1.xml"));

    // Also test standalone brand-new docx creation
    let standalone = dir.join("standalone_new.docx");
    let res_st = create_standalone_header_docx(&standalone, &cfg, Some(&org));
    assert!(res_st.success, "Lỗi tạo tệp standalone: {:?}", res_st.error);
    assert!(standalone.exists());
    assert_eq!(res_st.classification_after, HeaderClassification::Valid);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_03_standardize_existing_header_typo_and_dashed_lines() {
    let dir = make_temp_hb_dir("c03_typo_dash");
    let src = dir.join("typo_dash.docx");
    let body = r#"
      <w:tbl>
        <w:tr>
          <w:tc>
            <w:p><w:r><w:t>UBND TỈNH ĐỒNG NAI</w:t></w:r></w:p>
            <w:p><w:r><w:t>SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG</w:t></w:r></w:p>
            <w:p><w:r><w:t>------------</w:t></w:r></w:p>
            <w:p><w:r><w:t>Số: 22/CV-STNMT</w:t></w:r></w:p>
          </w:tc>
          <w:tc>
            <w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHŨ NGHĨA VIỆT NAM</w:t></w:r></w:p>
            <w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p>
            <w:p><w:r><w:t>___________</w:t></w:r></w:p>
            <w:p><w:r><w:t>Đồng Nai, ngày 12 tháng 03 năm 2026</w:t></w:r></w:p>
          </w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:r><w:t>Văn bản gốc phải giữ nguyên sau khi sửa phần đầu.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(&src, body, vec![], "", "", false, false);

    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.show_governing_org = true;

    let insp = inspect_file_header(&src, &cfg, Some(&org)).unwrap();
    assert_ne!(insp.classification, HeaderClassification::Valid);
    assert!(insp.findings.iter().any(|f| f.rule_id.starts_with("ND30-PLI-01") || f.rule_id.starts_with("ND30-PLI-02")));

    cfg.allow_override_existing_org = true;
    let res = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res.success, "Lỗi chuẩn hóa: {:?}", res.error);
    assert_eq!(res.classification_after, HeaderClassification::Valid);

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM"));
    assert!(!doc_xml.contains("CHŨ NGHĨA"));
    assert!(!doc_xml.contains("------------"));
    assert!(!doc_xml.contains("___________"));
    assert!(doc_xml.contains("<w:pBdr><w:bottom w:val=\"single\""));
    assert!(doc_xml.contains("22/CV-STNMT"));
    assert!(doc_xml.contains("Văn bản gốc phải giữ nguyên sau khi sửa phần đầu."));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_04_missing_components_and_cong_van_vs_quyet_dinh_layout() {
    let dir = make_temp_hb_dir("c04_doctype_diff");
    let src_cv = dir.join("partial_cv.docx");
    let src_qd = dir.join("partial_qd.docx");

    let partial_body = r#"
      <w:p><w:r><w:t>SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG</w:t></w:r></w:p>
      <w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p><w:r><w:t>Số: 05/STNMT</w:t></w:r></w:p>
      <w:p><w:r><w:t>Đồng Nai, ngày 08 tháng 04 năm 2026</w:t></w:r></w:p>
      <w:p><w:r><w:t>Điều 1. Nội dung thi hành.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(&src_cv, partial_body, vec![], "", "", false, false);
    build_fixture_docx(&src_qd, partial_body, vec![], "", "", false, false);

    let org = sample_state_org();
    let mut cfg_cv = HeaderBuilderConfig::default();
    cfg_cv.doc_type = AdminDocType::CongVan;
    cfg_cv.subject_text = "V/v báo cáo kết quả thực hiện".to_string();

    let insp = inspect_file_header(&src_cv, &cfg_cv, Some(&org)).unwrap();
    assert_eq!(insp.classification, HeaderClassification::MissingComponents);

    let res_cv = apply_header_builder_to_file(&src_cv, &cfg_cv, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res_cv.success, "Lỗi Công văn: {:?}", res_cv.error);
    let xml_cv = OpenXmlPackage::open(Path::new(&res_cv.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml_cv.contains("V/v báo cáo kết quả thực hiện"));
    // Công văn does NOT emit centered "CÔNG VĂN" heading (NĐ30-PLI-05B)
    assert!(!xml_cv.contains(">CÔNG VĂN<"));

    // Quyết định MUST emit centered QUYẾT ĐỊNH heading + centered Trích yếu below table
    let mut cfg_qd = HeaderBuilderConfig::default();
    cfg_qd.doc_type = AdminDocType::QuyetDinh;
    cfg_qd.subject_text = "Về việc ban hành Quy chế nội bộ".to_string();
    let res_qd = apply_header_builder_to_file(&src_qd, &cfg_qd, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res_qd.success, "Lỗi Quyết định: {:?}", res_qd.error);
    let xml_qd = OpenXmlPackage::open(Path::new(&res_qd.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml_qd.contains("QUYẾT ĐỊNH"));
    assert!(xml_qd.contains("Về việc ban hành Quy chế nội bộ"));
    assert!(xml_qd.contains("Điều 1. Nội dung thi hành."));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_05_prevent_duplicate_header_creation_and_idempotent_standardize() {
    let dir = make_temp_hb_dir("c05_no_dup");
    let src = dir.join("already_headed.docx");
    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.show_governing_org = true;
    cfg.number_mode = HeaderNumberMode::ManualInput;
    cfg.doc_number = "10".to_string();
    cfg.doc_symbol = "CV-STNMT".to_string();
    create_standalone_header_docx(&src, &cfg, Some(&org));

    // Attempting CreateNew on an already-headed document must fail safely
    let dup_attempt = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::CreateNew, Some(&org));
    assert!(!dup_attempt.success, "Không được phép chèn trùng phần đầu thứ 2!");

    // Running StandardizeExisting multiple times must keep exactly 1 header table
    cfg.output_mode = OutputMode::OverwriteWithBackup;
    let r1 = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(r1.success);
    let r2 = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(r2.success);

    let xml = OpenXmlPackage::open(&src)
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert_eq!(xml.matches("<w:tbl>").count(), 1);
    assert_eq!(xml.matches("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM").count(), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_06_preserve_existing_number_date_and_conflict_guard() {
    let dir = make_temp_hb_dir("c06_conflict");
    let src = dir.join("conflict.docx");
    let body = r#"
      <w:tbl>
        <w:tr>
          <w:tc>
            <w:p><w:r><w:t>SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG</w:t></w:r></w:p>
            <w:p><w:r><w:t>Số: 88/QĐ-STNMT</w:t></w:r></w:p>
          </w:tc>
          <w:tc>
            <w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
            <w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p>
            <w:p><w:r><w:t>Đồng Nai, ngày 10 tháng 05 năm 2025</w:t></w:r></w:p>
          </w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:r><w:t>Nội dung giữ số và ngày gốc.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(&src, body, vec![], "", "", false, false);

    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.doc_type = AdminDocType::QuyetDinh;
    cfg.number_mode = HeaderNumberMode::ManualInput;
    cfg.doc_number = "999".to_string();
    cfg.doc_symbol = "QĐ-NEW".to_string();
    cfg.date_mode = HeaderDateMode::ManualDate;
    cfg.manual_day = "01".to_string();
    cfg.manual_month = "01".to_string();
    cfg.manual_year = "2026".to_string();
    cfg.allow_override_existing_number = false;
    cfg.allow_override_existing_date = false;

    let insp = inspect_file_header(&src, &cfg, Some(&org)).unwrap();
    assert_eq!(insp.classification, HeaderClassification::DataConflict);

    // 1. Without override consent, manual conflicting number/date MUST be blocked safely
    let res_blocked = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(!res_blocked.success, "Phải chặn ghi đè khi chưa xác nhận xung đột số/ngày");

    // 2. With KeepExisting modes, standardization succeeds while preserving original number & date
    let mut cfg_keep = HeaderBuilderConfig::default();
    cfg_keep.doc_type = AdminDocType::QuyetDinh;
    cfg_keep.number_mode = HeaderNumberMode::KeepExisting;
    cfg_keep.date_mode = HeaderDateMode::KeepExisting;
    let res_keep = apply_header_builder_to_file(&src, &cfg_keep, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res_keep.success, "Chuẩn hóa KeepExisting thất bại: {:?}", res_keep.error);
    let xml_keep = OpenXmlPackage::open(Path::new(&res_keep.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml_keep.contains("88/QĐ-STNMT"));
    assert!(xml_keep.contains("ngày 10 tháng 5 năm 2025"));
    assert!(!xml_keep.contains("999/QĐ-NEW"));

    // 3. With explicit override flags, updates to new user values
    cfg.allow_override_existing_number = true;
    cfg.allow_override_existing_date = true;
    let res_ovr = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res_ovr.success);
    let xml_ovr = OpenXmlPackage::open(Path::new(&res_ovr.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml_ovr.contains("999/QĐ-NEW"));
    assert!(xml_ovr.contains("ngày 01 tháng 01 năm 2026"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_07_preserve_logo_media_header_footer_and_hyperlinks() {
    let dir = make_temp_hb_dir("c07_preserve_assets");
    let src = dir.join("assets.docx");
    let png_bytes = minimal_valid_png_1x1();
    let hdr_xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Running Header</w:t></w:r></w:p></w:hdr>"#;
    let ftr_xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Running Footer</w:t></w:r></w:p></w:ftr>"#;

    let body = r#"
      <w:tbl>
        <w:tr>
          <w:tc>
            <w:p>
              <w:r>
                <w:drawing>
                  <wp:inline><wp:docPr id="1" name="OrgLogo"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rIdLogo"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline>
                </w:drawing>
              </w:r>
            </w:p>
            <w:p><w:r><w:t>SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG</w:t></w:r></w:p>
            <w:p><w:r><w:t>Số: 07/TB-STNMT</w:t></w:r></w:p>
          </w:tc>
          <w:tc>
            <w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
            <w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p>
          </w:tc>
        </w:tr>
      </w:tbl>
      <w:p>
        <w:r><w:t>Truy cập cổng thông tin: </w:t></w:r>
        <w:hyperlink r:id="rIdLink"><w:r><w:t>https://monword.vercel.app</w:t></w:r></w:hyperlink>
      </w:p>
    "#;

    let extra_rels = r#"
      <Relationship Id="rIdLogo" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/logo.png"/>
      <Relationship Id="rIdLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://monword.vercel.app" TargetMode="External"/>
      <Relationship Id="rIdHdr" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
      <Relationship Id="rIdFtr" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
    "#;
    let extra_ct = r#"
      <Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
      <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
    "#;

    build_fixture_docx(
        &src,
        body,
        vec![
            ("word/media/logo.png", png_bytes.clone()),
            ("word/header1.xml", hdr_xml.to_vec()),
            ("word/footer1.xml", ftr_xml.to_vec()),
        ],
        extra_rels,
        extra_ct,
        false,
        false,
    );

    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.doc_type = AdminDocType::ThongBao;
    let res = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res.success, "Lỗi bảo toàn media/header: {:?}", res.error);

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    assert_eq!(out_pkg.get_bytes("word/media/logo.png").unwrap(), png_bytes.as_slice());
    assert!(out_pkg.has_entry("word/header1.xml"));
    assert!(out_pkg.has_entry("word/footer1.xml"));

    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains("<w:drawing>"));
    assert!(doc_xml.contains(r#"r:embed="rIdLogo""#));
    assert!(doc_xml.contains(r#"<w:hyperlink r:id="rIdLink">"#));

    let rels_xml = out_pkg.get_xml("word/_rels/document.xml.rels").unwrap();
    assert!(rels_xml.contains("rIdLogo"));
    assert!(rels_xml.contains("rIdLink"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_08_preserve_body_data_tables_and_signature_block() {
    let dir = make_temp_hb_dir("c08_body_tables_sig");
    let src = dir.join("body_tables_sig.docx");
    let body = r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:r><w:t>SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG</w:t></w:r></w:p></w:tc>
          <w:tc><w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p><w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:r><w:t>Danh sách tổng hợp số liệu chuyên môn:</w:t></w:r></w:p>
      <w:tbl>
        <w:tr><w:tc><w:p><w:r><w:t>CELL_A1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>CELL_B1</w:t></w:r></w:p></w:tc></w:tr>
        <w:tr><w:tc><w:p><w:r><w:t>CELL_A2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>CELL_C3</w:t></w:r></w:p></w:tc></w:tr>
      </w:tbl>
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:r><w:t>Nơi nhận: - Lưu VT.</w:t></w:r></w:p></w:tc>
          <w:tc><w:p><w:r><w:t>KT. GIÁM ĐỐC</w:t></w:r></w:p><w:p><w:r><w:t>Nguyễn Văn A</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
    "#;
    build_fixture_docx(&src, body, vec![], "", "", false, false);

    let org = sample_state_org();
    let cfg = HeaderBuilderConfig::default();
    let res = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res.success, "Lỗi bảo toàn bảng thân văn bản: {:?}", res.error);

    let doc_xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert_eq!(doc_xml.matches("<w:tbl>").count(), 3);
    assert!(doc_xml.contains("CELL_A1"));
    assert!(doc_xml.contains("CELL_B1"));
    assert!(doc_xml.contains("CELL_A2"));
    assert!(doc_xml.contains("CELL_C3"));
    assert!(doc_xml.contains("KT. GIÁM ĐỐC"));
    assert!(doc_xml.contains("Nguyễn Văn A"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_09_preserve_docm_vba_project_and_macro_types() {
    let dir = make_temp_hb_dir("c09_docm_macros");
    let src = dir.join("macro_doc.docm");
    let vba_dummy = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x4D, 0x4F, 0x4E, 0x57];
    let body = r#"
      <w:p><w:r><w:t>Nội dung tài liệu chứa Macro VBA.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(
        &src,
        body,
        vec![("word/vbaProject.bin", vba_dummy.clone())],
        "",
        "",
        true,
        false,
    );

    let org = sample_state_org();
    let cfg = HeaderBuilderConfig::default();
    let res = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::CreateNew, Some(&org));
    assert!(res.success, "Lỗi xử lý .docm: {:?}", res.error);
    assert!(res.output_path.ends_with(".docm"));

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    assert_eq!(out_pkg.get_bytes("word/vbaProject.bin").unwrap(), vba_dummy.as_slice());
    let ct_xml = out_pkg.get_xml("[Content_Types].xml").unwrap();
    assert!(ct_xml.contains("application/vnd.ms-word.document.macroEnabled.main+xml"));
    assert!(res.inspection_after.as_ref().unwrap().has_macros);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_10_preserve_multi_section_and_landscape_orientation() {
    let dir = make_temp_hb_dir("c10_multisect");
    let src = dir.join("multisection.docx");
    let body = r#"
      <w:p><w:r><w:t>Phần mở đầu trang dọc Section 1.</w:t></w:r></w:p>
      <w:p>
        <w:pPr>
          <w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
        </w:pPr>
      </w:p>
      <w:p><w:r><w:t>Bảng biểu trang ngang ở Section 2.</w:t></w:r></w:p>
      <w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/></w:sectPr>
    "#;
    build_fixture_docx(&src, body, vec![], "", "", false, true);

    let org = sample_state_org();
    let cfg = HeaderBuilderConfig::default();
    let res = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::CreateNew, Some(&org));
    assert!(res.success, "Lỗi multi-section: {:?}", res.error);

    let insp_after = res.inspection_after.as_ref().unwrap();
    assert_eq!(insp_after.section_count, 2);

    let doc_xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert_eq!(doc_xml.matches("<w:sectPr").count(), 2);
    assert!(doc_xml.contains(r#"w:orient="landscape""#));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_11_overwrite_backup_and_restore_initial_content() {
    let dir = make_temp_hb_dir("c11_restore_init");
    let src = dir.join("restore_target.docx");
    let body = r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:r><w:t>CƠ QUAN CŨ</w:t></w:r></w:p></w:tc>
          <w:tc><w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p><w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:r><w:t>Nội dung nguyên thủy cần khôi phục.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(&src, body, vec![], "", "", false, false);
    let original_bytes = fs::read(&src).unwrap();

    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.output_mode = OutputMode::OverwriteWithBackup;
    cfg.allow_override_existing_org = true;
    cfg.subject_text = "V/v lần chỉnh thứ 1".to_string();

    let r1 = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(r1.success);
    let init_bak = r1.initial_backup_path.clone().expect("Phải tạo tệp .header_init.bak");
    assert!(Path::new(&init_bak).exists());

    // Second edit must NOT overwrite initial baseline backup
    cfg.subject_text = "V/v lần chỉnh thứ 2".to_string();
    let r2 = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(r2.success);
    assert_eq!(fs::read(&init_bak).unwrap(), original_bytes);

    // Restore original content
    restore_initial_header_backup(&src, Path::new(&init_bak)).unwrap();
    assert_eq!(fs::read(&src).unwrap(), original_bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_12_real_microsoft_word_com_doc_conversion_and_open_verification() {
    let dir = make_temp_hb_dir("c12_real_word_com");
    let doc_legacy = dir.join("legacy_input.doc");
    let standalone_docx = dir.join("standalone_verify.docx");
    let multisect_docx = dir.join("multisect_verify.docx");

    // Step 1: Create genuine binary .doc via real Microsoft Word COM
    let create_ps = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $word.AutomationSecurity = 3
  $doc = $word.Documents.Add()
  $rng = $doc.Range(0, 0)
  $rng.Text = "CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM`r`nĐộc lập - Tự do - Hạnh phúc`r`nNội dung văn bản DOC nhị phân gốc."
  $path = '{}'
  $doc.SaveAs([ref]$path, [ref]0)
  $doc.Close([ref]$false)
}} finally {{
  if ($word -ne $null) {{
    $word.Quit([ref]$false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
  }}
}}
"#,
        doc_legacy.to_string_lossy().replace('\'', "''")
    );
    let out1 = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encode_ps_utf16_b64(&create_ps),
        ])
        .output()
        .expect("Khởi chạy PowerShell tạo .doc thất bại");
    assert!(
        out1.status.success() && doc_legacy.exists(),
        "Không tạo được tệp .doc bằng Word COM: stderr={}",
        String::from_utf8_lossy(&out1.stderr)
    );

    // Step 2: Inspect & standardize the .doc via MonWord Header Builder
    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.doc_type = AdminDocType::CongVan;
    cfg.show_governing_org = true;
    cfg.number_mode = HeaderNumberMode::ManualInput;
    cfg.doc_number = "101".to_string();
    cfg.doc_symbol = "CV-STNMT".to_string();
    cfg.subject_text = "V/v kiểm thử Word COM thật".to_string();

    let insp_doc = inspect_file_header(&doc_legacy, &cfg, Some(&org)).unwrap();
    assert!(!insp_doc.candidates.is_empty());

    let res_doc = apply_header_builder_to_file(
        &doc_legacy,
        &cfg,
        HeaderActionMode::StandardizeExisting,
        Some(&org),
    );
    assert!(res_doc.success, "Chuẩn hóa .doc thất bại: {:?}", res_doc.error);
    let converted_out = PathBuf::from(&res_doc.output_path);
    assert!(converted_out.exists());

    // Prepare standalone & multisection outputs too
    create_standalone_header_docx(&standalone_docx, &cfg, Some(&org));
    let multi_in = dir.join("multi_in.docx");
    build_fixture_docx(
        &multi_in,
        r#"<w:p><w:r><w:t>Sec 1</w:t></w:r></w:p><w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:pPr></w:p><w:p><w:r><w:t>Sec 2</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/></w:sectPr>"#,
        vec![],
        "",
        "",
        false,
        true,
    );
    cfg.output_mode = OutputMode::NewCopy;
    let res_multi = apply_header_builder_to_file(&multi_in, &cfg, HeaderActionMode::CreateNew, Some(&org));
    assert!(res_multi.success);
    fs::copy(&res_multi.output_path, &multisect_docx).unwrap();

    // Step 3: Open all generated DOCX files in real Microsoft Word COM & inspect DOM
    let verify_ps = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $word.AutomationSecurity = 3

  foreach ($p in @('{}', '{}')) {{
    $d = $word.Documents.Open($p, $false, $true)
    try {{
      if ($d.Tables.Count -lt 1) {{ throw "Thieu bang phan dau trong $p" }}
      $txt = $d.Content.Text
      if (-not $txt.Contains('VIỆT NAM')) {{ throw "Thieu Quoc hieu trong $p" }}
    }} finally {{
      $d.Close([ref]$false)
    }}
  }}

  $dm = $word.Documents.Open('{}', $false, $true)
  try {{
    if ($dm.Sections.Count -ne 2) {{ throw "Sai so section: $($dm.Sections.Count)" }}
    if ($dm.Tables.Count -lt 1) {{ throw "Thieu bang phan dau multisection" }}
  }} finally {{
    $dm.Close([ref]$false)
  }}
}} finally {{
  if ($word -ne $null) {{
    $word.Quit([ref]$false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
  }}
}}
"#,
        converted_out.to_string_lossy().replace('\'', "''"),
        standalone_docx.to_string_lossy().replace('\'', "''"),
        multisect_docx.to_string_lossy().replace('\'', "''"),
    );

    let out2 = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encode_ps_utf16_b64(&verify_ps),
        ])
        .output()
        .expect("Khởi chạy PowerShell xác minh Word COM thất bại");
    assert!(
        out2.status.success(),
        "Microsoft Word 16.0 báo lỗi khi mở/kiểm tra tệp đầu ra: stdout={} stderr={}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_13_multi_candidate_uncertain_requires_explicit_user_choice() {
    let dir = make_temp_hb_dir("c13_multi_cand");
    let src = dir.join("multi_candidates.docx");
    let body = r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:r><w:t>CƠ QUAN 1</w:t></w:r></w:p></w:tc>
          <w:tc><w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p><w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:r><w:t>CƠ QUAN 2</w:t></w:r></w:p><w:p><w:r><w:t>Số: 02/CV</w:t></w:r></w:p></w:tc>
          <w:tc><w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p><w:p><w:r><w:t>Hà Nội, ngày 01 tháng 02 năm 2026</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:r><w:t>Thân văn bản chính.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(&src, body, vec![], "", "", false, false);

    let org = sample_state_org();
    let mut cfg = HeaderBuilderConfig::default();
    cfg.selected_candidate_index = None;

    let insp = inspect_file_header(&src, &cfg, Some(&org)).unwrap();
    assert_eq!(insp.classification, HeaderClassification::Uncertain);
    assert!(insp.requires_user_choice);
    assert_eq!(insp.candidates.len(), 2);

    // Refuses blind standardization when ambiguous
    let refused = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(!refused.success);

    // Selecting candidate 0 succeeds and keeps candidate 1 + body
    cfg.selected_candidate_index = Some(0);
    cfg.allow_override_existing_org = true;
    let res = apply_header_builder_to_file(&src, &cfg, HeaderActionMode::StandardizeExisting, Some(&org));
    assert!(res.success, "Lỗi khi chọn ứng viên #0: {:?}", res.error);
    let xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert_eq!(xml.matches("<w:tbl>").count(), 2);
    assert!(xml.contains("CƠ QUAN 2"));
    assert!(xml.contains("Thân văn bản chính."));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn case_14_enterprise_non_dogmatic_and_party_org_36hd_rules() {
    let dir = make_temp_hb_dir("c14_sector_rules");

    // 1. Enterprise without NĐ30 compatibility: must NOT treat Quốc hiệu/Tiêu ngữ as mandatory error
    let ent_org = OrganizationProfile {
        id: "org_ent".to_string(),
        profile_name: "DN Nội bộ".to_string(),
        sector: OrgSector::Enterprise,
        issuing_org: "CÔNG TY TNHH GIẢI PHÁP SỐ".to_string(),
        default_location: "TP. Hồ Chí Minh".to_string(),
        use_nd30_layout_for_enterprise: false,
        ..Default::default()
    };

    let ent_rules = applicable_rules_for(Some(&ent_org), AdminDocType::VanBanNoiBo);
    let qh_rule = ent_rules.iter().find(|r| r.rule_id == "ND30-PLI-01").unwrap();
    assert_eq!(qh_rule.obligation, RuleObligation::OrgConfigurable);

    let ent_doc = dir.join("enterprise_internal.docx");
    let ent_body = r#"
      <w:tbl>
        <w:tr>
          <w:tc><w:p><w:r><w:t>CÔNG TY TNHH GIẢI PHÁP SỐ</w:t></w:r></w:p><w:p><w:r><w:t>Số: 09/2026/TB-DN</w:t></w:r></w:p></w:tc>
          <w:tc><w:p><w:r><w:t>TP. Hồ Chí Minh, ngày 20 tháng 05 năm 2026</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:r><w:t>Thông báo nội bộ doanh nghiệp.</w:t></w:r></w:p>
    "#;
    build_fixture_docx(&ent_doc, ent_body, vec![], "", "", false, false);

    let mut cfg_ent = HeaderBuilderConfig::default();
    cfg_ent.doc_type = AdminDocType::VanBanNoiBo;
    cfg_ent.show_national_header = false;
    let insp_ent = inspect_file_header(&ent_doc, &cfg_ent, Some(&ent_org)).unwrap();
    assert!(
        !insp_ent.findings.iter().any(|f| f.severity == AuditSeverity::Error && f.rule_id == "ND30-PLI-01"),
        "Hồ sơ doanh nghiệp không được báo lỗi cứng Quốc hiệu khi không bật tương thích NĐ 30"
    );

    // 2. PartyOrg (36-HD/VPTW)
    let party_org = OrganizationProfile {
        id: "org_party".to_string(),
        profile_name: "Đảng bộ thử nghiệm".to_string(),
        sector: OrgSector::PartyOrg,
        governing_org: "TỈNH ỦY THỪA THIÊN HUẾ".to_string(),
        issuing_org: "ĐẢNG ỦY KHỐI DOANH NGHIỆP".to_string(),
        default_location: "Huế".to_string(),
        show_governing_org_default: true,
        ..Default::default()
    };
    let party_rules = applicable_rules_for(Some(&party_org), AdminDocType::NghiQuyet);
    assert!(party_rules.iter().any(|r| r.rule_id == "DANG-36HD-01"));

    let party_doc = dir.join("party_standalone.docx");
    let mut cfg_party = HeaderBuilderConfig::default();
    cfg_party.doc_type = AdminDocType::NghiQuyet;
    cfg_party.show_governing_org = true;
    cfg_party.subject_text = "Về nhiệm vụ năm 2026".to_string();
    let res_party = create_standalone_header_docx(&party_doc, &cfg_party, Some(&party_org));
    assert!(res_party.success);
    let party_xml = OpenXmlPackage::open(&party_doc)
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(party_xml.contains("ĐẢNG CỘNG SẢN VIỆT NAM"));
    assert!(!party_xml.contains("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM"));
    assert!(get_header_rules_catalog().iter().any(|r| r.rule_id == "DANG-36HD-01"));

    let _ = fs::remove_dir_all(&dir);
}
