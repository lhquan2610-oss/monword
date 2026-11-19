use crate::engine::{self, openxml_arch::OpenXmlPackage};
use crate::models::{OrientationMode, OutputMode, PresetId, ProcessConfig};
use std::fs;
use std::path::{Path, PathBuf};
use zip::CompressionMethod;

fn make_temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chuanhoa_test_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn build_minimal_docx(path: &Path, body_inner_xml: &str, extra_entries: Vec<(&str, Vec<u8>)>, macro_enabled: bool) {
    let mut pkg = OpenXmlPackage { entries: Vec::new() };

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
  <Override PartName="/word/document.xml" ContentType="{}"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
  {}
</Types>"#,
        doc_main_ct,
        if macro_enabled {
            r#"<Override PartName="/word/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/>"#
        } else {
            ""
        }
    );
    pkg.set_bytes("[Content_Types].xml", content_types.into_bytes(), CompressionMethod::Deflated);

    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    pkg.set_bytes("_rels/.rels", root_rels.as_bytes().to_vec(), CompressionMethod::Deflated);

    let doc_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
  {}
</Relationships>"#,
        if macro_enabled {
            r#"<Relationship Id="rId3" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/>"#
        } else {
            ""
        }
    );
    pkg.set_bytes("word/_rels/document.xml.rels", doc_rels.into_bytes(), CompressionMethod::Deflated);

    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    {}
    <w:sectPr>
      <w:pgSz w:w="12240" w:h="15840"/>
      <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720"/>
    </w:sectPr>
  </w:body>
</w:document>"#,
        body_inner_xml
    );
    pkg.set_bytes("word/document.xml", doc_xml.into_bytes(), CompressionMethod::Deflated);

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
</w:styles>"#;
    pkg.set_bytes("word/styles.xml", styles_xml.as_bytes().to_vec(), CompressionMethod::Deflated);

    let settings_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
    pkg.set_bytes("word/settings.xml", settings_xml.as_bytes().to_vec(), CompressionMethod::Deflated);

    for (entry_name, bytes) in extra_entries {
        pkg.set_bytes(entry_name, bytes, CompressionMethod::Deflated);
    }

    pkg.save_to(path).unwrap();
}

#[test]
fn test_phase2_context_aware_formatting() {
    let dir = make_temp_test_dir("phase2");
    let src = dir.join("congvan.docx");

    let body = r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p>
      <w:p><w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr><w:t>Nội dung văn bản hành chính cần chuẩn hóa theo quy định hiện hành.</w:t></w:r></w:p>
      <w:tbl>
        <w:tr><w:tc><w:p><w:pPr><w:ind w:firstLine="720"/></w:pPr><w:r><w:t>STT</w:t></w:r></w:p></w:tc></w:tr>
        <w:tr><w:tc><w:p><w:pPr><w:ind w:firstLine="720"/></w:pPr><w:r><w:t>1</w:t></w:r></w:p></w:tc></w:tr>
      </w:tbl>
    "#;
    build_minimal_docx(&src, body, vec![], false);

    let cfg = ProcessConfig::from_preset(PresetId::Nd30);
    let res = engine::process_single_file(&src, &cfg);
    assert!(res.success, "Lỗi xử lý: {:?}", res.error);

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();

    // Quốc hiệu vẫn giữ center và không bị ép thụt đầu dòng 720
    assert!(doc_xml.contains(r#"<w:jc w:val="center"/>"#));
    assert!(doc_xml.contains(r#"w:ascii="Times New Roman""#));
    // Đoạn thân văn bản nhận 14pt (w:val="28") và thụt đầu dòng 1.27cm (~720 twips)
    assert!(doc_xml.contains(r#"w:val="28""#));
    assert!(doc_xml.contains(r#"w:firstLine="720""#));
    // Bảng tối ưu: tự co 100%, chống tách dòng, lặp tiêu đề và cỡ 13pt (26) + không thụt đầu dòng
    assert!(doc_xml.contains(r#"w:w="5000" w:type="pct""#));
    assert!(doc_xml.contains("<w:cantSplit/>"));
    assert!(doc_xml.contains("<w:tblHeader/>"));
    assert!(doc_xml.contains(r#"w:val="26""#));
    assert!(doc_xml.contains(r#"w:firstLine="0""#));
    // Khổ giấy A4 + lề 2-2-3-2 cm
    assert!(doc_xml.contains(r#"w:w="11906" w:h="16838""#));
    assert!(doc_xml.contains(r#"w:top="1134""#));
    assert!(doc_xml.contains(r#"w:left="1701""#));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_phase3_vn_cleaner_and_legacy_encoding() {
    let dir = make_temp_test_dir("phase3");
    let src = dir.join("bangma_cu.docx");

    // Mix TCVN3 (.VnTime) and VNI (VNI-Times) plus soft break & date & quotes
    let body = r#"
      <w:p>
        <w:r><w:rPr><w:rFonts w:ascii=".VnTime" w:hAnsi=".VnTime"/></w:rPr><w:t>Céng hßa x· héi ViÖt Nam</w:t></w:r>
      </w:p>
      <w:p>
        <w:r><w:rPr><w:rFonts w:ascii="VNI-Times" w:hAnsi="VNI-Times"/></w:rPr><w:t>Ñoäc laäp ,   töï do !  "Chuaån hoùa"  ngaøy 5 thaùng 2 naêm 2026 vaø ngaøy 5 thaùng 03 naêm 2026.</w:t><w:br/><w:t>Doøng sau ngắt mềm.</w:t></w:r>
      </w:p>
    "#;
    build_minimal_docx(&src, body, vec![], false);

    let cfg = ProcessConfig::from_preset(PresetId::Nd30);
    let res = engine::process_single_file(&src, &cfg);
    assert!(res.success, "Lỗi xử lý: {:?}", res.error);

    let before = res.audit_before.unwrap();
    let after = res.audit_after.unwrap();
    assert!(before.has_legacy_encoding);
    assert!(!after.has_legacy_encoding);
    assert!(after.score > before.score, "Before {} vs After {}", before.score, after.score);

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains("Cộng hòa xã hội Việt Nam"), "XML: {}", doc_xml);
    assert!(doc_xml.contains("Độc lập, tự do! “Chuẩn hóa” ngày 05 tháng 02 năm 2026 và ngày 05 tháng 3 năm 2026."), "XML: {}", doc_xml);
    assert!(!doc_xml.contains(".VnTime"));
    assert!(!doc_xml.contains("VNI-Times"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_phase4_page_numbers_landscape_margin_swap_and_toc() {
    let dir = make_temp_test_dir("phase4");
    let src = dir.join("baocao_3trang.docx");

    let body = r#"
      <w:p><w:r><w:t>CHƯƠNG I: QUY ĐỊNH CHUNG</w:t></w:r></w:p>
      <w:p><w:r><w:t>Điều 1. Phạm vi áp dụng</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>
      <w:p><w:r><w:t>1.1. Bảng phụ lục ngang trên trang 2</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>
      <w:p><w:r><w:t>Nội dung trang 3 trở lại khổ dọc.</w:t></w:r></w:p>
    "#;
    build_minimal_docx(&src, body, vec![], false);

    let mut cfg = ProcessConfig::from_preset(PresetId::Nd30);
    cfg.page_toc.page_numbers_enabled = true;
    cfg.page_toc.skip_first_page = true;
    cfg.page_toc.orientation_mode = OrientationMode::RotateRanges;
    cfg.page_toc.landscape_ranges = "2".to_string();
    cfg.page_toc.auto_swap_binding_margin = true;
    cfg.page_toc.apply_nd30_headings = true;
    cfg.page_toc.auto_detect_headings = true;
    cfg.page_toc.insert_toc = true;

    let res = engine::process_single_file(&src, &cfg);
    assert!(res.success, "Lỗi xử lý: {:?}", res.error);

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let hdr_entry = out_pkg
        .entries
        .iter()
        .find(|e| e.name.starts_with("word/header") && e.name.ends_with(".xml"))
        .map(|e| e.name.clone())
        .expect("Phải sinh file header chứa số trang");
    let hdr_xml = out_pkg.get_xml(&hdr_entry).unwrap();
    assert!(hdr_xml.contains("PAGE"), "Header XML: {}", hdr_xml);

    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains("<w:titlePg/>"));
    assert!(doc_xml.contains(r#"w:orient="landscape""#));
    // Trang ngang có tự đảo lề gáy 3cm (1701 twips) lên cạnh trên w:top
    assert!(doc_xml.contains(r#"w:top="1701""#));
    assert!(doc_xml.contains(r#"w:val="Heading1""#));
    assert!(doc_xml.contains(r#"w:val="Heading2""#));
    assert!(doc_xml.contains(r#"w:val="Heading3""#));
    assert!(doc_xml.contains("Table of Contents"));

    let settings_xml = out_pkg.get_xml("word/settings.xml").unwrap();
    assert!(
        settings_xml.contains("updateFields") || doc_xml.contains("TOC"),
        "Settings/Doc phải có cờ updateFields hoặc trường TOC"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_phase5_macro_cleaner_unlock_and_metadata() {
    let dir = make_temp_test_dir("phase5");
    let src = dir.join("bieumau_nhiem_macro.docm");

    let body = r#"<w:p><w:r><w:t>Nội dung mẫu có chứa Macro và khóa chỉnh sửa.</w:t></w:r></w:p>"#;
    let locked_settings = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:documentProtection w:edit="readOnly" w:enforcement="1"/>
</w:settings>"#.to_vec();
    let core_xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:creator>Nguyen Van A</dc:creator>
  <cp:lastModifiedBy>Tran Thi B</cp:lastModifiedBy>
</cp:coreProperties>"#.to_vec();
    let app_xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties">
  <Company>Cong Ty Cu</Company>
  <TotalTime>120</TotalTime>
</Properties>"#.to_vec();

    build_minimal_docx(
        &src,
        body,
        vec![
            ("word/vbaProject.bin", b"FAKE_VBA_BINARY".to_vec()),
            ("word/settings.xml", locked_settings),
            ("docProps/core.xml", core_xml),
            ("docProps/app.xml", app_xml),
        ],
        true,
    );

    let orig_bytes = fs::read(&src).unwrap();

    let mut cfg = ProcessConfig::from_preset(PresetId::Nd30);
    cfg.output_mode = OutputMode::OverwriteWithBackup;
    cfg.security.clean_macros = true;
    cfg.security.unlock_protection = true;
    cfg.security.scrub_personal_info = true;
    cfg.security.author = "Admin".to_string();

    let res = engine::process_single_file(&src, &cfg);
    assert!(res.success, "Lỗi xử lý: {:?}", res.error);
    assert!(res.output_path.ends_with(".docx"));
    let bak_path = res.backup_path.as_ref().expect("Phải có file backup");
    assert!(Path::new(bak_path).is_file());

    let clean_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    assert!(!clean_pkg.has_entry("word/vbaProject.bin"));
    let ct = clean_pkg.get_xml("[Content_Types].xml").unwrap();
    assert!(ct.contains("wordprocessingml.document.main+xml"));
    assert!(!ct.contains("vbaProject"));
    let rels = clean_pkg.get_xml("word/_rels/document.xml.rels").unwrap();
    assert!(!rels.contains("vbaProject"));

    let settings = clean_pkg.get_xml("word/settings.xml").unwrap();
    assert!(!settings.contains("documentProtection"));

    let core = clean_pkg.get_xml("docProps/core.xml").unwrap();
    assert!(!core.contains("Nguyen Van A"));
    assert!(!core.contains("Tran Thi B"));
    assert!(core.contains("Admin"));

    let app = clean_pkg.get_xml("docProps/app.xml").unwrap();
    assert!(!app.contains("Cong Ty Cu"));

    // Kiểm tra hoàn tác 1-click
    engine::restore_backup(&src, Path::new(bak_path)).unwrap();
    assert_eq!(fs::read(&src).unwrap(), orig_bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_real_world_quoc_hieu_table_blanks_and_signature() {
    let dir = make_temp_test_dir("real_world_qh");
    let src = dir.join("quyetdinh_de_an.docx");

    let long_cell_text = "Nội dung rất dài trong bảng kế hoạch năm 2026 ".repeat(14);
    let body = format!(
        r#"
      <w:tbl>
        <w:tblPr><w:tblW w:w="10201" w:type="dxa"/><w:jc w:val="center"/><w:tblLayout w:type="fixed"/></w:tblPr>
        <w:tblGrid><w:gridCol w:w="4531"/><w:gridCol w:w="5670"/></w:tblGrid>
        <w:tr>
          <w:tc><w:tcPr><w:tcW w:w="4531" w:type="dxa"/></w:tcPr>
            <w:p><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="1"/><w:rPr><w:szCs w:val="28"/></w:rPr></w:pPr><w:r><w:rPr><w:szCs w:val="28"/></w:rPr><w:t>UBND THÀNH PHỐ HUẾ</w:t></w:r></w:p>
            <w:p><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="1"/><w:rPr><w:b/><w:sz w:val="26"/></w:rPr></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="26"/></w:rPr><w:t>CÔNG TY CỔ PHẦN MÔI TRƯỜNG</w:t></w:r></w:p>
          </w:tc>
          <w:tc><w:tcPr><w:tcW w:w="5670" w:type="dxa"/></w:tcPr>
            <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="26"/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
            <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="26"/></w:rPr><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p>
          </w:tc>
        </w:tr>
        <w:tr>
          <w:tc><w:tcPr><w:tcW w:w="4531" w:type="dxa"/></w:tcPr>
            <w:p><w:pPr><w:spacing w:before="120"/><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:r><w:rPr><w:sz w:val="26"/></w:rPr><w:t xml:space="preserve">Số:           /QĐ-HĐQT</w:t></w:r></w:p>
          </w:tc>
          <w:tc><w:tcPr><w:tcW w:w="5670" w:type="dxa"/></w:tcPr>
            <w:p><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="2"/></w:pPr><w:r><w:rPr><w:i/><w:sz w:val="26"/></w:rPr><w:t>Thành phố Huế, ngày</w:t></w:r><w:r><w:rPr><w:sz w:val="26"/></w:rPr><w:t xml:space="preserve">           </w:t></w:r><w:r><w:rPr><w:i/><w:sz w:val="26"/></w:rPr><w:t>tháng</w:t></w:r><w:r><w:rPr><w:sz w:val="26"/></w:rPr><w:t xml:space="preserve">           </w:t></w:r><w:r><w:rPr><w:i/><w:sz w:val="26"/></w:rPr><w:t>năm 2026</w:t></w:r></w:p>
          </w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="4"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>QUYẾT ĐỊNH</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Về việc ban hành Đề án điều chỉnh</w:t></w:r></w:p>
      <w:p><w:pPr><w:ind w:firstLine="567"/><w:outlineLvl w:val="0"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Điều 1. </w:t></w:r><w:r><w:t>Ban hành kèm theo Quyết định này Đề án điều chỉnh về Chuyển đổi số tại Công ty Cổ phần Môi trường và Công trình Đô thị Huế.</w:t></w:r></w:p>
      <w:p><w:pPr><w:tabs><w:tab w:val="center" w:pos="6804"/></w:tabs><w:jc w:val="both"/><w:outlineLvl w:val="8"/></w:pPr><w:r><w:rPr><w:b/><w:i/><w:sz w:val="24"/></w:rPr><w:t>Nơi nhận:</w:t></w:r><w:r><w:tab/></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>TM. HỘI ĐỒNG QUẢN TRỊ</w:t></w:r></w:p>
      <w:p><w:pPr><w:tabs><w:tab w:val="center" w:pos="6804"/></w:tabs><w:jc w:val="both"/></w:pPr><w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t>- Như Điều 3;</w:t></w:r><w:r><w:tab/></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>CHỦ TỊCH HĐQT</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="both"/></w:pPr><w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t>- Lưu VT.</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="both"/></w:pPr></w:p>
      <w:p><w:pPr><w:jc w:val="both"/></w:pPr></w:p>
      <w:p><w:pPr><w:jc w:val="both"/></w:pPr></w:p>
      <w:p><w:pPr><w:tabs><w:tab w:val="center" w:pos="6804"/></w:tabs><w:jc w:val="both"/></w:pPr><w:r><w:tab/><w:t>Phan Lê Hiến</w:t></w:r></w:p>
      <w:p><w:r><w:t>PHẦN I. SỰ CẦN THIẾT VÀ QUAN ĐIỂM ĐIỀU CHỈNH</w:t></w:r></w:p>
      <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>5. Chuyển đổi số công tác quản lý tài sản, công cụ dụng cụ, phương tiện và sửa chữa kỹ thuật</w:t></w:r></w:p>
      <w:p><w:r><w:t>PHẦN VII. TỔ CHỨC THỰC HIỆN</w:t></w:r></w:p>
      <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>2. Chủ tịch Hội đồng quản trị</w:t></w:r></w:p>
      <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>3. Tổng Giám đốc</w:t></w:r></w:p>
      <w:tbl>
        <w:tblPr><w:tblW w:w="10047" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>
        <w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="8547"/></w:tblGrid>
        <w:tr><w:trPr><w:trHeight w:val="630"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Thời gian</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="8547" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Nhiệm vụ</w:t></w:r></w:p></w:tc></w:tr>
        <w:tr><w:trPr><w:trHeight w:val="2780"/><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:keepNext/></w:pPr><w:r><w:t>Năm 2026</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="8547" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:keepNext/></w:pPr><w:r><w:t>{long}</w:t></w:r></w:p></w:tc></w:tr>
      </w:tbl>
    "#,
        long = long_cell_text
    );

    let legacy_footer = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
</w:ftr>"#.to_vec();

    let styles_blue = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:rPr><w:color w:val="2F5496" w:themeColor="accent1" w:themeShade="BF"/></w:rPr></w:style>
</w:styles>"#.to_vec();

    build_minimal_docx(
        &src,
        &body,
        vec![
            ("word/footer2.xml", legacy_footer),
            ("word/styles.xml", styles_blue),
        ],
        false,
    );

    let mut cfg = ProcessConfig::from_preset(PresetId::Nd30);
    cfg.page_toc.apply_nd30_headings = true;
    cfg.page_toc.auto_detect_headings = true;
    cfg.page_toc.insert_toc = true;

    let res = engine::process_single_file(&src, &cfg);
    assert!(res.success, "Lỗi xử lý: {:?}", res.error);

    let out_pkg = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    let styles_xml = out_pkg.get_xml("word/styles.xml").unwrap();

    // 1. Bảng Quốc hiệu khớp vùng in NĐ30 (9071 dxa fixed), không bị co thành 5000 pct
    let pos_qh = doc_xml.find("CỘNG HÒA XÃ HỘI").unwrap();
    let qh_start = doc_xml[..pos_qh]
        .rfind("<w:tbl>")
        .or_else(|| doc_xml[..pos_qh].rfind("<w:tbl "))
        .unwrap();
    let qh_end = pos_qh + doc_xml[pos_qh..].find("</w:tbl>").unwrap();
    let qh_tbl_xml = &doc_xml[qh_start..qh_end];
    assert!(!qh_tbl_xml.contains(r#"w:type="pct""#), "Quốc hiệu bị chuyển sang pct: {}", qh_tbl_xml);
    assert!(qh_tbl_xml.contains(r#"w:w="9071" w:type="dxa""#), "Quốc hiệu phải vừa khổ in 9071 dxa: {}", qh_tbl_xml);

    // 1b. Bảng dữ liệu dài cho phép ngắt trang tự nhiên: bỏ trHeight=2780, bỏ keepNext, Row[1] không có cantSplit, Row[0] giữ tblHeader + cantSplit
    assert!(!doc_xml.contains(r#"w:val="2780""#), "Bảng dữ liệu vẫn còn w:trHeight cứng gây tràn/đẩy trang!");
    assert!(!doc_xml.contains("<w:keepNext/>"), "Trong bảng dữ liệu vẫn còn w:keepNext!");
    let pos_thoi_gian = doc_xml.find("Thời gian").unwrap();
    let tbl_start = doc_xml[..pos_thoi_gian]
        .rfind("<w:tbl>")
        .or_else(|| doc_xml[..pos_thoi_gian].rfind("<w:tbl "))
        .unwrap();
    let tbl_end = pos_thoi_gian + doc_xml[pos_thoi_gian..].find("</w:tbl>").unwrap();
    let tbl_data_xml = &doc_xml[tbl_start..tbl_end];
    let pos_nam_2026 = tbl_data_xml.find("Năm 2026").unwrap();
    let split_row_pos = tbl_data_xml[..pos_nam_2026].rfind("</w:tr>").unwrap();
    let row0_slice = &tbl_data_xml[..split_row_pos];
    let row1_slice = &tbl_data_xml[split_row_pos..];
    assert!(row0_slice.contains("<w:tblHeader/>"), "Hàng tiêu đề bảng dữ liệu thiếu tblHeader!");
    assert!(row0_slice.contains("<w:cantSplit/>"), "Hàng tiêu đề bảng dữ liệu nên giữ cantSplit!");
    assert!(!row1_slice.contains("<w:cantSplit/>"), "Hàng dữ liệu dài vẫn bị gắn cantSplit gây đẩy cả bảng xuống trang sau!");

    // 2. Bảo toàn khoảng trống điền Số / Ngày tháng
    assert!(doc_xml.contains("Số:           /QĐ-HĐQT"), "Mất khoảng trống Số văn bản!");
    assert!(doc_xml.contains("           "), "Mất khoảng trống ngày tháng!");

    // 3. Bảo toàn khối Nơi nhận & 3 dòng trống ký tên trước Phan Lê Hiến
    assert!(doc_xml.contains("Phan Lê Hiến"));
    assert!(!doc_xml.contains(r#"w:val="2F5496""#));
    assert!(!styles_xml.contains(r#"w:themeColor="accent1""#));

    // 4. Điều 1 dài không bị gắn Heading2; PHẦN I được gắn Heading1; Mục lục nằm SAU Phan Lê Hiến (trước PHẦN I)
    let pos_sig = doc_xml.find("Phan Lê Hiến").unwrap();
    let pos_toc = doc_xml.find("MỤC LỤC").unwrap();
    let pos_p1 = doc_xml.find("PHẦN I.").unwrap();
    assert!(pos_sig < pos_toc && pos_toc < pos_p1, "Vị trí Mục lục sai: sig={} toc={} p1={}", pos_sig, pos_toc, pos_p1);
    assert!(!doc_xml.contains(r#"\u"#), "TOC vẫn còn cờ \\u hút outlineLvl rác!");

    // 4b. Heading dài tiếng Việt (92 ký tự / 127 bytes UTF-8) và mục đánh số có từ khóa chức danh đều nhận Heading2
    let hd2_count = doc_xml.matches(r#"w:val="Heading2""#).count();
    assert_eq!(hd2_count, 3, "Kỳ vọng đúng 3 Heading2 (5., 2., 3.), thực tế {}: {}", hd2_count, doc_xml);

    // 5. Không còn số trang PAGE ở footer cũ khi đã đánh số trang ở đầu trang
    assert!(!doc_xml.contains("w:footerReference"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[ignore]
fn verify_real_document_from_env() {
    let path_str = std::env::var("CHUANHOA_VERIFY_DOCX").expect("Set CHUANHOA_VERIFY_DOCX");
    let toc_enabled = std::env::var("CHUANHOA_TOC")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    let src = PathBuf::from(path_str);
    let mut cfg = ProcessConfig::from_preset(PresetId::Nd30);
    if toc_enabled {
        cfg.page_toc.apply_nd30_headings = true;
        cfg.page_toc.auto_detect_headings = true;
        cfg.page_toc.insert_toc = true;
    }
    cfg.also_export_pdf = true;

    let res = engine::process_single_file(&src, &cfg);
    println!("Result success={} output={} error={:?}", res.success, res.output_path, res.error);
    println!("Stats: {}", res.stats_summary);
    for l in &res.logs {
        println!("LOG: {}", l);
    }
    assert!(res.success, "Failed: {:?}", res.error);
}
