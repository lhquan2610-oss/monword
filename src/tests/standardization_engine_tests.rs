use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::std_engine::{analyze_file, execute_standardization, get_std_rules_catalog};
use crate::engine;
use crate::models::{
    DiffDimension, FixSafetyTier, OutputMode, RuleCategory, StdDocScope, StdExecutionRequest,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::CompressionMethod;

fn make_temp_std_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "monword_std_test_{}_{}",
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

fn verify_open_in_real_word(paths: &[&Path], check_text: Option<&str>) {
    let list_ps = paths
        .iter()
        .map(|p| format!("'{}'", p.to_string_lossy().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    let text_check = match check_text {
        Some(t) => format!(
            "if (-not $d.Content.Text.Contains('{}')) {{ throw \"Thieu noi dung ky vong trong $($p)\" }}",
            t.replace('\'', "''")
        ),
        None => String::new(),
    };
    let ps = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $word.AutomationSecurity = 3
  foreach ($p in @({})) {{
    $d = $word.Documents.Open($p, $false, $true)
    try {{
      {}
    }} finally {{
      $d.Close([ref]$false)
    }}
  }}
}} finally {{
  if ($word -ne $null) {{
    $word.Quit([ref]$false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
  }}
}}
"#,
        list_ps, text_check
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
    assert!(
        out.status.success(),
        "Microsoft Word 16.0 báo lỗi khi mở tài liệu: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[allow(clippy::too_many_arguments)]
fn build_std_docx(
    path: &Path,
    body_inner_xml: &str,
    extra_entries: Vec<(&str, Vec<u8>)>,
    extra_rels: &str,
    extra_ct: &str,
    styles_override: Option<&str>,
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
        r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#
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

    let default_styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
</w:styles>"#;
    pkg.set_bytes(
        "word/styles.xml",
        styles_override.unwrap_or(default_styles).as_bytes().to_vec(),
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

fn minimal_png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn tnr_run(text: &str, sz_hp: &str, extra_rpr: &str) -> String {
    format!(
        r#"<w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/>{}<w:color w:val="000000"/><w:sz w:val="{}"/><w:szCs w:val="{}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        extra_rpr, sz_hp, sz_hp, text
    )
}

#[test]
fn std_case_01_valid_nd30_docx_no_false_positives() {
    let dir = make_temp_std_dir("c01_valid");
    let src = dir.join("valid_nd30.docx");
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
          </w:tc>
        </w:tr>
      </w:tbl>
      <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:before="120" w:after="60"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:tbl>
        <w:tr>
          <w:tc><w:p>{}</w:p><w:p>{}</w:p></w:tc>
          <w:tc><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
    "#,
        tnr_run("SỞ TÀI NGUYÊN VÀ MÔI TRƯỜNG", "26", "<w:b/>"),
        tnr_run("Số: 05/QĐ-UBND", "26", ""),
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Độc lập - Tự do - Hạnh phúc", "28", "<w:b/>"),
        tnr_run("QUYẾT ĐỊNH", "28", "<w:b/>"),
        tnr_run("Căn cứ Luật Tổ chức chính quyền địa phương năm 2015;", "28", "<w:i/>"),
        tnr_run("Ủy ban nhân dân ban hành Quy định về chuẩn hóa văn bản hành chính trên địa bàn.", "28", ""),
        tnr_run("Nơi nhận:", "24", "<w:b/><w:i/>"),
        tnr_run("- Lưu VT.", "22", ""),
        tnr_run("TM. ỦY BAN NHÂN DÂN", "28", "<w:b/>"),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    // Check Rule Catalog carries all 11 metadata fields including checkFunction / fixFunction
    let catalog = get_std_rules_catalog();
    assert!(catalog.len() >= 24);
    assert!(catalog.iter().all(|r| !r.check_function.is_empty() && !r.legal_basis.is_empty()));

    let (rep, _) = analyze_file(&src, StdDocScope::AutoDetect, None).unwrap();
    assert_eq!(rep.effective_scope, StdDocScope::AdminNd30);
    assert_eq!(rep.severity_counts.critical, 0, "Findings: {:?}", rep.findings);
    assert_eq!(rep.severity_counts.error, 0, "Findings: {:?}", rep.findings);
    assert_eq!(rep.severity_counts.warning, 0, "Findings: {:?}", rep.findings);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_02_wrong_font_and_mixed_run_fonts() {
    let dir = make_temp_std_dir("c02_wrong_font");
    let src = dir.join("wrong_font.docx");
    let body = r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/><w:b/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>
        <w:r><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/><w:sz w:val="28"/></w:rPr><w:t>•</w:t></w:r>
        <w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="28"/></w:rPr><w:t xml:space="preserve"> Đoạn văn dùng Calibri </w:t></w:r>
        <w:r><w:rPr><w:rFonts w:ascii="Tahoma" w:hAnsi="Tahoma"/><w:sz w:val="28"/></w:rPr><w:t>trộn Tahoma cần đồng bộ.</w:t></w:r>
      </w:p>
    "#;
    build_std_docx(&src, body, vec![], "", "", None, false, false);

    let req = StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    };
    let res = execute_standardization(&req);
    assert!(res.success, "Err: {:?}", res.error);
    assert!(res.verification.as_ref().unwrap().preservation_passed);
    let rep_after = res.report_after.as_ref().unwrap();
    assert!(!rep_after.findings.iter().any(|f| f.rule_id == "FONT-02"));

    let out_xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    // Text converted to Times New Roman while Symbol font preserved
    assert!(out_xml.contains(r#"w:ascii="Times New Roman""#));
    assert!(out_xml.contains(r#"w:ascii="Symbol""#));
    assert!(!out_xml.contains(r#"w:ascii="Calibri""#));
    assert!(!out_xml.contains(r#"w:ascii="Tahoma""#));
    assert!(res
        .diff
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .any(|d| d.dimension == DiffDimension::FontFamily));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_03_wrong_component_specific_font_sizes() {
    let dir = make_temp_std_dir("c03_sizes");
    let src = dir.join("wrong_sizes.docx");
    let body = format!(
        r#"
      <w:tbl><w:tr>
        <w:tc><w:p>{}</w:p></w:tc>
        <w:tc><w:p>{}</w:p><w:p>{}</w:p></w:tc>
      </w:tr></w:tbl>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p>{}</w:p>
      <w:p>{}</w:p>
    "#,
        tnr_run("CƠ QUAN BAN HÀNH", "32", "<w:b/>"), // 16pt header cell -> should become 13pt
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Độc lập - Tự do - Hạnh phúc", "28", "<w:b/>"),
        tnr_run("Nội dung chính đang để sai cỡ chữ 11pt.", "22", ""), // 11pt body -> 14pt
        tnr_run("Nơi nhận:", "24", "<w:b/><w:i/>"),
        tnr_run("- Lưu VT, Phòng TC.", "30", ""), // 15pt Nơi nhận -> 11pt (NOT 14pt!)
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    let req = StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    };
    let res = execute_standardization(&req);
    assert!(res.success, "Err: {:?}", res.error);

    let out_xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    // Header fixed to 26 (13pt), Body fixed to 28 (14pt), NoiNhan item fixed to 22 (11pt)
    let p_hdr = out_xml.find("CƠ QUAN BAN HÀNH").unwrap();
    assert!(out_xml[p_hdr.saturating_sub(250)..p_hdr].contains(r#"w:val="26""#));
    let p_body = out_xml.find("Nội dung chính đang để sai").unwrap();
    assert!(out_xml[p_body.saturating_sub(250)..p_body].contains(r#"w:val="28""#));
    let p_nn = out_xml.find("- Lưu VT").unwrap();
    assert!(out_xml[p_nn.saturating_sub(250)..p_nn].contains(r#"w:val="22""#));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_04_wrong_alignment_body_vs_protected_blocks() {
    let dir = make_temp_std_dir("c04_align");
    let src = dir.join("centered_body.docx");
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="center"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="center"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="center"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("QUYẾT ĐỊNH", "28", "<w:b/>"),
        tnr_run("Đoạn lời văn số 1 đang bị căn giữa sai quy chuẩn thể thức hành chính.", "28", ""),
        tnr_run("Đoạn lời văn số 2 cũng đang bị căn giữa thay vì căn đều hai bên.", "28", ""),
        tnr_run("Đoạn lời văn số 3 tiếp tục bị căn giữa toàn bộ tài liệu.", "28", ""),
        tnr_run("TM. GIÁM ĐỐC", "28", "<w:b/>"),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    let (rep_b, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    let para1_findings: Vec<_> = rep_b.findings.iter().filter(|f| f.rule_id == "PARA-01").collect();
    assert_eq!(para1_findings.len(), 3);
    assert!(para1_findings[0].current_value.contains("căn giữa toàn bộ"));

    let req = StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    };
    let res = execute_standardization(&req);
    assert!(res.success);
    assert!(!res.report_after.unwrap().findings.iter().any(|f| f.rule_id == "PARA-01"));

    let xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    // QUYẾT ĐỊNH and TM. GIÁM ĐỐC remain centered; body prose becomes both
    let para_slice = |needle: &str| {
        let p = xml.find(needle).unwrap();
        let st = xml[..p].rfind("<w:p>").unwrap();
        &xml[st..p]
    };
    assert!(para_slice("QUYẾT ĐỊNH").contains(r#"w:val="center""#));
    assert!(para_slice("TM. GIÁM ĐỐC").contains(r#"w:val="center""#));
    assert!(para_slice("Đoạn lời văn số 1").contains(r#"w:val="both""#));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_05_wrong_paragraph_and_line_spacing() {
    let dir = make_temp_std_dir("c05_spacing");
    let src = dir.join("bad_spacing.docx");
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="auto"/></w:pPr>{}<w:r><w:br/><w:t>Dòng tách ngắt mềm</w:t></w:r></w:p>
      <w:p/>
      <w:p/>
      <w:p/>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="500" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Đoạn văn giãn dòng 2.0 quá lớn.", "28", ""),
        tnr_run("Đoạn văn cách trước quá lớn.", "28", ""),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    let (rep_b, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    assert!(rep_b.findings.iter().any(|f| f.rule_id == "PARA-03"));
    assert!(rep_b.findings.iter().any(|f| f.rule_id == "PARA-04"));

    let req = StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    };
    let res = execute_standardization(&req);
    assert!(res.success, "Err: {:?}", res.error);
    let rep_a = res.report_after.unwrap();
    assert!(!rep_a.findings.iter().any(|f| f.rule_id == "PARA-03"));
    assert!(!rep_a.findings.iter().any(|f| f.rule_id == "PARA-04"));

    let xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml.contains("Dòng tách ngắt mềm"));
    assert!(xml.contains("Đoạn văn cách trước quá lớn."));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_06_wrong_page_margins_25mm_to_30mm() {
    let dir = make_temp_std_dir("c06_margins");
    let src = dir.join("wrong_margins.docx");
    // Left margin = 1417 twips (~25.0 mm), outside NĐ30 30–35mm
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1417"/></w:sectPr>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Văn bản đang đặt lề trái 25 mm.", "28", ""),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, true);

    let (rep_b, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    let p2 = rep_b.findings.iter().find(|f| f.rule_id == "PAGE-02").unwrap();
    assert!(p2.current_value.contains("25.0 mm"));

    let req = StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    };
    let res = execute_standardization(&req);
    assert!(res.success);
    assert!(res.verification.as_ref().unwrap().reopened_from_disk);
    assert_eq!(res.report_after.unwrap().sections[0].margin_left_mm, 30.0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_07_multi_section_preserved() {
    let dir = make_temp_std_dir("c07_multisect");
    let src = dir.join("multi_section.docx");
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/></w:rPr><w:t>Section 1 content.</w:t></w:r></w:p>
      <w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701"/></w:sectPr></w:pPr></w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701"/></w:sectPr></w:pPr></w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701"/></w:sectPr>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Section 2 content.", "28", ""),
        tnr_run("Section 3 content.", "28", ""),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, true);

    let req = StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    };
    let res = execute_standardization(&req);
    assert!(res.success, "Err: {:?}", res.error);
    assert_eq!(res.report_after.as_ref().unwrap().sections.len(), 3);
    assert!(res.verification.as_ref().unwrap().preservation_passed);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_08_landscape_section_preserved() {
    let dir = make_temp_std_dir("c08_landscape");
    let src = dir.join("landscape_sect.docx");
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701"/></w:sectPr></w:pPr></w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/></w:rPr><w:t>Bảng phụ lục khổ ngang.</w:t></w:r></w:p>
      <w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="1701" w:right="1134" w:bottom="1134" w:left="1134"/></w:sectPr>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, true);

    let req = StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    };
    let res = execute_standardization(&req);
    assert!(res.success, "Err: {:?}", res.error);
    let sects = &res.report_after.as_ref().unwrap().sections;
    assert_eq!(sects.len(), 2);
    assert_eq!(sects[0].orientation, "portrait");
    assert_eq!(sects[1].orientation, "landscape");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_09_wide_table_exceeding_printable_width() {
    let dir = make_temp_std_dir("c09_wide_table");
    let src = dir.join("wide_tbl.docx");
    // Printable width = 11906 - 1701 - 1134 = 9071 twips; table = 12000 twips
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:tbl>
        <w:tblPr><w:tblW w:w="12000" w:type="dxa"/></w:tblPr>
        <w:tblGrid><w:gridCol w:w="6000"/><w:gridCol w:w="6000"/></w:tblGrid>
        <w:tr>
          <w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:p>{}</w:p></w:tc>
          <w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:p>{}</w:p></w:tc>
        </w:tr>
      </w:tbl>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Cột quá rộng 1", "24", ""),
        tnr_run("Cột quá rộng 2", "24", ""),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    let (rep, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    let tbl_f = rep.findings.iter().find(|f| f.rule_id == "TBL-01").unwrap();
    assert_eq!(tbl_f.safety_tier, FixSafetyTier::GroupBConfirm);

    // Group A alone must NOT shrink Group B table OR rotate page
    let res_a = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res_a.success);
    assert_eq!(res_a.report_after.as_ref().unwrap().tables[0].total_width_twips, 12000);
    assert_eq!(res_a.report_after.as_ref().unwrap().sections[0].orientation, "portrait");

    // Approving Group B finding scales table width cleanly without rotating orientation
    let res_b = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: false,
        approved_finding_ids: vec![tbl_f.finding_id.clone()],
        ..Default::default()
    });
    assert!(res_b.success);
    let t_after = &res_b.report_after.as_ref().unwrap().tables[0];
    assert_eq!(t_after.total_width_twips, t_after.printable_width_twips);
    assert_eq!(res_b.report_after.as_ref().unwrap().sections[0].orientation, "portrait");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_10_multi_page_table_header_and_row_split() {
    let dir = make_temp_std_dir("c10_tbl_pag");
    let src = dir.join("multipage_tbl.docx");
    let long_txt = "Chi tiết triển khai dự án giai đoạn 2026–2030 cần kéo dài qua nhiều dòng và trang khác nhau. ".repeat(3);
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:tbl>
        <w:tblPr><w:tblW w:w="9071" w:type="dxa"/></w:tblPr>
        <w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="7071"/></w:tblGrid>
        <w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>
        <w:tr><w:trPr><w:trHeight w:val="2800"/><w:cantSplit/></w:trPr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>
        <w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>
      </w:tbl>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("STT", "24", "<w:b/>"),
        tnr_run("Nội dung", "24", "<w:b/>"),
        tnr_run("1", "24", ""),
        tnr_run(&long_txt, "24", ""),
        tnr_run("2", "24", ""),
        tnr_run("Hạng mục tiếp theo", "24", ""),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    let (rep, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    let f = rep.findings.iter().find(|f| f.rule_id == "TBL-02").unwrap();

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: false,
        approved_finding_ids: vec![f.finding_id.clone()],
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let t_after = &res.report_after.unwrap().tables[0];
    assert!(t_after.has_repeat_header);
    assert!(!t_after.has_locked_long_rows);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_11_complex_header_footer_and_page_numpages_fields() {
    let dir = make_temp_std_dir("c11_hf_fields");
    let src = dir.join("hf_fields.docx");
    let hdr_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
</w:hdr>"#;
    let ftr_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:t xml:space="preserve">Tổng số trang: </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
</w:ftr>"#;
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/></w:rPr><w:t>Nội dung tài liệu có Header PAGE và Footer NUMPAGES.</w:t></w:r></w:p>
      <w:sectPr><w:headerReference w:type="default" r:id="rId10"/><w:footerReference w:type="default" r:id="rId11"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701"/></w:sectPr>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
    );
    build_std_docx(
        &src,
        &body,
        vec![
            ("word/header1.xml", hdr_xml.as_bytes().to_vec()),
            ("word/footer1.xml", ftr_xml.as_bytes().to_vec()),
        ],
        r#"<Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rId11" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>"#,
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#,
        None,
        false,
        true,
    );

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let pkg_a = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let h_after = pkg_a.get_xml("word/header1.xml").unwrap();
    let f_after = pkg_a.get_xml("word/footer1.xml").unwrap();
    assert!(h_after.contains("PAGE"));
    assert!(f_after.contains("NUMPAGES"));
    assert!(f_after.contains("Tổng số trang:"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_12_logos_and_embedded_images_preserved() {
    let dir = make_temp_std_dir("c12_media");
    let src = dir.join("logos_imgs.docx");
    let png1 = minimal_png_bytes();
    let mut png2 = minimal_png_bytes();
    png2.push(0x99);
    let body = r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="26"/><w:b/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rId20"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
      <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rId21"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/></w:rPr><w:t xml:space="preserve">Đoạn kèm hình minh họa.</w:t></w:r></w:p>
    "#;
    build_std_docx(
        &src,
        body,
        vec![
            ("word/media/logo.png", png1.clone()),
            ("word/media/chart.png", png2.clone()),
        ],
        r#"<Relationship Id="rId20" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/logo.png"/><Relationship Id="rId21" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/chart.png"/>"#,
        "",
        None,
        false,
        false,
    );

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let pres = &res.diff.as_ref().unwrap().preservation;
    assert_eq!(pres.media_before, 2);
    assert_eq!(pres.media_after, 2);
    assert_eq!(pres.drawings_before, 2);
    assert_eq!(pres.drawings_after, 2);

    let pkg_a = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    assert_eq!(pkg_a.get_bytes("word/media/logo.png").unwrap(), png1.as_slice());
    assert_eq!(pkg_a.get_bytes("word/media/chart.png").unwrap(), png2.as_slice());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_13_signature_block_and_group_c_untouched() {
    let dir = make_temp_std_dir("c13_group_c");
    let src = dir.join("sig_groupc.docx");
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/></w:rPr><w:t>Nội dung điều khoản văn bản.</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p/>
      <w:p/>
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("Số: 12/QD-UBND", "26", ""),
        tnr_run("kt. giám đốc", "28", "<w:b/>"),
        tnr_run("Nguyễn Văn Minh", "28", "<w:b/>"),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    let (rep, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    let abbr = rep.findings.iter().find(|f| f.rule_id == "ABBR-01").unwrap();
    let cap = rep.findings.iter().find(|f| f.rule_id == "CAP-01").unwrap();
    assert_eq!(abbr.safety_tier, FixSafetyTier::GroupCManualOnly);
    assert_eq!(cap.safety_tier, FixSafetyTier::GroupCManualOnly);

    // Even if someone passes Group C finding IDs in approved_finding_ids, engine NEVER modifies them
    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        approved_finding_ids: vec![abbr.finding_id.clone(), cap.finding_id.clone()],
        ..Default::default()
    });
    assert!(res.success);
    let out_xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(out_xml.contains("Số: 12/QD-UBND"));
    assert!(out_xml.contains("kt. giám đốc"));
    assert!(out_xml.contains("Nguyễn Văn Minh"));
    assert!(!res.actions_applied.iter().any(|a| a.rule_id == "ABBR-01" || a.rule_id == "CAP-01"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_14_hyperlinks_and_bookmarks_preserved() {
    let dir = make_temp_std_dir("c14_links_bm");
    let src = dir.join("links_bm.docx");
    let body = r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="26"/><w:b/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p>
        <w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>
        <w:bookmarkStart w:id="1" w:name="Muc_1"/>
        <w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:color w:val="FF0000"/><w:sz w:val="28"/></w:rPr><w:t xml:space="preserve">Tra cứu chi tiết tại </w:t></w:r>
        <w:hyperlink r:id="rId30"><w:r><w:rPr><w:color w:val="0563C1"/><w:sz w:val="28"/></w:rPr><w:t>chinhphu.vn</w:t></w:r></w:hyperlink>
        <w:bookmarkEnd w:id="1"/>
      </w:p>
    "#;
    build_std_docx(
        &src,
        body,
        vec![],
        r#"<Relationship Id="rId30" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://chinhphu.vn" TargetMode="External"/>"#,
        "",
        None,
        false,
        false,
    );

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let pres = &res.diff.as_ref().unwrap().preservation;
    assert_eq!(pres.hyperlinks_before, 1);
    assert_eq!(pres.hyperlinks_after, 1);
    assert_eq!(pres.bookmarks_before, 1);
    assert_eq!(pres.bookmarks_after, 1);

    let pkg_a = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let doc_xml = pkg_a.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains(r#"<w:bookmarkStart w:id="1" w:name="Muc_1""#));
    assert!(doc_xml.contains(r#"<w:bookmarkEnd w:id="1""#));
    assert!(doc_xml.contains(r#"<w:hyperlink r:id="rId30""#));
    assert!(doc_xml.contains(r#"w:val="0563C1""#)); // Hyperlink color preserved while FF0000 body turned 000000
    assert!(!doc_xml.contains(r#"w:val="FF0000""#));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_15_footnotes_endnotes_and_comments_preserved() {
    let dir = make_temp_std_dir("c15_notes_comments");
    let src = dir.join("notes_comments.docx");
    let fn_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:type="separator" w:id="-1"/><w:footnote w:id="1"><w:p><w:r><w:t>Chú thích chân trang 1.</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
    let en_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:endnote w:type="separator" w:id="-1"/><w:endnote w:id="1"><w:p><w:r><w:t>Chú thích cuối văn bản 1.</w:t></w:r></w:p></w:endnote></w:endnotes>"#;
    let cmt_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:comment w:id="1" w:author="Thanh tra"><w:p><w:r><w:t>Ghi chú rà soát</w:t></w:r></w:p></w:comment></w:comments>"#;

    let body = r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="26"/><w:b/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p>
        <w:pPr><w:jc w:val="left"/></w:pPr>
        <w:commentRangeStart w:id="1"/>
        <w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/></w:rPr><w:t>Đoạn văn bản có chú thích và bình luận</w:t></w:r>
        <w:r><w:footnoteReference w:id="1"/></w:r>
        <w:r><w:endnoteReference w:id="1"/></w:r>
        <w:commentRangeEnd w:id="1"/>
        <w:r><w:commentReference w:id="1"/></w:r>
      </w:p>
    "#;
    build_std_docx(
        &src,
        body,
        vec![
            ("word/footnotes.xml", fn_xml.as_bytes().to_vec()),
            ("word/endnotes.xml", en_xml.as_bytes().to_vec()),
            ("word/comments.xml", cmt_xml.as_bytes().to_vec()),
        ],
        r#"<Relationship Id="rId40" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="rId41" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/><Relationship Id="rId42" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>"#,
        "",
        None,
        false,
        false,
    );

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let pres = &res.diff.as_ref().unwrap().preservation;
    assert_eq!(pres.footnotes_before, 1);
    assert_eq!(pres.footnotes_after, 1);
    assert_eq!(pres.endnotes_before, 1);
    assert_eq!(pres.endnotes_after, 1);
    assert_eq!(pres.comments_before, 1);
    assert_eq!(pres.comments_after, 1);

    let pkg_a = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    let doc_xml = pkg_a.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains(r#"<w:footnoteReference w:id="1""#));
    assert!(doc_xml.contains(r#"<w:endnoteReference w:id="1""#));
    assert!(doc_xml.contains(r#"<w:commentReference w:id="1""#));
    assert!(pkg_a.has_entry("word/footnotes.xml"));
    assert!(pkg_a.has_entry("word/endnotes.xml"));
    assert!(pkg_a.has_entry("word/comments.xml"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_16_multi_level_headings_and_clause_numbers_preserved() {
    let dir = make_temp_std_dir("c16_headings");
    let src = dir.join("headings.docx");
    let body = format!(
        r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr>{}</w:p>
    "#,
        tnr_run("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM", "26", "<w:b/>"),
        tnr_run("PHẦN I QUY ĐỊNH CHUNG", "28", ""),
        tnr_run("MỤC 1 NGUYÊN TẮC", "28", ""),
        tnr_run("Điều 1. Phạm vi áp dụng giữ nguyên thứ tự.", "28", ""),
        tnr_run("Điều 2. Đối tượng thi hành không đổi số.", "28", ""),
    );
    build_std_docx(&src, &body, vec![], "", "", None, false, false);

    let (rep, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    let approves: Vec<String> = rep
        .findings
        .iter()
        .filter(|f| f.rule_id == "STRUCT-02" || f.rule_id == "STRUCT-03")
        .map(|f| f.finding_id.clone())
        .collect();
    assert!(!approves.is_empty());

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        approved_finding_ids: approves,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let doc_xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(doc_xml.contains(r#"w:val="Heading1""#));
    assert!(doc_xml.contains(r#"w:val="Heading2""#));
    let p_d1 = doc_xml.find("Điều 1. Phạm vi áp dụng").unwrap();
    let p_d2 = doc_xml.find("Điều 2. Đối tượng thi hành").unwrap();
    assert!(p_d1 < p_d2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_17_legacy_vietnamese_encoding_tcvn3_vni_with_word_com() {
    let dir = make_temp_std_dir("c17_tcvn3_vni_word");
    let src = dir.join("legacy_encoding.docx");
    let body = r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii=".VnTime" w:hAnsi=".VnTime"/><w:sz w:val="26"/><w:b/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii=".VnTime" w:hAnsi=".VnTime"/><w:sz w:val="28"/></w:rPr><w:t>Céng hßa x· héi chñ nghÜa ViÖt Nam.</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="VNI-Times" w:hAnsi="VNI-Times"/><w:sz w:val="28"/></w:rPr><w:t>Ñoäc laäp töï do haïnh phuùc.</w:t></w:r></w:p>
    "#;
    build_std_docx(&src, body, vec![], "", "", None, false, false);

    let (rep, _) = analyze_file(&src, StdDocScope::AdminNd30, None).unwrap();
    assert_eq!(rep.rule_summaries.iter().find(|r| r.rule_id == "FONT-01").unwrap().status, crate::models::RuleCheckStatus::Failed);
    assert!(rep.severity_counts.critical >= 1);

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let diff = res.diff.as_ref().unwrap();
    assert!(diff.entries.iter().any(|e| e.dimension == DiffDimension::TextContent));
    assert!(diff.entries.iter().any(|e| e.dimension == DiffDimension::FontFamily));

    let out_path = Path::new(&res.output_path);
    verify_open_in_real_word(&[out_path], Some("Độc lập tự do hạnh phúc"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_18_docm_vba_macros_preserved_with_word_com() {
    let dir = make_temp_std_dir("c18_docm_macro_word");

    // Part A: Byte-for-byte VBA macro entry preservation on .docm package
    let src_vba = dir.join("macro_doc_vba.docm");
    let vba_dummy = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x56, 0x42, 0x41, 0x33];
    let body = r#"
      <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="26"/><w:b/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr><w:t>Văn bản chứa Macro VBA cần bảo toàn nguyên vẹn.</w:t></w:r></w:p>
    "#;
    build_std_docx(
        &src_vba,
        body,
        vec![("word/vbaProject.bin", vba_dummy.clone())],
        "",
        "",
        None,
        true,
        false,
    );

    let res = execute_standardization(&StdExecutionRequest {
        path: src_vba.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    assert!(res.output_path.ends_with(".docm"));
    assert!(res.diff.as_ref().unwrap().preservation.macros_after);

    let pkg_a = OpenXmlPackage::open(Path::new(&res.output_path)).unwrap();
    assert_eq!(pkg_a.get_bytes("word/vbaProject.bin").unwrap(), vba_dummy.as_slice());

    // Part B: Real Microsoft Word 16.0 COM round-trip on genuine Word-saved .docm (WdFormat 13)
    let word_docm = dir.join("real_word_macro.docm");
    let create_docm_ps = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $word.AutomationSecurity = 3
  $d = $word.Documents.Add()
  $r = $d.Range(0, 0)
  $r.Text = "CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM`r`nVăn bản chứa Macro VBA cần bảo toàn nguyên vẹn."
  $r.Font.Name = "Arial"
  $p = '{}'
  $d.SaveAs([ref]$p, [ref]13)
  $d.Close([ref]$false)
}} finally {{
  if ($word -ne $null) {{
    $word.Quit([ref]$false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
  }}
}}
"#,
        word_docm.to_string_lossy().replace('\'', "''")
    );
    let out_c = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encode_ps_utf16_b64(&create_docm_ps),
        ])
        .output()
        .expect("Khởi chạy Word tạo .docm thất bại");
    assert!(out_c.status.success() && word_docm.exists(), "stderr={}", String::from_utf8_lossy(&out_c.stderr));

    let res_real = execute_standardization(&StdExecutionRequest {
        path: word_docm.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res_real.success, "Err: {:?}", res_real.error);
    assert!(res_real.output_path.ends_with(".docm"));
    verify_open_in_real_word(&[Path::new(&res_real.output_path)], Some("Văn bản chứa Macro VBA"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_19_large_docx_selective_low_ram_analysis_and_post_save_verify() {
    let dir = make_temp_std_dir("c19_large_lowram");
    let src = dir.join("large_1600_paras.docx");

    let mut body = String::from(
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="26"/><w:b/></w:rPr><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r></w:p>"#,
    );
    for i in 1..=1599 {
        body.push_str(&format!(
            r#"<w:p><w:pPr><w:jc w:val="both"/><w:ind w:firstLine="720"/><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="28"/></w:rPr><w:t>Đoạn dữ liệu lớn thứ {} giữ nguyên nội dung.</w:t></w:r></w:p>"#,
            i
        ));
    }

    // Attach 4 MB synthetic media payload to prove selective analysis skips unpacking binary media
    let huge_media = vec![0xABu8; 4 * 1024 * 1024];
    build_std_docx(
        &src,
        &body,
        vec![("word/media/huge.png", huge_media)],
        "",
        "",
        None,
        false,
        false,
    );
    let orig_bytes = fs::read(&src).unwrap();

    let view = OpenXmlPackage::open_selective_for_analysis(&src).unwrap();
    assert!(!view.xml_parts.contains_key("word/media/huge.png"));
    assert!(view.part_metas.iter().any(|m| m.name == "word/media/huge.png" && m.uncompressed_size >= 4_000_000));

    let res = execute_standardization(&StdExecutionRequest {
        path: src.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::OverwriteWithBackup,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res.success, "Err: {:?}", res.error);
    let ver = res.verification.as_ref().unwrap();
    assert!(ver.output_exists && ver.reopened_from_disk && ver.xml_valid && ver.preservation_passed);
    let pres = &res.diff.as_ref().unwrap().preservation;
    assert_eq!(pres.text_paragraphs_before, 1600);
    assert_eq!(pres.text_paragraphs_after, 1600);
    assert_eq!(pres.media_before, 1);
    assert_eq!(pres.media_after, 1);

    let bak = res.backup_path.as_ref().unwrap();
    engine::restore_backup(&src, Path::new(bak)).unwrap();
    assert_eq!(fs::read(&src).unwrap(), orig_bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn std_case_20_corrupted_and_ambiguous_docx_safe_handling() {
    let dir = make_temp_std_dir("c20_corrupt_ambiguous");

    // 20a. Corrupted non-ZIP & corrupted XML
    let bad_zip = dir.join("corrupted.docx");
    fs::write(&bad_zip, b"NOT_A_VALID_ZIP_HEADER_BYTES").unwrap();
    assert!(analyze_file(&bad_zip, StdDocScope::AdminNd30, None).is_err());

    let bad_xml_docx = dir.join("bad_xml.docx");
    let mut bad_pkg = OpenXmlPackage { entries: Vec::new() };
    bad_pkg.set_bytes(
        "word/document.xml",
        b"<w:document><w:body><w:p><w:broken_unclosed>".to_vec(),
        CompressionMethod::Deflated,
    );
    bad_pkg.save_to(&bad_xml_docx).unwrap();
    assert!(analyze_file(&bad_xml_docx, StdDocScope::AdminNd30, None).is_err());

    // 20b. Ambiguous document without administrative/legal/thesis/party cues
    let amb = dir.join("ambiguous.docx");
    let amb_body = r#"
      <w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr><w:t>Đây là ghi chú công việc nội bộ thông thường không rõ loại tài liệu.</w:t></w:r></w:p>
    "#;
    build_std_docx(&amb, amb_body, vec![], "", "", None, false, false);

    let (rep_auto, _) = analyze_file(&amb, StdDocScope::AutoDetect, None).unwrap();
    assert_eq!(rep_auto.effective_scope, StdDocScope::UnknownRequiresSelection);
    assert!(rep_auto.requires_scope_selection);
    assert!(rep_auto
        .rule_summaries
        .iter()
        .any(|r| r.category == RuleCategory::PageAndSection && r.status == crate::models::RuleCheckStatus::Indeterminate));

    // Must refuse blind modification under AutoDetect when scope cannot be determined
    let refused = execute_standardization(&StdExecutionRequest {
        path: amb.to_string_lossy().to_string(),
        scope: StdDocScope::AutoDetect,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(!refused.success);

    // Once user selects explicit scope Freeform, executes cleanly & opens in real Word COM
    let ok_res = execute_standardization(&StdExecutionRequest {
        path: amb.to_string_lossy().to_string(),
        scope: StdDocScope::Freeform,
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(ok_res.success, "Err: {:?}", ok_res.error);
    verify_open_in_real_word(&[Path::new(&ok_res.output_path)], Some("ghi chú công việc"));

    let _ = fs::remove_dir_all(&dir);
}
