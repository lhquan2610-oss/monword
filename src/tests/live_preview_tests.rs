use crate::engine::compliance_audit::sha256_util::sha256_hex;
use crate::engine::live_preview::{
    self, build_live_preview, cancel_preview_render, load_preview_media_base64,
    render_word_com_verification, PREVIEW_FIDELITY_DISCLAIMER_VI,
};
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::process_single_file;
use crate::models::{
    AdminDocType, HeaderActionMode, HeaderBuilderConfig, OrgSector, OrganizationProfile, PaperSize,
    PreviewBlock, PreviewBuildRequest, PreviewOverrideConfig, ProcessConfig, StdDocScope,
};
use std::fs;
use std::path::{Path, PathBuf};
use zip::CompressionMethod;

fn make_temp_preview_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "monword_preview_test_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn minimal_png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn inline_img_para(rid: &str, name: &str, cx: i64, cy: i64) -> String {
    format!(
        r#"<w:p>
  <w:pPr><w:jc w:val="center"/></w:pPr>
  <w:r>
    <w:drawing>
      <wp:inline distT="0" distB="0" distL="0" distR="0">
        <wp:extent cx="{cx}" cy="{cy}"/>
        <wp:docPr id="1" name="{name}"/>
        <a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
            <pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
              <pic:nvPicPr><pic:cNvPr id="0" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr>
              <pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
              <pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>
            </pic:pic>
          </a:graphicData>
        </a:graphic>
      </wp:inline>
    </w:drawing>
  </w:r>
</w:p>"#,
        rid = rid,
        name = name,
        cx = cx,
        cy = cy
    )
}

#[allow(clippy::too_many_arguments)]
fn build_preview_fixture_docx(
    path: &Path,
    body_inner_xml: &str,
    sect_pr_xml: Option<&str>,
    extra_entries: Vec<(&str, Vec<u8>)>,
    extra_rels: &str,
    extra_ct: &str,
) {
    let mut pkg = OpenXmlPackage {
        entries: Vec::new(),
    };

    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
  {}
</Types>"#,
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
  {}
</Relationships>"#,
        extra_rels
    );
    pkg.set_bytes(
        "word/_rels/document.xml.rels",
        doc_rels.into_bytes(),
        CompressionMethod::Deflated,
    );

    let default_sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
  <w:body>{}{}</w:body>
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

    for (name, bytes) in extra_entries {
        pkg.set_bytes(name, bytes, CompressionMethod::Deflated);
    }

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

#[test]
fn preview_case_01_a4_geometry_fonts_spacing_headers_footers_and_page_numbers() {
    let dir = make_temp_preview_dir("c01_a4_hf");
    let src = dir.join("a4_hf.docx");

    let hdr_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p>
    <w:pPr><w:jc w:val="center"/></w:pPr>
    <w:r><w:t xml:space="preserve">Trang </w:t></w:r>
    <w:fldSimple w:instr=" PAGE "><w:r><w:t>2</w:t></w:r></w:fldSimple>
    <w:r><w:t xml:space="preserve">/</w:t></w:r>
    <w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>2</w:t></w:r></w:fldSimple>
  </w:p>
</w:hdr>"#;

    let ftr_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t>MonWord Footer</w:t></w:r></w:p>
</w:ftr>"#;

    let sect = r#"<w:sectPr>
  <w:headerReference w:type="default" r:id="rId3"/>
  <w:footerReference w:type="default" r:id="rId4"/>
  <w:pgSz w:w="11906" w:h="16838"/>
  <w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/>
  <w:pgNumType w:start="1"/>
  <w:titlePg/>
</w:sectPr>"#;

    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:r><w:br w:type="page"/></w:r>{}</w:p>
    "#,
        tnr_run("QUYẾT ĐỊNH", "28", "<w:b/>"),
        tnr_run("Nội dung trang thứ nhất chuẩn Times New Roman 14pt giãn dòng 1.15.", "28", ""),
        tnr_run("Nội dung sang trang thứ hai sau ngắt trang.", "28", ""),
    );

    build_preview_fixture_docx(
        &src,
        &body,
        Some(sect),
        vec![
            ("word/header1.xml", hdr_xml.as_bytes().to_vec()),
            ("word/footer1.xml", ftr_xml.as_bytes().to_vec()),
        ],
        r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>"#,
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#,
    );

    let req = PreviewBuildRequest {
        job_id: "c01".to_string(),
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        ..Default::default()
    };
    let session = build_live_preview(&req, None);
    assert_eq!(session.status, "completed");
    assert_eq!(session.fidelity_disclaimer, PREVIEW_FIDELITY_DISCLAIMER_VI);
    assert!(session.fidelity_disclaimer.contains("KHÔNG khẳng định khớp tuyệt đối"));

    let doc = session.before.unwrap();
    assert_eq!(doc.total_pages, 2);
    let p1 = &doc.pages[0];
    let p2 = &doc.pages[1];

    assert_eq!(p1.paper_label, "A4");
    assert_eq!(p1.orientation, "portrait");
    assert!((p1.width_mm - 210.0).abs() <= 0.5);
    assert!((p1.height_mm - 297.0).abs() <= 0.5);
    assert!((p1.margin_left_mm - 30.0).abs() <= 0.5);
    assert!(p1.header_blocks.is_empty(), "w:titlePg phải ẩn header trang 1");

    assert_eq!(p2.page_number, 2);
    assert_eq!(p2.header_blocks.len(), 1);
    assert!(
        p2.header_blocks[0].plain_text.contains("Trang 2/2"),
        "Header trang 2 thực tế: {}",
        p2.header_blocks[0].plain_text
    );
    assert_eq!(p2.footer_blocks[0].plain_text, "MonWord Footer");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_02_multi_section_portrait_and_landscape_rendering() {
    let dir = make_temp_preview_dir("c02_multi_section");
    let src = dir.join("multi_section.docx");

    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="both"/><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="center"/><w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="1701" w:right="1134" w:bottom="850" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/></w:pPr>{}</w:p>
    "#,
        tnr_run("Section 1: Trang dọc A4.", "28", ""),
        tnr_run("Section 2: Phụ lục xoay ngang A4.", "28", "<w:b/>"),
        tnr_run("Section 3: Quay lại trang dọc A4.", "28", ""),
    );
    let final_sect = r#"<w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    build_preview_fixture_docx(&src, &body, Some(final_sect), vec![], "", "");

    let req = PreviewBuildRequest {
        job_id: "c02".to_string(),
        path: src.to_string_lossy().to_string(),
        ..Default::default()
    };
    let session = build_live_preview(&req, None);
    let doc = session.before.unwrap();
    assert_eq!(doc.section_count, 3);
    assert_eq!(doc.total_pages, 3);

    assert_eq!(doc.pages[0].section_index, 1);
    assert_eq!(doc.pages[0].orientation, "portrait");
    assert!(doc.pages[0].width_mm < doc.pages[0].height_mm);

    assert_eq!(doc.pages[1].section_index, 2);
    assert_eq!(doc.pages[1].orientation, "landscape");
    assert!(doc.pages[1].width_mm > doc.pages[1].height_mm);
    assert!((doc.pages[1].margin_top_mm - 30.0).abs() <= 0.5);

    assert_eq!(doc.pages[2].section_index, 3);
    assert_eq!(doc.pages[2].orientation, "portrait");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_03_tables_colspan_rowspan_borders_and_lazy_media_images() {
    let dir = make_temp_preview_dir("c03_tbl_img");
    let src = dir.join("tables_and_image.docx");

    let mut data_rows = String::new();
    for i in 1..=38 {
        data_rows.push_str(&format!(
            r#"<w:tr><w:trPr><w:trHeight w:val="450"/></w:trPr>
              <w:tc><w:p>{}</w:p></w:tc>
              <w:tc><w:p>{}</w:p></w:tc>
              <w:tc><w:p>{}</w:p></w:tc>
            </w:tr>"#,
            tnr_run(&i.to_string(), "24", ""),
            tnr_run("Dữ liệu bảng nhiều dòng tách trang kiểm tra lặp tiêu đề.", "24", ""),
            tnr_run("Đạt", "24", ""),
        ));
    }

    let body = format!(
        r#"
      <w:tbl>
        <w:tblPr>
          <w:tblBorders>
            <w:top w:val="nil"/><w:bottom w:val="nil"/><w:left w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/>
          </w:tblBorders>
        </w:tblPr>
        <w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="5000"/></w:tblGrid>
        <w:tr>
          <w:tc><w:p>{}</w:p></w:tc>
          <w:tc><w:p>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
      {}
      <w:tbl>
        <w:tblPr>
          <w:tblBorders>
            <w:top w:val="single" w:sz="6"/><w:bottom w:val="single" w:sz="6"/><w:left w:val="single" w:sz="6"/><w:right w:val="single" w:sz="6"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/>
          </w:tblBorders>
        </w:tblPr>
        <w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="5500"/><w:gridCol w:w="2000"/></w:tblGrid>
        <w:tr><w:trPr><w:tblHeader/></w:trPr>
          <w:tc><w:tcPr><w:shd w:fill="DBEAFE"/></w:tcPr><w:p>{}</w:p></w:tc>
          <w:tc><w:tcPr><w:gridSpan w:val="2"/><w:shd w:fill="DBEAFE"/></w:tcPr><w:p>{}</w:p></w:tc>
        </w:tr>
        <w:tr>
          <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p>{}</w:p></w:tc>
          <w:tc><w:p>{}</w:p></w:tc>
          <w:tc><w:p>{}</w:p></w:tc>
        </w:tr>
        <w:tr>
          <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p><w:r><w:t></w:t></w:r></w:p></w:tc>
          <w:tc><w:p>{}</w:p></w:tc>
          <w:tc><w:p>{}</w:p></w:tc>
        </w:tr>
        {}
      </w:tbl>
    "#,
        tnr_run("BỘ TÀI CHÍNH", "26", "<w:b/>"),
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        inline_img_para("rId3", "LogoTest", 1440000, 720000),
        tnr_run("STT", "24", "<w:b/>"),
        tnr_run("Nội dung &amp; Trạng thái (gộp 2 cột)", "24", "<w:b/>"),
        tnr_run("A", "24", "<w:b/>"),
        tnr_run("Hàng gộp dọc dòng 1", "24", ""),
        tnr_run("OK", "24", ""),
        tnr_run("Hàng gộp dọc dòng 2", "24", ""),
        tnr_run("OK", "24", ""),
        data_rows
    );

    build_preview_fixture_docx(
        &src,
        &body,
        None,
        vec![("word/media/logo.png", minimal_png_bytes())],
        r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/logo.png"/>"#,
        "",
    );

    let req = PreviewBuildRequest {
        job_id: "c03".to_string(),
        path: src.to_string_lossy().to_string(),
        ..Default::default()
    };
    let session = build_live_preview(&req, None);
    let doc = session.before.unwrap();
    assert!(doc.total_pages >= 2, "Bảng 40+ dòng phải phân tách >= 2 trang");
    assert_eq!(doc.image_count, 1);

    let first_pg_blocks = &doc.pages[0].blocks;
    let admin_tbl = match &first_pg_blocks[0] {
        PreviewBlock::Table(t) => t,
        _ => panic!("Khối 0 phải là bảng Quốc hiệu"),
    };
    assert_eq!(admin_tbl.table_role, "adminHeader");
    assert!(!admin_tbl.borders.top.visible);

    let data_tbl_p1 = first_pg_blocks
        .iter()
        .find_map(|b| match b {
            PreviewBlock::Table(t) if t.table_role == "dataTable" => Some(t),
            _ => None,
        })
        .unwrap();
    assert!(data_tbl_p1.borders.top.visible);
    assert_eq!(data_tbl_p1.rows[0].cells[1].col_span, 2);
    assert_eq!(data_tbl_p1.rows[0].cells[0].shading_hex.as_deref(), Some("DBEAFE"));
    assert_eq!(data_tbl_p1.rows[1].cells[0].row_span, 2);
    assert!(data_tbl_p1.rows[2].cells[0].merged_continuation);

    let data_tbl_p2 = doc.pages[1]
        .blocks
        .iter()
        .find_map(|b| match b {
            PreviewBlock::Table(t) if t.table_role == "dataTable" => Some(t),
            _ => None,
        })
        .expect("Trang 2 phải tiếp nối bảng dữ liệu");
    assert!(data_tbl_p2.rows[0].is_header_row, "Phải lặp tiêu đề tblHeader ở đầu trang 2");

    // Lazy media loading & path traversal prevention
    let data_uri = load_preview_media_base64(&src, "word/media/logo.png").unwrap();
    assert!(data_uri.starts_with("data:image/png;base64,"));
    assert!(load_preview_media_base64(&src, "../word/document.xml").is_err());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_04_live_preview_incremental_patch_and_template_header_updates() {
    let dir = make_temp_preview_dir("c04_live_incremental");
    let src = dir.join("source_untouched.docx");

    let wrong_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>
    "#,
        custom_font_run("Arial", "Đoạn văn bản gốc chưa chuẩn hóa để xem thử thay đổi.", "24", ""),
        custom_font_run("Arial", "Đoạn thứ hai dùng Arial 12pt căn trái.", "24", ""),
    );
    build_preview_fixture_docx(&src, &body, Some(wrong_sect), vec![], "", "");

    let sha_original = sha256_hex(&fs::read(&src).unwrap());

    // 1. Prime cache
    let _ = build_live_preview(
        &PreviewBuildRequest {
            job_id: "c04_init".to_string(),
            path: src.to_string_lossy().to_string(),
            ..Default::default()
        },
        None,
    );

    // 2. Fast incremental patch (Font + Size + Alignment + A4 + Margins)
    let inc_req = PreviewBuildRequest {
        job_id: "c04_inc".to_string(),
        path: src.to_string_lossy().to_string(),
        include_after: true,
        prefer_incremental: true,
        overrides: PreviewOverrideConfig {
            enabled: true,
            font_name: Some("Times New Roman".to_string()),
            font_size_pt: Some(14.0),
            body_alignment: Some("justify".to_string()),
            line_spacing_mult: Some(1.3),
            space_after_pt: Some(6.0),
            paper_size: Some(PaperSize::A4),
            margin_left_cm: Some(3.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let inc_session = build_live_preview(&inc_req, None);
    assert_eq!(inc_session.status, "completed");
    assert_eq!(inc_session.update_mode, "incrementalPatch");
    let after_inc = inc_session.after.unwrap();
    assert_eq!(after_inc.pages[0].paper_label, "A4");
    assert!((after_inc.pages[0].margin_left_mm - 30.0).abs() <= 0.5);
    match &after_inc.pages[0].blocks[0] {
        PreviewBlock::Paragraph(p) => {
            assert_eq!(p.alignment, "justify");
            assert_eq!(p.spans[0].font_family, "Times New Roman");
            assert_eq!(p.spans[0].font_size_pt, 14.0);
        }
        _ => panic!("Expected paragraph"),
    }

    let test_org = OrganizationProfile {
        id: "org-preview-test".to_string(),
        profile_name: "Sở Xây dựng (Test)".to_string(),
        sector: OrgSector::StateAgency,
        governing_org: "ỦY BAN NHÂN DÂN TỈNH".to_string(),
        issuing_org: "SỞ XÂY DỰNG".to_string(),
        agency_abbreviation: "SXD".to_string(),
        default_location: "Hà Nội".to_string(),
        show_governing_org_default: true,
        ..Default::default()
    };

    // 3a. Live HeaderBuilder CreateNew on unheaded doc
    let hdr_req = PreviewBuildRequest {
        job_id: "c04_hdr".to_string(),
        path: src.to_string_lossy().to_string(),
        include_after: true,
        overrides: PreviewOverrideConfig {
            enabled: true,
            apply_header_mode: Some(HeaderActionMode::CreateNew),
            header_config: Some(HeaderBuilderConfig {
                doc_type: AdminDocType::CongVan,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let hdr_session = build_live_preview(&hdr_req, Some(&test_org));
    assert_eq!(hdr_session.status, "completed");
    assert_eq!(hdr_session.update_mode, "fullRebuild");
    assert!(hdr_session.after.is_some());

    // 3b. Full in-memory transform with Template + Header StandardizeExisting
    let tpl_req = PreviewBuildRequest {
        job_id: "c04_tpl".to_string(),
        path: src.to_string_lossy().to_string(),
        include_after: true,
        overrides: PreviewOverrideConfig {
            enabled: true,
            template_id: Some("adm-cong-van".to_string()),
            apply_header_mode: Some(HeaderActionMode::StandardizeExisting),
            header_config: Some(HeaderBuilderConfig {
                doc_type: AdminDocType::CongVan,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let tpl_session = build_live_preview(&tpl_req, Some(&test_org));
    assert_eq!(tpl_session.status, "completed");
    assert_eq!(tpl_session.update_mode, "fullRebuild");
    assert!(tpl_session.after.is_some());

    // Source file MUST be completely unmodified on disk!
    assert_eq!(sha256_hex(&fs::read(&src).unwrap()), sha_original);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_05_before_after_comparison_and_diff_highlighting() {
    let dir = make_temp_preview_dir("c05_diffs");
    let src = dir.join("compare_diff.docx");

    let wrong_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"<w:p><w:pPr><w:jc w:val="left"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{}</w:p>"#,
        custom_font_run("Arial", "Đoạn cần làm nổi bật khác biệt Trước và Sau.", "22", ""),
    );
    build_preview_fixture_docx(&src, &body, Some(wrong_sect), vec![], "", "");

    let req = PreviewBuildRequest {
        job_id: "c05".to_string(),
        path: src.to_string_lossy().to_string(),
        include_after: true,
        overrides: PreviewOverrideConfig {
            enabled: true,
            font_name: Some("Times New Roman".to_string()),
            font_size_pt: Some(14.0),
            body_alignment: Some("justify".to_string()),
            paper_size: Some(PaperSize::A4),
            margin_left_cm: Some(3.0),
            ..Default::default()
        },
        ..Default::default()
    };

    let session = build_live_preview(&req, None);
    assert!(!session.diffs.is_empty());
    assert!(session.diffs.iter().any(|d| d.change_kinds.contains(&"pageSetup".to_string())));
    let p_diff = session
        .diffs
        .iter()
        .find(|d| d.element_path == "body/p[0]")
        .expect("Phải có diff cho đoạn body/p[0]");
    assert_eq!(p_diff.before_page, Some(1));
    assert_eq!(p_diff.after_page, Some(1));
    assert!(p_diff.change_kinds.contains(&"typography".to_string()));
    assert!(p_diff.change_kinds.contains(&"alignment".to_string()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_06_audit_report_bidirectional_location_sync() {
    let dir = make_temp_preview_dir("c06_audit_sync");
    let src = dir.join("audit_sync.docx");

    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>{}</w:p>
      <w:p><w:r><w:br w:type="page"/></w:r>{}</w:p>
      <w:p><w:pPr><w:pStyle w:val="Heading3"/></w:pPr>{}</w:p>
    "#,
        tnr_run("QUYẾT ĐỊNH", "28", "<w:b/>"),
        tnr_run("I. PHẦN ĐẦU TIÊN", "30", "<w:b/>"),
        custom_font_run("Arial", "Đoạn sai font Arial nằm ở trang 2 cần đồng bộ đúng số trang.", "24", ""),
        tnr_run("1.1. Mục nhảy cấp trang 2", "26", "<w:b/><w:i/>"),
    );
    build_preview_fixture_docx(&src, &body, None, vec![], "", "");

    let req = PreviewBuildRequest {
        job_id: "c06".to_string(),
        path: src.to_string_lossy().to_string(),
        include_audit_sync: true,
        scope: StdDocScope::AdminNd30,
        ..Default::default()
    };
    let session = build_live_preview(&req, None);
    assert!(!session.audit_markers.is_empty());

    let p2_marker = session
        .audit_markers
        .iter()
        .find(|m| {
            m.page_number == 2
                && (m.rule_or_issue_id == "FONT-01"
                    || m.rule_or_issue_id == "FONT-02"
                    || m.rule_or_issue_id.starts_with("ANOM-H-"))
        })
        .expect("Phải có lỗi Audit trỏ về phần tử nằm ở trang 2");
    assert_eq!(p2_marker.page_number, 2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_07_print_preview_diagnostics_clipped_and_overflowing_tables() {
    let dir = make_temp_preview_dir("c07_print_check");
    let src = dir.join("print_warnings.docx");

    let clipped_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="170" w:right="170" w:bottom="170" w:left="170" w:header="140" w:footer="140" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"
      {}
      <w:tbl>
        <w:tblPr><w:tblW w:w="13500" w:type="dxa"/></w:tblPr>
        <w:tblGrid><w:gridCol w:w="6500"/><w:gridCol w:w="7000"/></w:tblGrid>
        <w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>
      </w:tbl>
    "#,
        inline_img_para("rId3", "WideImg", 8_000_000, 1_500_000),
        tnr_run("Cột 1 quá rộng", "26", ""),
        tnr_run("Cột 2 tràn khổ", "26", ""),
    );
    build_preview_fixture_docx(
        &src,
        &body,
        Some(clipped_sect),
        vec![("word/media/wide.png", minimal_png_bytes())],
        r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/wide.png"/>"#,
        "",
    );

    let req = PreviewBuildRequest {
        job_id: "c07".to_string(),
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        ..Default::default()
    };
    let session = build_live_preview(&req, None);
    let doc = session.before.unwrap();
    let print = doc.print_check;
    assert!(!print.is_print_ready);
    assert!(print.clipped_count >= 2);
    assert!(print.overflow_table_count >= 1);
    assert!(print.diagnostics.iter().any(|d| d.kind == "paperSize"));
    assert!(print.diagnostics.iter().any(|d| d.kind == "clippedContent"));
    assert!(print.diagnostics.iter().any(|d| d.kind == "tableOverflow"));
    assert!(doc.pages[0].has_overflow_warning);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_08_performance_large_multipage_doc_low_ram_and_cancellation() {
    let dir = make_temp_preview_dir("c08_perf_cancel");
    let src = dir.join("large_multipage.docx");

    let mut body = inline_img_para("rId3", "HeavyLogo", 1_440_000, 720_000);
    for i in 1..=140 {
        body.push_str(&format!(
            r#"<w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>"#,
            tnr_run(
                &format!(
                    "Điều {}. Kiểm thử hiệu năng dựng trang theo nhu cầu đối với tài liệu nhiều trang để đảm bảo không nhúng dữ liệu nhị phân hình ảnh trực tiếp vào cấu trúc JSON phân trang tổng.",
                    i
                ),
                "28",
                ""
            )
        ));
    }
    let heavy_payload = vec![0xAAu8; 32_768];
    build_preview_fixture_docx(
        &src,
        &body,
        None,
        vec![("word/media/heavy.png", heavy_payload)],
        r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/heavy.png"/>"#,
        "",
    );

    let req = PreviewBuildRequest {
        job_id: "c08_perf".to_string(),
        path: src.to_string_lossy().to_string(),
        ..Default::default()
    };
    let session = build_live_preview(&req, None);
    assert_eq!(session.status, "completed");
    let doc = session.before.as_ref().unwrap();
    assert!(doc.total_pages >= 6, "Tài liệu 140 đoạn phải có >= 6 trang (thực tế: {})", doc.total_pages);
    assert!(session.elapsed_ms < 5000);

    // Payload MUST NOT embed raw base64/binary bytes
    let json_len = serde_json::to_string(&session).unwrap().len();
    assert!(!serde_json::to_string(&session).unwrap().contains("base64,"), "Không được nhúng sẵn ảnh base64 vào payload phân trang!");
    assert!(json_len < 600_000);

    // Verify cooperative cancel registry
    let tok = live_preview::cancel::start_job("cancel_unit_job");
    assert!(!tok.is_cancelled());
    assert!(cancel_preview_render("cancel_unit_job"));
    assert!(tok.is_cancelled());
    live_preview::cancel::finish_job("cancel_unit_job");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preview_case_09_real_microsoft_word_com_pdf_ground_truth_and_non_destructive_save() {
    let dir = make_temp_preview_dir("c09_word_com");
    let src = dir.join("ground_truth_source.docx");

    let wrong_sect = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
    "#,
        tnr_run("THÔNG BÁO", "28", "<w:b/>"),
        custom_font_run("Arial", "Văn bản kiểm chứng đối chiếu Microsoft Word COM thật trong Phase 7.", "24", ""),
        custom_font_run("Arial", "Kiểm tra kết xuất PDF tạm và bảo toàn luồng lưu văn bản gốc.", "24", ""),
    );
    build_preview_fixture_docx(&src, &body, Some(wrong_sect), vec![], "", "");
    let orig_sha = sha256_hex(&fs::read(&src).unwrap());

    let req = PreviewBuildRequest {
        job_id: "c09_word".to_string(),
        path: src.to_string_lossy().to_string(),
        include_after: true,
        overrides: PreviewOverrideConfig {
            enabled: true,
            font_name: Some("Times New Roman".to_string()),
            font_size_pt: Some(14.0),
            paper_size: Some(PaperSize::A4),
            margin_left_cm: Some(3.0),
            apply_std_group_a: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let ver = render_word_com_verification(&req, "after").unwrap();
    assert!(!ver.cancelled);
    assert!(ver.word_page_count >= 1);
    assert!(ver.pdf_url_path.starts_with("/__preview_pdf__/"));
    let pdf_path = Path::new(&ver.pdf_disk_path);
    assert!(pdf_path.is_file());
    let pdf_bytes = fs::read(pdf_path).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));
    assert!(pdf_bytes.len() > 5_000);
    println!(
        "WORD_PREVIEW_VERIFIED|word_pages={}|openxml_pages={}|delta={}|pdf_bytes={}",
        ver.word_page_count,
        ver.openxml_estimated_pages,
        ver.page_count_delta,
        pdf_bytes.len()
    );

    // Original source unchanged
    assert_eq!(sha256_hex(&fs::read(&src).unwrap()), orig_sha);

    // Normal edit & save workflow still intact
    let cfg = ProcessConfig::default();
    let save_res = process_single_file(&src, &cfg);
    assert!(save_res.success, "Save failed: {:?}", save_res.error);
    let out_pkg = OpenXmlPackage::open(Path::new(&save_res.output_path)).unwrap();
    assert!(out_pkg.is_word());

    let _ = fs::remove_file(&ver.pdf_disk_path);
    let _ = fs::remove_file(&ver.temp_docx_path);
    let _ = fs::remove_dir_all(&dir);
}
