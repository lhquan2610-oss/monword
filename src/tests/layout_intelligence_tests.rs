use crate::engine::layout_intelligence;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::std_engine;
use crate::engine::template_engine::{self, TemplateStore};
use crate::engine::xml_helpers::XmlDocument;
use crate::models::{
    DynamicTemplateFields, LayoutExecutionRequest, LayoutProposalTier, OrgSector,
    OrganizationProfile, OutputMode, RecognizedStructureKind, SmartProposalKind, StdDocScope,
    TemplateApplyRequest, TemplateTargetMode,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::CompressionMethod;

fn make_temp_layout_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "monword_layout_test_{}_{}",
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

fn inline_image_paragraph_xml(rid: &str, name: &str, cx: i64, cy: i64) -> String {
    format!(
        r#"<w:p>
  <w:pPr><w:jc w:val="center"/></w:pPr>
  <w:r>
    <w:drawing>
      <wp:inline distT="0" distB="0" distL="0" distR="0">
        <wp:extent cx="{cx}" cy="{cy}"/>
        <wp:effectExtent l="0" t="0" r="0" b="0"/>
        <wp:docPr id="1" name="{name}"/>
        <wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/></wp:cNvGraphicFramePr>
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

fn word_textbox_paragraph_xml(id: usize, text_line_1: &str, text_line_2: &str) -> String {
    format!(
        r#"<w:p>
  <w:r>
    <mc:AlternateContent>
      <mc:Choice Requires="wps">
        <w:drawing>
          <wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="251658240" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">
            <wp:simplePos x="0" y="0"/>
            <wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH>
            <wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>
            <wp:extent cx="2880000" cy="900000"/>
            <wp:effectExtent l="0" t="0" r="0" b="0"/>
            <wp:wrapNone/>
            <wp:docPr id="{id}" name="TextBox {id}"/>
            <wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"/></wp:cNvGraphicFramePr>
            <a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
                <wps:wsp>
                  <wps:cNvSpPr txBox="1"/>
                  <wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2880000" cy="900000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr>
                  <wps:txbx>
                    <w:txbxContent>
                      <w:p><w:r><w:t>{t1}</w:t></w:r></w:p>
                      <w:p><w:r><w:t>{t2}</w:t></w:r></w:p>
                    </w:txbxContent>
                  </wps:txbx>
                  <wps:bodyPr/>
                </wps:wsp>
              </a:graphicData>
            </a:graphic>
          </wp:anchor>
        </w:drawing>
      </mc:Choice>
      <mc:Fallback>
        <w:pict>
          <v:rect id="vtb{id}" style="width:200pt;height:60pt">
            <v:textbox>
              <w:txbxContent>
                <w:p><w:r><w:t>{t1}</w:t></w:r></w:p>
                <w:p><w:r><w:t>{t2}</w:t></w:r></w:p>
              </w:txbxContent>
            </v:textbox>
          </v:rect>
        </w:pict>
      </mc:Fallback>
    </mc:AlternateContent>
  </w:r>
</w:p>"#,
        id = id,
        t1 = text_line_1,
        t2 = text_line_2
    )
}

#[allow(clippy::too_many_arguments)]
fn build_layout_fixture_docx(
    path: &Path,
    body_inner_xml: &str,
    custom_styles_xml: Option<&str>,
    extra_entries: Vec<(&str, Vec<u8>)>,
    extra_rels: &str,
    extra_ct: &str,
    omit_default_sect_pr: bool,
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
            xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
            xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
            xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
            xmlns:v="urn:schemas-microsoft-com:vml"
            mc:Ignorable="wps">
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

    let default_styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Heading1"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Heading2"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/><w:i/><w:sz w:val="26"/></w:rPr></w:style>
</w:styles>"#;
    pkg.set_bytes(
        "word/styles.xml",
        custom_styles_xml.unwrap_or(default_styles).as_bytes().to_vec(),
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

fn verify_docs_in_real_word_com(paths: &[(&Path, &str, &str)]) {
    let checks_ps = paths
        .iter()
        .map(|(p, expected_body, expected_txbx)| {
            format!(
                r#"$p = '{}'
$fname = [System.IO.Path]::GetFileName($p)
try {{
  $d = $word.Documents.Open($p, $false, $true)
  try {{
    if (-not $d.Content.Text.Contains('{}')) {{
      throw "Thieu chuoi than van '{}' trong $fname"
    }}
    $allShapeTxt = ""
    foreach ($sh in $d.Shapes) {{
      try {{
        if ($sh.TextFrame.HasText) {{ $allShapeTxt += "`n" + $sh.TextFrame.TextRange.Text }}
      }} catch {{}}
    }}
    if (-not $allShapeTxt.Contains('{}')) {{
      throw "Thieu chuoi TextBox '{}' trong Shapes cua $fname"
    }}
    Write-Output ("WORD_OK|" + $fname + "|sec=" + $d.Sections.Count + "|tbl=" + $d.Tables.Count + "|shp=" + $d.Shapes.Count + "|ishp=" + $d.InlineShapes.Count + "|links=" + $d.Hyperlinks.Count + "|bmk=" + $d.Bookmarks.Count + "|fn=" + $d.Footnotes.Count)
  }} finally {{
    $d.Close([ref]$false)
  }}
}} catch {{
  throw "FAIL_FILE [$fname]: $($_.Exception.Message)"
}}"#,
                p.to_string_lossy().replace('\'', "''"),
                expected_body.replace('\'', "''"),
                expected_body.replace('\'', "''"),
                expected_txbx.replace('\'', "''"),
                expected_txbx.replace('\'', "''"),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let ps = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $word.AutomationSecurity = 3
  {}
}} finally {{
  if ($word -ne $null) {{
    $word.Quit([ref]$false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
  }}
}}
"#,
        checks_ps
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
        .expect("Khong goi duoc PowerShell Word COM");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("Word COM Layout Verification:\n{}", stdout);
    assert!(
        out.status.success(),
        "Word COM that bai:\nSTDOUT:\n{}\nSTDERR:\n{}",
        stdout,
        stderr
    );
}

#[test]
fn layout_case_01_structure_recognition_context_vs_keyword() {
    let dir = make_temp_layout_dir("c01");

    // Doc A: Administrative with header table, DocTypeTitle, DocSubject, inline citation vs real Appendix after signature
    let doc_a = dir.join("admin_context.docx");
    let body_a = r#"
<w:tbl>
  <w:tblPr><w:tblW w:w="9355" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="5355"/></w:tblGrid>
  <w:tr>
    <w:tc><w:p><w:r><w:t>SỞ TÀI CHÍNH</w:t></w:r></w:p><w:p><w:r><w:t>Số: 12/BC-STC</w:t></w:r></w:p></w:tc>
    <w:tc><w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p><w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p></w:tc>
  </w:tr>
</w:tbl>
<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>BÁO CÁO</w:t></w:r></w:p>
<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>V/v Tình hình triển khai chuyển đổi số năm 2026</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>I. KẾT QUẢ THỰC HIỆN</w:t></w:r></w:p>
<w:p><w:r><w:t>Chi tiết các chỉ tiêu tổng hợp được thực hiện theo Phụ lục 1 kèm theo báo cáo này nhằm đối chiếu số liệu.</w:t></w:r></w:p>
<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:i/></w:rPr><w:t>Bảng 1: Số liệu thống kê hệ thống</w:t></w:r></w:p>
<w:tbl>
  <w:tblPr><w:tblW w:w="9355" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="6355"/></w:tblGrid>
  <w:tr><w:tc><w:p><w:r><w:t>Chỉ tiêu</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Giá trị</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:tbl>
  <w:tblPr><w:tblW w:w="9355" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="5355"/></w:tblGrid>
  <w:tr>
    <w:tc><w:p><w:r><w:t>Nơi nhận:</w:t></w:r></w:p><w:p><w:r><w:t>- Như trên;</w:t></w:r></w:p></w:tc>
    <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>GIÁM ĐỐC</w:t></w:r></w:p><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Nguyễn Văn Minh</w:t></w:r></w:p></w:tc>
  </w:tr>
</w:tbl>
<w:p><w:pPr><w:pageBreakBefore/><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>PHỤ LỤC I</w:t></w:r></w:p>
"#;
    build_layout_fixture_docx(&doc_a, body_a, None, vec![], "", "", false);
    let rep_a = layout_intelligence::analyze_file(&doc_a, StdDocScope::AutoDetect, None).unwrap();

    assert!(rep_a.maps.document_map.iter().any(|n| n.kind == RecognizedStructureKind::DocumentHeader));
    assert!(rep_a.maps.document_map.iter().any(|n| n.kind == RecognizedStructureKind::DocTypeTitle));
    assert!(rep_a.maps.document_map.iter().any(|n| n.kind == RecognizedStructureKind::DocSubject));
    assert!(rep_a.maps.document_map.iter().any(|n| n.kind == RecognizedStructureKind::SignatureBlock));
    assert!(rep_a.maps.document_map.iter().any(|n| n.kind == RecognizedStructureKind::RecipientBlock));

    // The inline citation mentioning "Phụ lục 1" MUST remain MainBody, NOT AppendixBlock
    let citation_p = rep_a
        .maps
        .paragraph_map
        .iter()
        .find(|p| p.excerpt.contains("theo Phụ lục 1"))
        .unwrap();
    assert_eq!(citation_p.kind, RecognizedStructureKind::MainBody);

    // Table caption MUST remain MainBody, NOT Heading
    let caption_p = rep_a
        .maps
        .paragraph_map
        .iter()
        .find(|p| p.excerpt.contains("Bảng 1:"))
        .unwrap();
    assert_eq!(caption_p.kind, RecognizedStructureKind::MainBody);

    // Real Appendix after signature table MUST be AppendixBlock
    let real_app = rep_a
        .maps
        .paragraph_map
        .iter()
        .find(|p| p.excerpt == "PHỤ LỤC I")
        .unwrap();
    assert_eq!(real_app.kind, RecognizedStructureKind::AppendixBlock);

    // Doc B: Enterprise SOP with MainTitle (no NĐ30 DocTypeTitle)
    let doc_b = dir.join("enterprise_title.docx");
    let body_b = r#"
<w:p><w:pPr><w:pStyle w:val="Title"/><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="36"/></w:rPr><w:t>QUY TRÌNH QUẢN LÝ RỦI RO VẬN HÀNH DOANH NGHIỆP</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>1. MỤC ĐÍCH VÀ PHẠM VI</w:t></w:r></w:p>
<w:p><w:r><w:t>Quy trình này áp dụng cho toàn bộ các đơn vị trực thuộc doanh nghiệp.</w:t></w:r></w:p>
"#;
    build_layout_fixture_docx(&doc_b, body_b, None, vec![], "", "", false);
    let rep_b = layout_intelligence::analyze_file(&doc_b, StdDocScope::EnterpriseInternal, None).unwrap();
    assert!(
        rep_b.maps.document_map.iter().any(|n| n.kind == RecognizedStructureKind::MainTitle),
        "Tai lieu doanh nghiep phai nhan dien duoc MainTitle"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_02_multi_chapter_and_4_level_headings_with_numbering_xml() {
    let dir = make_temp_layout_dir("c02");
    let path = dir.join("multi_chapter_4lvl.docx");

    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0"><w:numFmt w:val="upperRoman"/><w:lvlText w:val="%1."/></w:lvl>
    <w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/><w:lvlText w:val="%2."/></w:lvl>
    <w:lvl w:ilvl="2"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2.%3"/></w:lvl>
    <w:lvl w:ilvl="3"><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%4)"/></w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;

    let body = r#"
<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:keepNext/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>CHƯƠNG I: TỔNG QUAN HỆ THỐNG</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading2"/><w:keepNext/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>1. Kiến trúc tổng thể</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading3"/><w:keepNext/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>1.1. Tầng xử lý OpenXML</w:t></w:r></w:p>
<w:p><w:pPr><w:outlineLvl w:val="3"/><w:numPr><w:ilvl w:val="3"/><w:numId w:val="1"/></w:numPr><w:keepNext/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>a) Bộ phân tích DOM không mất dữ liệu</w:t></w:r></w:p>
<w:p><w:r><w:t>Nội dung giải thích chi tiết tầng OpenXML bảo toàn nguyên trạng quan hệ tài liệu.</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:keepNext/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>CHƯƠNG II: TỐI ƯU HIỆU NĂNG</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr><w:keepNext/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="30"/></w:rPr><w:t>Cơ chế đọc chọn lọc ZIP</w:t></w:r></w:p>
"#;
    build_layout_fixture_docx(
        &path,
        body,
        None,
        vec![("word/numbering.xml", numbering_xml.as_bytes().to_vec())],
        r#"<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
        r#"<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"#,
        false,
    );

    let rep = layout_intelligence::analyze_file(&path, StdDocScope::AcademicThesis, None).unwrap();
    let h1: Vec<_> = rep.maps.document_map.iter().filter(|n| n.kind == RecognizedStructureKind::HeadingLevel1).collect();
    let h2: Vec<_> = rep.maps.document_map.iter().filter(|n| n.kind == RecognizedStructureKind::HeadingLevel2).collect();
    let h3: Vec<_> = rep.maps.document_map.iter().filter(|n| n.kind == RecognizedStructureKind::HeadingLevel3).collect();
    let h4: Vec<_> = rep.maps.document_map.iter().filter(|n| n.kind == RecognizedStructureKind::HeadingLevel4).collect();

    assert_eq!(h1.len(), 2, "Phai nhan dien 2 chuong H1");
    assert_eq!(h2.len(), 2, "Phai nhan dien 2 muc H2 (gom 1 muc qua word/numbering.xml)");
    assert_eq!(h3.len(), 1);
    assert_eq!(h4.len(), 1);

    // Check hierarchical parent chain: H2 -> H1[0], H3 -> H2[0], H4 -> H3[0]
    assert_eq!(h2[0].parent_id.as_deref(), Some(h1[0].node_id.as_str()));
    assert_eq!(h3[0].parent_id.as_deref(), Some(h2[0].node_id.as_str()));
    assert_eq!(h4[0].parent_id.as_deref(), Some(h3[0].node_id.as_str()));
    assert_eq!(h2[1].parent_id.as_deref(), Some(h1[1].node_id.as_str()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_03_multi_table_overflow_and_orientation_recommendation() {
    let dir = make_temp_layout_dir("c03");
    let path = dir.join("multi_table_overflow.docx");

    let body = r#"
<w:tbl>
  <w:tblPr><w:tblW w:w="9355" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="5355"/></w:tblGrid>
  <w:tr><w:tc><w:p><w:r><w:t>UBND THÀNH PHỐ</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p><w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:p><w:r><w:t>Bảng 1 đạt chuẩn khổ in và bảng 2 gồm 8 cột bị tràn lề phải.</w:t></w:r></w:p>
<w:tbl>
  <w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:jc w:val="center"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="4500"/><w:gridCol w:w="4500"/></w:tblGrid>
  <w:tr><w:tc><w:p><w:r><w:t>Cột 1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cột 2</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:tbl>
  <w:tblPr><w:tblW w:w="13600" w:type="dxa"/><w:tblInd w:w="400" w:type="dxa"/></w:tblPr>
  <w:tblGrid>
    <w:gridCol w:w="1700"/><w:gridCol w:w="1700"/><w:gridCol w:w="1700"/><w:gridCol w:w="1700"/>
    <w:gridCol w:w="1700"/><w:gridCol w:w="1700"/><w:gridCol w:w="1700"/><w:gridCol w:w="1700"/>
  </w:tblGrid>
  <w:tr>
    <w:tc><w:p><w:r><w:t>STT</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Mã</w:t></w:r></w:p></w:tc>
    <w:tc><w:p><w:r><w:t>Nội dung</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>ĐVT</w:t></w:r></w:p></w:tc>
    <w:tc><w:p><w:r><w:t>Q1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Q2</w:t></w:r></w:p></w:tc>
    <w:tc><w:p><w:r><w:t>Q3</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Q4</w:t></w:r></w:p></w:tc>
  </w:tr>
</w:tbl>
"#;
    let custom_styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:b/><w:sz w:val="30"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Heading1"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Heading2"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>
</w:styles>"#;
    build_layout_fixture_docx(&path, body, Some(custom_styles), vec![], "", "", false);
    let rep = layout_intelligence::analyze_file(&path, StdDocScope::AdminNd30, None).unwrap();

    assert_eq!(rep.maps.table_map.len(), 3);
    assert_eq!(rep.maps.table_map[0].table_role, "adminHeader");
    assert_eq!(rep.maps.table_map[1].table_role, "dataTable");
    assert!(!rep.maps.table_map[1].overflows_width);
    assert!(rep.maps.table_map[2].overflows_width);
    assert_eq!(rep.maps.table_map[2].recommended_orientation, "landscape");

    let orient_prop = rep
        .proposals
        .iter()
        .find(|p| p.kind == SmartProposalKind::RecommendOrientation)
        .expect("Phai co de xuat xoay huong giay cho bang 8 cot tran le");
    assert_eq!(orient_prop.tier, LayoutProposalTier::RequiresConfirmation);
    assert!(orient_prop.affects_section);

    let fit_prop = rep
        .proposals
        .iter()
        .find(|p| p.kind == SmartProposalKind::ResolveOverflow)
        .expect("Phai co de xuat co bang vua vung in");
    assert_eq!(fit_prop.tier, LayoutProposalTier::RequiresConfirmation);
    assert!(fit_prop.affects_table);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_04_embedded_and_overflowing_images_preserve_rels() {
    let dir = make_temp_layout_dir("c04");
    let path = dir.join("images_overflow.docx");

    // Image 1: 40mm x 20mm (1,440,000 x 720,000 EMU); Image 2: 300mm x 150mm (10,800,000 x 5,400,000 EMU -> overflows 165mm)
    let p_ok = inline_image_paragraph_xml("rId3", "OkImg.png", 1_440_000, 720_000);
    let p_big = inline_image_paragraph_xml("rId4", "WideDiagram.png", 10_800_000, 5_400_000);
    let body = format!(
        "<w:p><w:r><w:t>Hình minh họa kích thước hợp lệ và hình vượt khổ trang.</w:t></w:r></w:p>{}\n{}",
        p_ok, p_big
    );

    build_layout_fixture_docx(
        &path,
        &body,
        None,
        vec![
            ("word/media/image1.png", minimal_png_bytes()),
            ("word/media/image2.png", minimal_png_bytes()),
        ],
        r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image2.png"/>"#,
        "",
        false,
    );

    let rep = layout_intelligence::analyze_file(&path, StdDocScope::EnterpriseInternal, None).unwrap();
    let imgs: Vec<_> = rep
        .maps
        .visual_objects
        .iter()
        .filter(|v| v.object_kind == "image")
        .collect();
    assert_eq!(imgs.len(), 2);
    assert!(!imgs[0].overflows_printable_area);
    assert!(imgs[1].overflows_printable_area);

    let fit_img_prop = rep
        .proposals
        .iter()
        .find(|p| {
            p.kind == SmartProposalKind::ResolveOverflow
                && p.param_value.as_deref().unwrap_or("").starts_with("fitInlineImage:")
        })
        .expect("Phai sinh de xuat thu nho anh tran trang");

    let exec_res = layout_intelligence::execute_proposals(
        &LayoutExecutionRequest {
            path: path.to_string_lossy().to_string(),
            scope: Some(StdDocScope::EnterpriseInternal),
            output_mode: OutputMode::NewCopy,
            apply_high_confidence_auto: false,
            accepted_proposal_ids: vec![fit_img_prop.proposal_id.clone()],
            rejected_proposal_ids: vec![],
        },
        None,
    );
    assert!(exec_res.success);
    assert!(exec_res.content_preserved_verified);
    assert!(exec_res.relationships_preserved_verified);

    let after = exec_res.post_save_report.unwrap();
    assert_eq!(after.assets.media_count, 2);
    assert!(after.maps.visual_objects.iter().all(|v| !v.overflows_printable_area));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_05_multi_section_columns_and_landscape_pages() {
    let dir = make_temp_layout_dir("c05");
    let path = dir.join("multi_section_cols_landscape.docx");

    let hdr1 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>TIÊU ĐỀ ĐẦU TRANG SECTION 1</w:t></w:r></w:p></w:hdr>"#;
    let ftr1 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:fldSimple w:instr=" PAGE "/><w:r><w:t> / Trang cuối</w:t></w:r></w:p></w:ftr>"#;

    let body = r#"
<w:p><w:r><w:t>Phần mở đầu thuộc Section 1 khổ dọc 1 cột.</w:t></w:r></w:p>
<w:p>
  <w:pPr>
    <w:sectPr>
      <w:headerReference w:type="default" r:id="rId3"/>
      <w:footerReference w:type="default" r:id="rId4"/>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/>
      <w:cols w:num="1" w:space="720"/>
    </w:sectPr>
  </w:pPr>
</w:p>
<w:p><w:r><w:t>Nội dung Section 2 chia thành 2 cột song song nhau theo chuẩn tin tức kỹ thuật.</w:t></w:r></w:p>
<w:p>
  <w:pPr>
    <w:sectPr>
      <w:type w:val="nextPage"/>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/>
      <w:cols w:num="2" w:space="720" w:equalWidth="1"/>
    </w:sectPr>
  </w:pPr>
</w:p>
<w:p><w:r><w:t>Phụ lục Section 3 trình bày khổ giấy A4 Ngang.</w:t></w:r></w:p>
<w:sectPr>
  <w:type w:val="nextPage"/>
  <w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/>
  <w:pgMar w:top="1701" w:right="850" w:bottom="1134" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/>
  <w:cols w:num="1" w:space="720"/>
</w:sectPr>
"#;

    build_layout_fixture_docx(
        &path,
        body,
        None,
        vec![
            ("word/header1.xml", hdr1.as_bytes().to_vec()),
            ("word/footer1.xml", ftr1.as_bytes().to_vec()),
        ],
        r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>"#,
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#,
        true,
    );

    let rep = layout_intelligence::analyze_file(&path, StdDocScope::EnterpriseInternal, None).unwrap();
    assert_eq!(rep.maps.section_map.len(), 3);
    assert_eq!(rep.maps.section_map[0].column_count, 1);
    assert_eq!(rep.maps.section_map[0].header_ids.len(), 1);
    assert_eq!(rep.maps.section_map[0].footer_ids.len(), 1);
    assert_eq!(rep.maps.section_map[1].column_count, 2);
    assert_eq!(rep.maps.section_map[1].col_widths_mm.len(), 2);
    assert_eq!(rep.maps.section_map[2].orientation, "landscape");
    assert!(rep.maps.page_layout_map.iter().any(|p| p.orientation == "landscape"));
    assert!(rep.maps.header_footer_map.iter().any(|hf| hf.has_page_field));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_06_textbox_recognition_deduplication_and_preservation() {
    let dir = make_temp_layout_dir("c06");
    let path = dir.join("textbox_alt_content.docx");

    let tb_xml = word_textbox_paragraph_xml(
        99,
        "Ghi chú nổi bật bên trong Text Box dòng 1",
        "Thông số bảo toàn mc:Choice + mc:Fallback dòng 2",
    );
    let body = format!(
        "<w:p><w:r><w:t>Đoạn văn chính trước hộp thoại Text Box.</w:t></w:r></w:p>\n{}\n<w:p><w:pPr><w:jc w:val=\"left\"/><w:ind w:firstLine=\"0\"/></w:pPr><w:r><w:t>Đoạn văn dài hơn 40 ký tự để kiểm tra chuẩn hóa đoạn văn bản ngoài Text Box giữ nguyên mọi thứ bên trong hộp.</w:t></w:r></w:p>",
        tb_xml
    );
    build_layout_fixture_docx(&path, &body, None, vec![], "", "", false);

    let rep = layout_intelligence::analyze_file(&path, StdDocScope::EnterpriseInternal, None).unwrap();
    assert_eq!(
        rep.summary.text_box_count, 1,
        "mc:AlternateContent (Choice + Fallback) chi duoc dem dung 1 Text Box"
    );
    let txbx_paras: Vec<_> = rep
        .maps
        .paragraph_map
        .iter()
        .filter(|p| p.in_text_box_id.as_deref() == Some("txbx-1"))
        .collect();
    assert_eq!(txbx_paras.len(), 2, "2 doan ben trong Text Box khong duoc nhan doi");
    assert!(txbx_paras[0].excerpt.contains("Text Box dòng 1"));
    assert!(txbx_paras[1].excerpt.contains("Fallback dòng 2"));

    // Run Smart Formatting and assert BOTH Choice and Fallback Text Boxes remain 100% intact
    let exec_res = layout_intelligence::execute_proposals(
        &LayoutExecutionRequest {
            path: path.to_string_lossy().to_string(),
            scope: Some(StdDocScope::EnterpriseInternal),
            output_mode: OutputMode::NewCopy,
            apply_high_confidence_auto: true,
            accepted_proposal_ids: vec![],
            rejected_proposal_ids: vec![],
        },
        None,
    );
    assert!(exec_res.success);
    assert!(exec_res.content_preserved_verified);

    let out_pkg = OpenXmlPackage::open(Path::new(&exec_res.output_path)).unwrap();
    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains("<mc:Choice"));
    assert!(doc_xml.contains("<mc:Fallback"));
    assert!(doc_xml.contains("Text Box dòng 1"));
    assert!(doc_xml.contains("Fallback dòng 2"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_07_multi_style_widow_orphan_whitespace_and_misalignment() {
    let dir = make_temp_layout_dir("c07");
    let path = dir.join("styles_widow_whitespace.docx");

    let body = r#"
<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:jc w:val="left"/><w:ind w:firstLine="720"/></w:pPr><w:r><w:rPr><w:b/><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/></w:rPr><w:t>1. TIÊU ĐỀ LỆCH LỀ VÀ THIẾU KEEP_NEXT</w:t></w:r></w:p>
<w:p><w:pPr><w:widowControl w:val="0"/><w:spacing w:before="600" w:after="600"/></w:pPr><w:r><w:t>Đây là đoạn văn bản dài hơn hai trăm ký tự được tạo ra để mô phỏng một đoạn thuyết minh nhiều dòng tắt chế độ kiểm soát dòng đơn độc widowControl và bị đặt khoảng cách đoạn quá lớn tạo khoảng trắng thừa bất thường trong trang in tài liệu hiện tại.</w:t></w:r></w:p>
<w:p/>
<w:p/>
<w:p/>
<w:p><w:r><w:t>Đoạn tiếp theo sau ba dòng Enter trống liên tiếp.</w:t></w:r></w:p>
"#;
    let custom_styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:b/><w:sz w:val="30"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Heading1"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Heading2"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>
</w:styles>"#;
    build_layout_fixture_docx(&path, body, Some(custom_styles), vec![], "", "", false);

    let rep = layout_intelligence::analyze_file(&path, StdDocScope::AcademicThesis, None).unwrap();
    assert!(rep.maps.style_map.len() >= 4);
    assert!(rep.maps.style_map.iter().any(|s| s.style_id == "Heading1" && s.direct_conflict_count > 0));

    let has_kind = |k| rep.warnings.iter().any(|w| w.kind == k);
    assert!(has_kind(crate::models::LayoutWarningKind::WidowOrphanRisk));
    assert!(has_kind(crate::models::LayoutWarningKind::StrandedHeadingBottom));
    assert!(has_kind(crate::models::LayoutWarningKind::StackedEmptyParagraphs));
    assert!(has_kind(crate::models::LayoutWarningKind::ExcessiveSpacing));
    assert!(has_kind(crate::models::LayoutWarningKind::MisalignedComponent));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_08_confidence_gate_and_zero_content_mutation() {
    let dir = make_temp_layout_dir("c08");
    let path = dir.join("gate_check.docx");

    let body = r#"
<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:ind w:firstLine="720"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>I. MỞ ĐẦU</w:t></w:r></w:p>
<w:p><w:pPr><w:widowControl w:val="0"/><w:spacing w:before="600" w:after="400"/><w:jc w:val="left"/></w:pPr><w:r><w:t>Nội dung đoạn thân văn cần chuẩn hóa khoảng cách và định dạng căn đều không làm mất một ký tự nào.</w:t></w:r></w:p>
<w:tbl>
  <w:tblPr><w:tblW w:w="12800" w:type="dxa"/><w:tblInd w:w="300" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="6400"/><w:gridCol w:w="6400"/></w:tblGrid>
  <w:tr><w:tc><w:p><w:r><w:t>Cột A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cột B</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
"#;
    let custom_styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:b/><w:sz w:val="30"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Heading1"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Heading2"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>
</w:styles>"#;
    build_layout_fixture_docx(&path, body, Some(custom_styles), vec![], "", "", false);

    let rep = layout_intelligence::analyze_file(&path, StdDocScope::AdminNd30, None).unwrap();
    let confirm_ids: Vec<String> = rep
        .proposals
        .iter()
        .filter(|p| p.tier == LayoutProposalTier::RequiresConfirmation)
        .map(|p| p.proposal_id.clone())
        .collect();
    assert!(!confirm_ids.is_empty());

    // Step 1: Run ONLY HighConfidenceAuto with accepted_proposal_ids = empty
    let res1 = layout_intelligence::execute_proposals(
        &LayoutExecutionRequest {
            path: path.to_string_lossy().to_string(),
            scope: Some(StdDocScope::AdminNd30),
            output_mode: OutputMode::NewCopy,
            apply_high_confidence_auto: true,
            accepted_proposal_ids: vec![],
            rejected_proposal_ids: vec![],
        },
        None,
    );
    assert!(res1.success);
    assert!(res1.content_preserved_verified);
    // EVERY confirmation-required proposal MUST remain in skipped_proposal_ids!
    for cid in &confirm_ids {
        assert!(
            res1.skipped_proposal_ids.contains(cid),
            "De xuat can xac nhan {} KHONG duoc phep tu chay",
            cid
        );
    }

    // Verify auto-applied changes happened, while overflowing table stayed untouched
    let rep1 = res1.post_save_report.unwrap();
    assert!(rep1.maps.table_map[0].overflows_width);

    // Step 2: Explicitly accept first confirm proposal, reject last confirm proposal, with OverwriteWithBackup
    let orig_bytes = fs::read(&path).unwrap();
    let mut accepted = vec![confirm_ids[0].clone()];
    let rejected = if confirm_ids.len() > 1 {
        vec![confirm_ids[confirm_ids.len() - 1].clone()]
    } else {
        vec![]
    };
    if confirm_ids.len() > 2 {
        accepted.push(confirm_ids[1].clone());
    }

    let res2 = layout_intelligence::execute_proposals(
        &LayoutExecutionRequest {
            path: path.to_string_lossy().to_string(),
            scope: Some(StdDocScope::AdminNd30),
            output_mode: OutputMode::OverwriteWithBackup,
            apply_high_confidence_auto: true,
            accepted_proposal_ids: accepted,
            rejected_proposal_ids: rejected.clone(),
        },
        None,
    );
    assert!(res2.success);
    assert!(res2.content_preserved_verified);
    for rej in &rejected {
        assert!(res2.skipped_proposal_ids.contains(rej));
    }
    let bak = res2.backup_path.expect("Phai tao ban sao luu backup");
    crate::engine::restore_backup(&path, Path::new(&bak)).unwrap();
    assert_eq!(fs::read(&path).unwrap(), orig_bytes, "Khoi phuc backup phai tra lai tung byte ban goc");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_case_09_shared_mapping_with_phase3_and_phase4() {
    let dir = make_temp_layout_dir("c09");
    let path = dir.join("shared_p3_p4.docx");

    let body = r#"
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>1. NỘI DUNG CHUNG</w:t></w:r></w:p>
<w:p><w:r><w:t>Đoạn văn kiểm chứng dữ liệu dùng chung giữa Phase 3 và Phase 5.</w:t></w:r></w:p>
"#;
    let custom_styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:b/><w:sz w:val="30"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Heading1"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Heading2"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>
</w:styles>"#;
    build_layout_fixture_docx(&path, body, Some(custom_styles), vec![], "", "", false);

    // Phase 3 shared map call
    let (std_rep, _ir, shared_map) =
        std_engine::analyze_file_with_layout_map(&path, StdDocScope::AdminNd30, None).unwrap();
    assert!(!std_rep.rule_summaries.is_empty());
    assert!(!shared_map.document_map.is_empty());
    assert!(!shared_map.section_map.is_empty());

    // Phase 4 preview invokes Layout Intelligence check at Step 8
    let store_dir = make_temp_layout_dir("c09_store");
    let store = TemplateStore::with_root(&store_dir);
    let org = OrganizationProfile {
        id: "org-test".to_string(),
        profile_name: "Test Org".to_string(),
        governing_org: "SỞ X".to_string(),
        issuing_org: "TRUNG TÂM A".to_string(),
        agency_abbreviation: "TTA".to_string(),
        sector: OrgSector::StateAgency,
        default_location: "Hà Nội".to_string(),
        use_nd30_layout_for_enterprise: true,
        ..OrganizationProfile::default()
    };
    let mut fields = DynamicTemplateFields::default();
    fields.trich_yeu = "Triển khai thử nghiệm".to_string();
    fields.so_van_ban = "01/TB-TTA".to_string();
    let preview = template_engine::preview_template(
        &store,
        &TemplateApplyRequest {
            template_id: "adm-thong-bao".to_string(),
            target_mode: TemplateTargetMode::CreateNewDocx,
            target_path: Some(dir.join("out_tpl.docx").to_string_lossy().to_string()),
            output_mode: OutputMode::NewCopy,
            org_profile_override: Some(org),
            dynamic_fields: fields,
            ..TemplateApplyRequest::default()
        },
    )
    .unwrap();
    assert!(preview.compatibility.compatible);

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&store_dir);
}

#[test]
fn layout_case_10_complex_master_docx_and_real_microsoft_word_com() {
    let dir = make_temp_layout_dir("c10_master");
    let path = dir.join("complex_master.docx");

    let hdr1 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>TÀI LIỆU KIỂM CHỨNG PHASE 5</w:t></w:r></w:p></w:hdr>"#;
    let ftr1 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Trang </w:t></w:r><w:fldSimple w:instr=" PAGE "/></w:p></w:ftr>"#;
    let footnotes = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>
  <w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>
  <w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> Chú thích chuẩn tài liệu.</w:t></w:r></w:p></w:footnote>
</w:footnotes>"#;

    let img_ok = inline_image_paragraph_xml("rId3", "LogoNho.png", 1_080_000, 540_000);
    let tb_xml = word_textbox_paragraph_xml(
        88,
        "Hộp ghi chú trong tài liệu phức tạp",
        "Bảo toàn nguyên trạng OpenXML trong Word COM",
    );

    let body = format!(
        r#"
<w:tbl>
  <w:tblPr><w:tblW w:w="9355" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="5355"/></w:tblGrid>
  <w:tr><w:tc><w:p><w:r><w:t>BỘ NỘI VỤ</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p><w:p><w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>ĐỀ ÁN</w:t></w:r></w:p>
<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Nâng cấp hạ tầng số toàn diện</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:ind w:firstLine="720"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>CHƯƠNG I: HIỆN TRẠNG</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>1.1. Đánh giá hạ tầng</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading3"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>1.1.1. Mạng trục chính</w:t></w:r></w:p>
<w:p><w:pPr><w:spacing w:before="480" w:after="480"/></w:pPr><w:bookmarkStart w:id="0" w:name="BMK_INTRO"/><w:r><w:t xml:space="preserve">Tham khảo </w:t></w:r><w:hyperlink r:id="rId7"><w:r><w:t>Cổng thông tin Chính phủ</w:t></w:r></w:hyperlink><w:r><w:footnoteReference w:id="1"/></w:r><w:bookmarkEnd w:id="0"/></w:p>
{img_ok}
{tb_xml}
<w:p>
  <w:pPr>
    <w:sectPr>
      <w:headerReference w:type="default" r:id="rId4"/>
      <w:footerReference w:type="default" r:id="rId5"/>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/>
      <w:cols w:num="1" w:space="720"/>
    </w:sectPr>
  </w:pPr>
</w:p>
<w:p><w:r><w:t>Nội dung Section 2 trình bày theo 2 cột báo cáo phân tích.</w:t></w:r></w:p>
<w:p>
  <w:pPr>
    <w:sectPr>
      <w:type w:val="nextPage"/>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/>
      <w:cols w:num="2" w:space="720"/>
    </w:sectPr>
  </w:pPr>
</w:p>
<w:tbl>
  <w:tblPr><w:tblW w:w="13000" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="6000"/><w:gridCol w:w="7000"/></w:tblGrid>
  <w:tr><w:tc><w:p><w:r><w:t>Nơi nhận:</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>BỘ TRƯỞNG</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>PHỤ LỤC I: BẢNG SỐ LIỆU CHI TIẾT</w:t></w:r></w:p>
<w:tbl>
  <w:tblPr><w:tblW w:w="12200" w:type="dxa"/><w:tblInd w:w="200" w:type="dxa"/></w:tblPr>
  <w:tblGrid><w:gridCol w:w="3050"/><w:gridCol w:w="3050"/><w:gridCol w:w="3050"/><w:gridCol w:w="3050"/></w:tblGrid>
  <w:tr><w:tc><w:p><w:r><w:t>C1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>C2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>C3</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>C4</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:sectPr>
  <w:type w:val="nextPage"/>
  <w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/>
  <w:pgMar w:top="1701" w:right="850" w:bottom="1134" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/>
  <w:cols w:num="1" w:space="720"/>
</w:sectPr>
"#,
        img_ok = img_ok,
        tb_xml = tb_xml
    );

    build_layout_fixture_docx(
        &path,
        &body,
        None,
        vec![
            ("word/media/image1.png", minimal_png_bytes()),
            ("word/header1.xml", hdr1.as_bytes().to_vec()),
            ("word/footer1.xml", ftr1.as_bytes().to_vec()),
            ("word/footnotes.xml", footnotes.as_bytes().to_vec()),
        ],
        r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
<Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
<Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://chinhphu.vn" TargetMode="External"/>"#,
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
  <Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>"#,
        true,
    );

    let rep = layout_intelligence::analyze_file(&path, StdDocScope::AdminNd30, None).unwrap();
    assert_eq!(rep.summary.section_count, 3);
    assert_eq!(rep.summary.table_count, 3);
    assert_eq!(rep.summary.image_count, 1);
    assert_eq!(rep.summary.text_box_count, 1);
    assert!(rep.summary.heading_count >= 3);

    // Execute both HighConfidenceAuto + accepted confirmation proposals
    let all_confirm: Vec<String> = rep
        .proposals
        .iter()
        .filter(|p| p.tier == LayoutProposalTier::RequiresConfirmation)
        .map(|p| p.proposal_id.clone())
        .collect();
    let res = layout_intelligence::execute_proposals(
        &LayoutExecutionRequest {
            path: path.to_string_lossy().to_string(),
            scope: Some(StdDocScope::AdminNd30),
            output_mode: OutputMode::NewCopy,
            apply_high_confidence_auto: true,
            accepted_proposal_ids: all_confirm,
            rejected_proposal_ids: vec![],
        },
        None,
    );
    assert!(res.success);
    assert!(res.content_preserved_verified);
    assert!(res.relationships_preserved_verified);

    let out_path = PathBuf::from(&res.output_path);
    // Double-check output XML parses cleanly and keeps every non-empty paragraph
    let out_pkg = OpenXmlPackage::open(&out_path).unwrap();
    let out_doc = XmlDocument::parse(&out_pkg.get_xml("word/document.xml").unwrap()).unwrap();
    assert!(!layout_intelligence::smart_formatter::collect_all_non_empty_paragraph_texts(&out_doc.root).is_empty());

    // Open BOTH original and processed master files in real Microsoft Word COM!
    verify_docs_in_real_word_com(&[
        (&path, "Nâng cấp hạ tầng số toàn diện", "Hộp ghi chú trong tài liệu phức tạp"),
        (&out_path, "Nâng cấp hạ tầng số toàn diện", "Hộp ghi chú trong tài liệu phức tạp"),
    ]);

    let _ = fs::remove_dir_all(&dir);
}
