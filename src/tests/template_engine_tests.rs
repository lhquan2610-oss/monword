use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::std_engine::ir::DocumentIr;
use crate::engine::template_engine::{self, TemplateStore};
use crate::models::{
    DocumentTemplate, DynamicTemplateFields, ImportDocxTemplateRequest, OrgSector,
    OrganizationProfile, SaveTemplateRequest, StdDocScope, TemplateBlockKind, TemplateGroup,
    TemplateSectionBlock, TemplateTargetMode, TemplateUsageStatus,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::CompressionMethod;

fn make_temp_tpl_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "monword_tpl_test_{}_{}",
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

fn valid_drawing_paragraph_xml(rid: &str, name: &str) -> String {
    format!(
        r#"<w:p>
  <w:pPr><w:jc w:val="center"/></w:pPr>
  <w:r>
    <w:drawing>
      <wp:inline distT="0" distB="0" distL="0" distR="0">
        <wp:extent cx="914400" cy="914400"/>
        <wp:effectExtent l="0" t="0" r="0" b="0"/>
        <wp:docPr id="1" name="{name}"/>
        <wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/></wp:cNvGraphicFramePr>
        <a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
            <pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
              <pic:nvPicPr><pic:cNvPr id="0" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr>
              <pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
              <pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>
            </pic:pic>
          </a:graphicData>
        </a:graphic>
      </wp:inline>
    </w:drawing>
  </w:r>
</w:p>"#,
        rid = rid,
        name = name
    )
}

#[allow(clippy::too_many_arguments)]
fn build_tpl_fixture_docx(
    path: &Path,
    body_inner_xml: &str,
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

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
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

fn verify_docs_in_real_word_com(paths: &[(&Path, &str)]) {
    let checks_ps = paths
        .iter()
        .map(|(p, expected_txt)| {
            format!(
                r#"$p = '{}'
$fname = [System.IO.Path]::GetFileName($p)
try {{
  $d = $word.Documents.Open($p, $false, $true)
  try {{
    if (-not $d.Content.Text.Contains('{}')) {{
      throw "Thieu chuoi '{}' trong $fname"
    }}
    Write-Output ("WORD_OK|" + $fname + "|sec=" + $d.Sections.Count + "|tbl=" + $d.Tables.Count + "|links=" + $d.Hyperlinks.Count + "|bmk=" + $d.Bookmarks.Count + "|fn=" + $d.Footnotes.Count)
  }} finally {{
    $d.Close([ref]$false)
  }}
}} catch {{
  throw "FAIL_FILE [$fname]: $($_.Exception.Message)"
}}"#,
                p.to_string_lossy().replace('\'', "''"),
                expected_txt.replace('\'', "''"),
                expected_txt.replace('\'', "''"),
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

    let stdout_str = String::from_utf8_lossy(&out.stdout);
    let stderr_str = String::from_utf8_lossy(&out.stderr);
    println!("Word COM verification output:\n{}", stdout_str);
    assert!(
        out.status.success(),
        "Microsoft Word COM bao loi khi mo DOCX: stdout={}, stderr={}",
        stdout_str,
        stderr_str
    );
}

fn sample_org_state() -> OrganizationProfile {
    OrganizationProfile {
        id: "org_state_1".to_string(),
        profile_name: "Sở Nội vụ Đắk Lắk".to_string(),
        sector: OrgSector::StateAgency,
        governing_org: "ỦY BAN NHÂN DÂN TỈNH ĐẮK LẮK".to_string(),
        issuing_org: "SỞ NỘI VỤ".to_string(),
        affiliated_unit: String::new(),
        department_or_branch: String::new(),
        agency_abbreviation: "SNV".to_string(),
        default_location: "Buôn Ma Thuột".to_string(),
        use_nd30_layout_for_enterprise: true,
        show_governing_org_default: true,
        show_affiliated_unit_default: false,
        show_department_default: false,
        updated_at: "19/11/2026".to_string(),
    }
}

fn sample_org_ent_corporate() -> OrganizationProfile {
    OrganizationProfile {
        id: "org_ent_2".to_string(),
        profile_name: "Công ty TNHH Giải pháp Số Alpha".to_string(),
        sector: OrgSector::Enterprise,
        governing_org: String::new(),
        issuing_org: "CÔNG TY TNHH GIẢI PHÁP SỐ ALPHA".to_string(),
        affiliated_unit: String::new(),
        department_or_branch: String::new(),
        agency_abbreviation: "ALPHA".to_string(),
        default_location: "TP. Hồ Chí Minh".to_string(),
        use_nd30_layout_for_enterprise: false,
        show_governing_org_default: false,
        show_affiliated_unit_default: false,
        show_department_default: false,
        updated_at: "19/11/2026".to_string(),
    }
}

fn sample_full_fields() -> DynamicTemplateFields {
    DynamicTemplateFields {
        ten_co_quan: String::new(),
        co_quan_chu_quan: String::new(),
        viet_tat_co_quan: String::new(),
        so_van_ban: "125".to_string(),
        dia_danh: String::new(),
        ngay_thang: String::new(),
        day: "5".to_string(),
        month: "2".to_string(),
        year: "2026".to_string(),
        nguoi_ky: "Nguyễn Văn An".to_string(),
        chuc_vu: "Giám đốc".to_string(),
        nguoi_nhan: "Các phòng, đơn vị trực thuộc".to_string(),
        trich_yeu: "Triển khai kế hoạch công tác năm 2026".to_string(),
        extra_fields: BTreeMap::new(),
    }
}

#[test]
fn tpl_case_01_library_inventory_distinct_structures_and_dropin_extensibility() {
    let dir = make_temp_tpl_dir("case01_lib");
    let store = TemplateStore::with_root(&dir);
    let catalog = store.list_catalog().unwrap();

    let admin: Vec<&DocumentTemplate> = catalog
        .templates
        .iter()
        .filter(|t| t.group == TemplateGroup::Administrative)
        .collect();
    let ent: Vec<&DocumentTemplate> = catalog
        .templates
        .iter()
        .filter(|t| t.group == TemplateGroup::Enterprise)
        .collect();

    assert_eq!(admin.len(), 12, "Phải có tối thiểu 12 mẫu hành chính");
    assert_eq!(ent.len(), 8, "Phải có tối thiểu 8 mẫu doanh nghiệp");

    let expected_admin_ids = [
        "adm-cong-van",
        "adm-to-trinh",
        "adm-bao-cao",
        "adm-quyet-dinh",
        "adm-thong-bao",
        "adm-bien-ban",
        "adm-ke-hoach",
        "adm-giay-moi",
        "adm-quy-che",
        "adm-huong-dan",
        "adm-phieu-de-xuat",
        "adm-chuong-trinh-cong-tac",
    ];
    for id in expected_admin_ids {
        assert!(
            admin.iter().any(|t| t.template_id == id),
            "Thiếu mẫu hành chính '{}'",
            id
        );
    }

    let expected_ent_ids = [
        "ent-to-trinh-noi-bo",
        "ent-bao-cao-noi-bo",
        "ent-quyet-dinh-noi-bo",
        "ent-thong-bao-noi-bo",
        "ent-bien-ban-hop",
        "ent-phieu-de-xuat",
        "ent-quy-trinh-nghiep-vu",
        "ent-van-ban-dieu-hanh",
    ];
    for id in expected_ent_ids {
        assert!(
            ent.iter().any(|t| t.template_id == id),
            "Thiếu mẫu doanh nghiệp '{}'",
            id
        );
    }

    // Distinct structures per type + all 13 fields present
    let cv = catalog
        .templates
        .iter()
        .find(|t| t.template_id == "adm-cong-van")
        .unwrap();
    let qd = catalog
        .templates
        .iter()
        .find(|t| t.template_id == "adm-quyet-dinh")
        .unwrap();
    let bb = catalog
        .templates
        .iter()
        .find(|t| t.template_id == "adm-bien-ban")
        .unwrap();
    assert!(cv.header_config.use_left_cell_subject);
    assert!(cv.header_config.symbol_prefix.is_empty());
    assert!(qd
        .content_structure
        .iter()
        .any(|s| s.kind == TemplateBlockKind::DecisionKeyword));
    assert_eq!(
        bb.signature_config.layout,
        crate::models::SignatureLayoutKind::DualSecretaryChair
    );
    assert!(!bb.header_config.show_location_date);

    // Drop-in external JSON template extensibility without changing engine code
    let dropin_path = store.dropin_dir().join("ngoai_bien_ban_ban_giao.json");
    let dropin_tpl = DocumentTemplate {
        template_id: "ext-bien-ban-ban-giao".to_string(),
        name: "Biên bản bàn giao tài sản (Drop-in JSON)".to_string(),
        group: TemplateGroup::Custom,
        doc_type: "bienBanBanGiao".to_string(),
        scope: StdDocScope::EnterpriseInternal,
        status: TemplateUsageStatus::Active,
        content_structure: vec![TemplateSectionBlock {
            section_id: "bg-1".to_string(),
            title: "1. Nội dung bàn giao".to_string(),
            kind: TemplateBlockKind::HeadingSection,
            required: true,
            enabled_by_default: true,
            body_paragraphs: vec!["Bàn giao hồ sơ thuộc {{trich_yeu}} tại {{ten_co_quan}}.".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };
    fs::write(
        &dropin_path,
        serde_json::to_string_pretty(&dropin_tpl).unwrap(),
    )
    .unwrap();

    let reloaded = store.list_catalog().unwrap();
    assert_eq!(reloaded.templates.len(), 21);
    assert!(reloaded
        .templates
        .iter()
        .any(|t| t.template_id == "ext-bien-ban-ban-giao"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_02_designer_crud_system_guard_default_and_version_restore() {
    let dir = make_temp_tpl_dir("case02_designer");
    let store = TemplateStore::with_root(&dir);

    // 1. Refuse overwriting system template without confirmation
    let mut sys_cv = store.get_template("adm-cong-van").unwrap();
    sys_cv.name = "Công văn sửa trái phép".to_string();
    let denied = store.save_template(SaveTemplateRequest {
        template: sys_cv.clone(),
        confirm_system_override: false,
        change_note: "Khong xac nhan".to_string(),
    });
    assert!(
        denied.is_err(),
        "Không được phép ghi đè mẫu hệ thống khi confirm_system_override = false"
    );

    // 2. Refuse deleting built-in system template
    let del_sys = store.delete_template("adm-cong-van");
    assert!(del_sys.is_err(), "Không được xóa mẫu hệ thống");

    // 3. Confirmed system override bumps version & preserves 1.0.0 -> restore 1.0.0 works
    store
        .save_template(SaveTemplateRequest {
            template: sys_cv.clone(),
            confirm_system_override: true,
            change_note: "Sửa có xác nhận".to_string(),
        })
        .unwrap();
    let updated_sys = store.get_template("adm-cong-van").unwrap();
    assert_eq!(updated_sys.version, "1.0.1");
    assert_eq!(updated_sys.name, "Công văn sửa trái phép");

    let (restored_sys, _) = store.restore_version("adm-cong-van", "1.0.0").unwrap();
    assert_eq!(restored_sys.version, "1.0.0");
    assert_eq!(
        restored_sys.name,
        "Công văn hành chính (NĐ 30/2020/NĐ-CP)"
    );

    // 4. Create new user template, edit to 1.0.1, clone, set default, restore 1.0.0, delete
    let user_tpl = DocumentTemplate {
        template_id: String::new(),
        name: "Mẫu đề xuất tùy biến A".to_string(),
        group: TemplateGroup::Custom,
        doc_type: "deXuatA".to_string(),
        scope: StdDocScope::AdminNd30,
        content_structure: vec![TemplateSectionBlock {
            section_id: "s1".to_string(),
            title: "Mục ban đầu".to_string(),
            kind: TemplateBlockKind::HeadingSection,
            required: true,
            enabled_by_default: true,
            body_paragraphs: vec!["Nội dung phiên bản đầu.".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let cat1 = store
        .save_template(SaveTemplateRequest {
            template: user_tpl,
            confirm_system_override: false,
            change_note: "Tạo mới".to_string(),
        })
        .unwrap();
    let created = cat1
        .templates
        .iter()
        .find(|t| t.name == "Mẫu đề xuất tùy biến A")
        .unwrap()
        .clone();
    assert_eq!(created.version, "1.0.0");
    assert!(!created.is_system);

    let mut edited = created.clone();
    edited.name = "Mẫu đề xuất tùy biến A v2".to_string();
    store
        .save_template(SaveTemplateRequest {
            template: edited,
            confirm_system_override: false,
            change_note: "Cập nhật tiêu đề".to_string(),
        })
        .unwrap();
    let after_edit = store.get_template(&created.template_id).unwrap();
    assert_eq!(after_edit.version, "1.0.1");

    let vers = store.list_versions(&created.template_id).unwrap();
    assert!(vers.iter().any(|v| v.version == "1.0.0"));
    assert!(vers.iter().any(|v| v.version == "1.0.1"));

    let (reverted, _) = store.restore_version(&created.template_id, "1.0.0").unwrap();
    assert_eq!(reverted.name, "Mẫu đề xuất tùy biến A");

    let (cloned, _) = store
        .clone_template("adm-quyet-dinh", Some("Quyết định riêng đơn vị".to_string()))
        .unwrap();
    assert!(!cloned.is_system);
    let cat_def = store.set_default_template(&cloned.template_id).unwrap();
    let cloned_ref = cat_def
        .templates
        .iter()
        .find(|t| t.template_id == cloned.template_id)
        .unwrap();
    assert!(cloned_ref.is_default_for_doc_type);

    store.delete_template(&created.template_id).unwrap();
    assert!(store.get_template(&created.template_id).is_err());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_03_import_and_export_docx_template() {
    let dir = make_temp_tpl_dir("case03_import_export");
    let store = TemplateStore::with_root(&dir);
    let master_src = dir.join("master_logo_table.docx");
    let png = minimal_png_bytes();

    let body = format!(
        r#"
      {}
      <w:p><w:r><w:t>Mã hồ sơ: {{{{so_van_ban}}}} — Dự án: {{{{custom_ma_du_an}}}}</w:t></w:r></w:p>
      <w:tbl>
        <w:tblPr><w:tblW w:w="9071" w:type="dxa"/></w:tblPr>
        <w:tblGrid><w:gridCol w:w="4500"/><w:gridCol w:w="4571"/></w:tblGrid>
        <w:tr>
          <w:tc><w:tcPr><w:tcW w:w="4500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Đơn vị</w:t></w:r></w:p></w:tc>
          <w:tc><w:tcPr><w:tcW w:w="4571" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{{{{ten_co_quan}}}}</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
    "#,
        valid_drawing_paragraph_xml("rId20", "LogoDonVi")
    );
    build_tpl_fixture_docx(
        &master_src,
        &body,
        vec![("word/media/logo.png", png.clone())],
        r#"<Relationship Id="rId20" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/logo.png"/>"#,
        "",
        false,
    );

    let (imported, _) = store
        .import_docx_template(ImportDocxTemplateRequest {
            source_path: master_src.to_string_lossy().to_string(),
            name: "Mẫu DOCX có logo và bảng".to_string(),
            group: TemplateGroup::Custom,
            doc_type: "masterLogoTable".to_string(),
            scope: StdDocScope::AdminNd30,
        })
        .unwrap();

    assert!(imported
        .detected_placeholders
        .iter()
        .any(|p| p == "custom_ma_du_an"));
    assert!(imported
        .detected_placeholders
        .iter()
        .any(|p| p == "so_van_ban"));
    assert!(imported.source_docx_asset_id.is_some());

    // Apply imported DOCX template
    let applied_out = dir.join("applied_from_imported.docx");
    let mut fields = sample_full_fields();
    fields
        .extra_fields
        .insert("custom_ma_du_an".to_string(), "DA-MONWORD-2026".to_string());
    let res = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: imported.template_id.clone(),
            org_profile_override: Some(sample_org_state()),
            target_mode: TemplateTargetMode::CreateNewDocx,
            dest_path: Some(applied_out.to_string_lossy().to_string()),
            dynamic_fields: fields,
            ..Default::default()
        },
    );
    assert!(res.success, "Err: {:?}", res.error);
    let out_pkg = OpenXmlPackage::open(&applied_out).unwrap();
    assert_eq!(
        out_pkg.get_bytes("word/media/logo.png").unwrap(),
        png.as_slice()
    );
    let doc_xml = out_pkg.get_xml("word/document.xml").unwrap();
    assert!(doc_xml.contains("DA-MONWORD-2026"));
    assert!(doc_xml.contains("SỞ NỘI VỤ"));
    assert!(!doc_xml.contains("{{custom_ma_du_an}}"));

    // Export system template as master DOCX
    let exported_master = dir.join("exported_totrinh.docx");
    template_engine::export_docx_template(&store, "adm-to-trinh", &exported_master).unwrap();
    let exp_pkg = OpenXmlPackage::open(&exported_master).unwrap();
    let exp_xml = exp_pkg.get_xml("word/document.xml").unwrap();
    assert!(exp_xml.contains("{{trich_yeu}}"));
    assert!(exp_xml.contains("{{nguoi_nhan}}"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_04_apply_all_20_templates_across_multiple_org_profiles() {
    let dir = make_temp_tpl_dir("case04_all20");
    let store = TemplateStore::with_root(&dir);
    let catalog = store.list_catalog().unwrap();

    let org_state = sample_org_state();
    let org_ent_corp = sample_org_ent_corporate();
    let fields = sample_full_fields();

    for tpl in &catalog.templates {
        let out_file = dir.join(format!("{}.docx", tpl.template_id));
        let org = if tpl.group == TemplateGroup::Enterprise {
            org_ent_corp.clone()
        } else {
            org_state.clone()
        };
        let res = template_engine::apply_template(
            &store,
            &crate::models::TemplateApplyRequest {
                template_id: tpl.template_id.clone(),
                org_profile_override: Some(org),
                target_mode: TemplateTargetMode::CreateNewDocx,
                dest_path: Some(out_file.to_string_lossy().to_string()),
                dynamic_fields: fields.clone(),
                ..Default::default()
            },
        );
        assert!(
            res.success && res.post_save_verified,
            "Mẫu '{}' thất bại: {:?}",
            tpl.template_id,
            res.error
        );
        let prev = res.preview.unwrap();
        assert_eq!(prev.pipeline_steps.len(), 10);
        assert!(prev.pipeline_steps.iter().all(|s| s.completed));

        let doc_xml = OpenXmlPackage::open(&out_file)
            .unwrap()
            .get_xml("word/document.xml")
            .unwrap();
        if tpl.group == TemplateGroup::Enterprise {
            assert!(doc_xml.contains("CÔNG TY TNHH GIẢI PHÁP SỐ ALPHA"));
            assert!(
                !doc_xml.contains("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM"),
                "Hồ sơ doanh nghiệp không dùng NĐ30 không được chèn Quốc hiệu"
            );
        } else {
            assert!(doc_xml.contains("SỞ NỘI VỤ"));
            assert!(doc_xml.contains("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM"));
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_05_dynamic_fields_and_split_run_substitution() {
    let dir = make_temp_tpl_dir("case05_split_runs");
    let store = TemplateStore::with_root(&dir);
    let target = dir.join("split_run_target.docx");

    // Deliberately split {{so_van_ban}} across 3 <w:r> elements with color formatting!
    let body = r#"
      <w:p>
        <w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Số hồ sơ: </w:t></w:r>
        <w:r><w:rPr><w:b/><w:color w:val="1F4E79"/></w:rPr><w:t>{{so_</w:t></w:r>
        <w:r><w:rPr><w:b/></w:rPr><w:t>van_ban</w:t></w:r>
        <w:r><w:rPr><w:b/></w:rPr><w:t>}}</w:t></w:r>
      </w:p>
      <w:p><w:r><w:t>Cơ quan: {{ten_co_quan}} | Địa danh: {{dia_danh}} | Ngày: {{ngay_thang}} | Ký: {{chuc_vu}} {{nguoi_ky}} | Gửi: {{nguoi_nhan}} | V/v: {{trich_yeu}}</w:t></w:r></w:p>
    "#;
    build_tpl_fixture_docx(&target, body, vec![], "", "", false);

    let res = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: "adm-cong-van".to_string(),
            org_profile_override: Some(sample_org_state()),
            target_mode: TemplateTargetMode::ApplyToExistingDocx,
            target_path: Some(target.to_string_lossy().to_string()),
            dest_path: Some(dir.join("split_out.docx").to_string_lossy().to_string()),
            dynamic_fields: sample_full_fields(),
            ..Default::default()
        },
    );
    assert!(res.success, "Err: {:?}", res.error);
    let xml = OpenXmlPackage::open(Path::new(&res.output_path))
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();

    assert!(xml.contains("125/SNV"));
    assert!(xml.contains("1F4E79"), "Định dạng màu của run đầu tiên bị mất");
    assert!(xml.contains("SỞ NỘI VỤ"));
    assert!(xml.contains("Buôn Ma Thuột"));
    assert!(xml.contains("ngày 05 tháng 02 năm 2026"));
    assert!(xml.contains("Nguyễn Văn An"));
    assert!(xml.contains("Giám đốc"));
    assert!(xml.contains("Các phòng, đơn vị trực thuộc"));
    assert!(xml.contains("Triển khai kế hoạch công tác năm 2026"));
    assert!(!xml.contains("{{"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_06_missing_data_explicit_markers_no_fake_data() {
    let dir = make_temp_tpl_dir("case06_missing");
    let store = TemplateStore::with_root(&dir);
    let out = dir.join("missing_markers.docx");

    let res = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: "adm-quyet-dinh".to_string(),
            org_profile_id: None,
            org_profile_override: None,
            target_mode: TemplateTargetMode::CreateNewDocx,
            dest_path: Some(out.to_string_lossy().to_string()),
            dynamic_fields: DynamicTemplateFields::default(),
            ..Default::default()
        },
    );
    assert!(res.success, "Err: {:?}", res.error);
    let prev = res.preview.unwrap();
    let missing_keys: Vec<&str> = prev
        .compatibility
        .missing_fields
        .iter()
        .map(|m| m.field_key.as_str())
        .collect();
    for expected_key in [
        "ten_co_quan",
        "so_van_ban",
        "dia_danh",
        "ngay_thang",
        "nguoi_ky",
        "chuc_vu",
        "nguoi_nhan",
        "trich_yeu",
    ] {
        assert!(
            missing_keys.contains(&expected_key),
            "Thiếu cảnh báo trường chưa nhập '{}'",
            expected_key
        );
    }

    let xml = OpenXmlPackage::open(&out)
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml.contains("[CHƯA NHẬP:"));
    assert!(xml.contains("ngày [..] tháng [..] năm [....]"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_07_template_with_tables_and_optional_sections() {
    let dir = make_temp_tpl_dir("case07_tables_sections");
    let store = TemplateStore::with_root(&dir);

    // 1. Even if optional sections are disabled, mandatory table & sections stay ON
    let out_no_opt = dir.join("kehoach_mandatory_only.docx");
    let res1 = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: "adm-ke-hoach".to_string(),
            org_profile_override: Some(sample_org_state()),
            target_mode: TemplateTargetMode::CreateNewDocx,
            dest_path: Some(out_no_opt.to_string_lossy().to_string()),
            enabled_section_ids: vec!["only-this-dummy-id".to_string()],
            dynamic_fields: sample_full_fields(),
            ..Default::default()
        },
    );
    assert!(res1.success);
    let xml1 = OpenXmlPackage::open(&out_no_opt)
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml1.contains("I. MỤC ĐÍCH, YÊU CẦU"));
    assert!(xml1.contains("Bảng phân công thực hiện kế hoạch"));
    assert!(xml1.contains("w:tblHeader"));
    assert!(!xml1.contains("III. KINH PHÍ VÀ ĐIỀU KIỆN BẢO ĐẢM"));

    // 2. Enable optional section kh-kinh-phi -> appears cleanly
    let out_with_opt = dir.join("kehoach_with_opt.docx");
    let res2 = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: "adm-ke-hoach".to_string(),
            org_profile_override: Some(sample_org_state()),
            target_mode: TemplateTargetMode::CreateNewDocx,
            dest_path: Some(out_with_opt.to_string_lossy().to_string()),
            enabled_section_ids: vec!["kh-kinh-phi".to_string()],
            dynamic_fields: sample_full_fields(),
            ..Default::default()
        },
    );
    assert!(res2.success);
    let xml2 = OpenXmlPackage::open(&out_with_opt)
        .unwrap()
        .get_xml("word/document.xml")
        .unwrap();
    assert!(xml2.contains("III. KINH PHÍ VÀ ĐIỀU KIỆN BẢO ĐẢM"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_08_preserve_logo_and_embedded_images() {
    let dir = make_temp_tpl_dir("case08_logo");
    let store = TemplateStore::with_root(&dir);
    let target = dir.join("doc_with_logo.docx");
    let png = minimal_png_bytes();

    let body = format!(
        r#"
      {}
      <w:p><w:r><w:t>Nội dung tài liệu có logo đính kèm.</w:t></w:r></w:p>
    "#,
        valid_drawing_paragraph_xml("rId30", "OrgLogo")
    );
    build_tpl_fixture_docx(
        &target,
        &body,
        vec![("word/media/image1.png", png.clone())],
        r#"<Relationship Id="rId30" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#,
        "",
        false,
    );

    let out = dir.join("doc_with_logo_out.docx");
    let res = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: "ent-thong-bao-noi-bo".to_string(),
            org_profile_override: Some(sample_org_ent_corporate()),
            target_mode: TemplateTargetMode::ApplyToExistingDocx,
            target_path: Some(target.to_string_lossy().to_string()),
            dest_path: Some(out.to_string_lossy().to_string()),
            dynamic_fields: sample_full_fields(),
            ..Default::default()
        },
    );
    assert!(res.success, "Err: {:?}", res.error);
    let pkg_out = OpenXmlPackage::open(&out).unwrap();
    assert_eq!(pkg_out.get_bytes("word/media/image1.png").unwrap(), png.as_slice());
    let prev = res.preview.unwrap();
    assert!(prev.compatibility.preserved_assets.drawing_count >= 1);
    assert!(prev.compatibility.preserved_assets.media_count >= 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_09_preserve_multi_section_and_landscape() {
    let dir = make_temp_tpl_dir("case09_multisection");
    let store = TemplateStore::with_root(&dir);
    let target = dir.join("multi_section.docx");

    let body = r#"
      <w:p><w:r><w:t>Trang 1 (Portrait) thuộc section đầu tiên.</w:t></w:r></w:p>
      <w:p>
        <w:pPr>
          <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>
        </w:pPr>
      </w:p>
      <w:p><w:r><w:t>Trang 2 (Landscape) chứa bảng rộng cần giữ nguyên hướng ngang.</w:t></w:r></w:p>
      <w:p>
        <w:pPr>
          <w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="1701" w:right="1134" w:bottom="1134" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>
        </w:pPr>
      </w:p>
      <w:p><w:r><w:t>Trang 3 (Portrait) kết luận báo cáo.</w:t></w:r></w:p>
      <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>
    "#;
    build_tpl_fixture_docx(&target, body, vec![], "", "", true);

    let out = dir.join("multi_section_out.docx");
    let res = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: "adm-bao-cao".to_string(),
            org_profile_override: Some(sample_org_state()),
            target_mode: TemplateTargetMode::ApplyToExistingDocx,
            target_path: Some(target.to_string_lossy().to_string()),
            dest_path: Some(out.to_string_lossy().to_string()),
            dynamic_fields: sample_full_fields(),
            ..Default::default()
        },
    );
    assert!(res.success, "Err: {:?}", res.error);
    let view = OpenXmlPackage::open_selective_for_analysis(&out).unwrap();
    let ir = DocumentIr::build(&out, &view, StdDocScope::AdminNd30, None).unwrap();
    assert_eq!(ir.sections.len(), 3, "Phải bảo toàn đủ 3 section");
    assert_eq!(ir.sections[1].summary.orientation, "landscape");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_10_signature_layouts_single_dual_triple() {
    let dir = make_temp_tpl_dir("case10_signatures");
    let store = TemplateStore::with_root(&dir);

    let p_single = dir.join("sig_single.docx");
    let p_dual = dir.join("sig_dual.docx");
    let p_triple = dir.join("sig_triple.docx");

    for (tpl_id, path) in [
        ("adm-cong-van", &p_single),
        ("adm-bien-ban", &p_dual),
        ("ent-phieu-de-xuat", &p_triple),
    ] {
        let res = template_engine::apply_template(
            &store,
            &crate::models::TemplateApplyRequest {
                template_id: tpl_id.to_string(),
                org_profile_override: Some(sample_org_state()),
                target_mode: TemplateTargetMode::CreateNewDocx,
                dest_path: Some(path.to_string_lossy().to_string()),
                dynamic_fields: sample_full_fields(),
                ..Default::default()
            },
        );
        assert!(res.success, "Err: {:?}", res.error);
    }

    let x1 = OpenXmlPackage::open(&p_single).unwrap().get_xml("word/document.xml").unwrap();
    let x2 = OpenXmlPackage::open(&p_dual).unwrap().get_xml("word/document.xml").unwrap();
    let x3 = OpenXmlPackage::open(&p_triple).unwrap().get_xml("word/document.xml").unwrap();

    assert!(x1.contains("Nơi nhận:"));
    assert!(x1.contains("Nguyễn Văn An"));
    assert!(x2.contains("THƯ KÝ"));
    assert!(x3.contains("NGƯỜI ĐỀ XUẤT"));
    assert!(x3.contains("TRƯỞNG BỘ PHẬN / TCKT"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tpl_case_11_complex_docx_preservation_and_real_microsoft_word_com() {
    let dir = make_temp_tpl_dir("case11_word_com");
    let store = TemplateStore::with_root(&dir);
    let png = minimal_png_bytes();

    let header1_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t xml:space="preserve">Mẫu kiểm chuẩn {{ten_co_quan}} - Trang </w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple></w:p>
</w:hdr>"#;

    let footer1_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t xml:space="preserve">Tổng số trang: </w:t></w:r><w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>2</w:t></w:r></w:fldSimple></w:p>
</w:ftr>"#;

    let footnotes_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>
  <w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>
  <w:footnote w:id="1"><w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> Chú thích bảo toàn OpenXML.</w:t></w:r></w:p></w:footnote>
</w:footnotes>"#;

    let body = format!(
        r#"
      {logo}
      <w:p>
        <w:bookmarkStart w:id="0" w:name="MucTieuChinh"/>
        <w:r><w:t xml:space="preserve">Tài liệu kế hoạch số </w:t></w:r>
        <w:r><w:rPr><w:b/><w:color w:val="1F4E79"/></w:rPr><w:t>{{{{so_</w:t></w:r>
        <w:r><w:rPr><w:b/></w:rPr><w:t>van_ban}}}}</w:t></w:r>
        <w:bookmarkEnd w:id="0"/>
        <w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="1"/></w:r>
      </w:p>
      <w:p>
        <w:hyperlink r:id="rId35" w:history="1"><w:r><w:t>Cổng thông tin điện tử</w:t></w:r></w:hyperlink>
      </w:p>
      <w:tbl>
        <w:tblPr><w:tblW w:w="9071" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders></w:tblPr>
        <w:tblGrid><w:gridCol w:w="4500"/><w:gridCol w:w="4571"/></w:tblGrid>
        <w:tr>
          <w:tc><w:tcPr><w:tcW w:w="4500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Trích yếu</w:t></w:r></w:p></w:tc>
          <w:tc><w:tcPr><w:tcW w:w="4571" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{{{{trich_yeu}}}}</w:t></w:r></w:p></w:tc>
        </w:tr>
      </w:tbl>
      <w:p>
        <w:pPr>
          <w:sectPr>
            <w:headerReference w:type="default" r:id="rId31"/>
            <w:footerReference w:type="default" r:id="rId32"/>
            <w:pgSz w:w="11906" w:h="16838"/>
            <w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/>
          </w:sectPr>
        </w:pPr>
      </w:p>
      <w:p><w:r><w:t>Bảng phụ lục trang ngang thuộc section 2.</w:t></w:r></w:p>
      <w:sectPr>
        <w:headerReference w:type="default" r:id="rId31"/>
        <w:footerReference w:type="default" r:id="rId32"/>
        <w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/>
        <w:pgMar w:top="1701" w:right="1134" w:bottom="1134" w:left="1134" w:header="708" w:footer="708" w:gutter="0"/>
      </w:sectPr>
    "#,
        logo = valid_drawing_paragraph_xml("rId34", "LogoCoQuan")
    );

    let complex_src = dir.join("complex_input.docx");
    build_tpl_fixture_docx(
        &complex_src,
        &body,
        vec![
            ("word/header1.xml", header1_xml.as_bytes().to_vec()),
            ("word/footer1.xml", footer1_xml.as_bytes().to_vec()),
            ("word/footnotes.xml", footnotes_xml.as_bytes().to_vec()),
            ("word/media/image1.png", png.clone()),
        ],
        r#"
          <Relationship Id="rId31" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
          <Relationship Id="rId32" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
          <Relationship Id="rId33" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
          <Relationship Id="rId34" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
          <Relationship Id="rId35" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://dichvucong.gov.vn" TargetMode="External"/>
        "#,
        r#"
          <Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
          <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
          <Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>
        "#,
        true,
    );

    let complex_out = dir.join("complex_applied.docx");
    let complex_res = template_engine::apply_template(
        &store,
        &crate::models::TemplateApplyRequest {
            template_id: "adm-ke-hoach".to_string(),
            org_profile_override: Some(sample_org_state()),
            target_mode: TemplateTargetMode::ApplyToExistingDocx,
            target_path: Some(complex_src.to_string_lossy().to_string()),
            dest_path: Some(complex_out.to_string_lossy().to_string()),
            dynamic_fields: sample_full_fields(),
            insert_framing_on_existing: true,
            ..Default::default()
        },
    );
    assert!(complex_res.success, "Err: {:?}", complex_res.error);

    let out_pkg = OpenXmlPackage::open(&complex_out).unwrap();
    assert_eq!(out_pkg.get_bytes("word/media/image1.png").unwrap(), png.as_slice());
    assert!(out_pkg.get_xml("word/header1.xml").unwrap().contains("SỞ NỘI VỤ"));
    let prev = complex_res.preview.unwrap();
    assert_eq!(prev.compatibility.preserved_assets.hyperlink_count, 1);
    assert_eq!(prev.compatibility.preserved_assets.bookmark_count, 1);
    assert_eq!(prev.compatibility.preserved_assets.footnote_count, 1);

    // Also generate representative fresh documents from library
    let gen_cv = dir.join("gen_congvan.docx");
    let gen_qd = dir.join("gen_quyetdinh.docx");
    let gen_bb = dir.join("gen_bienban.docx");
    let gen_sop = dir.join("gen_sop.docx");

    for (tpl_id, path, org) in [
        ("adm-cong-van", &gen_cv, sample_org_state()),
        ("adm-quyet-dinh", &gen_qd, sample_org_state()),
        ("adm-bien-ban", &gen_bb, sample_org_state()),
        ("ent-quy-trinh-nghiep-vu", &gen_sop, sample_org_ent_corporate()),
    ] {
        let r = template_engine::apply_template(
            &store,
            &crate::models::TemplateApplyRequest {
                template_id: tpl_id.to_string(),
                org_profile_override: Some(org),
                target_mode: TemplateTargetMode::CreateNewDocx,
                dest_path: Some(path.to_string_lossy().to_string()),
                dynamic_fields: sample_full_fields(),
                ..Default::default()
            },
        );
        assert!(r.success, "Err for {}: {:?}", tpl_id, r.error);
    }

    // Real Microsoft Word COM round-trip open & inspection
    verify_docs_in_real_word_com(&[
        (&complex_out, "125/KH-SNV"),
        (&gen_cv, "V/v Triển khai kế hoạch công tác năm 2026"),
        (&gen_qd, "QUYẾT ĐỊNH:"),
        (&gen_bb, "THƯ KÝ"),
        (&gen_sop, "KIỂM SOÁT TÀI LIỆU"),
    ]);

    let _ = fs::remove_dir_all(&dir);
}
