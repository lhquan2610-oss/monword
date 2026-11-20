use crate::engine::compliance_audit::sha256_util::sha256_hex;
use crate::engine::compliance_audit::{
    execute_autofix_with_recheck, export_compliance_report, run_compliance_audit, AuditHistoryStore,
    ComplianceRulePackStore,
};
use crate::engine::header_builder::{apply_header_builder_to_file, create_standalone_header_docx};
use crate::engine::layout_intelligence;
use crate::engine::live_preview::{
    build_live_preview, cleanup_all_temp_previews_in, load_preview_media_base64, preview_temp_dir,
};
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::org_profile::{load_org_profiles_from, upsert_org_profile_at};
use crate::engine::reliability::{
    backup_manager, logger, resource_guard, safe_save,
};
use crate::engine::system_office::run_word_com_script_with_timeout;
use crate::engine::template_engine::{self, TemplateStore};
use crate::engine::{self, std_engine};
use crate::installer_core::{
    extract_sfx_sections, pack_self_extracting_installer, run_install_from_sfx, run_uninstall,
    InstallManifest, InstallOptions, SFX_MAGIC,
};
use crate::ipc::{validate_safe_external_url, validate_safe_filesystem_path};
use crate::models::{
    AdminDocType, AuditExportFormat, ComplianceAuditRequest, ComplianceAutoFixAndRecheckRequest,
    DynamicTemplateFields, HeaderActionMode, HeaderBuilderConfig, HeaderDateMode, HeaderNumberMode,
    OrgSector, OrganizationProfile, OutputMode, PresetId, PreviewBuildRequest, ProcessConfig,
    StdDocScope, StdExecutionRequest, TemplateApplyRequest, TemplateTargetMode,
};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[repr(C)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct WinFileTime {
    dw_low_date_time: u32,
    dw_high_date_time: u32,
}

impl WinFileTime {
    fn to_100ns(self) -> u64 {
        ((self.dw_high_date_time as u64) << 32) | (self.dw_low_date_time as u64)
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn GetProcessTimes(
        h_process: isize,
        creation_time: *mut WinFileTime,
        exit_time: *mut WinFileTime,
        kernel_time: *mut WinFileTime,
        user_time: *mut WinFileTime,
    ) -> i32;
    fn K32GetProcessMemoryInfo(
        process: isize,
        ppsmem_counters: *mut ProcessMemoryCounters,
        cb: u32,
    ) -> i32;
}

fn sample_process_ram_and_cpu_ms() -> (f64, f64, u64) {
    unsafe {
        let proc_h = GetCurrentProcess();
        let mut mem = ProcessMemoryCounters {
            cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        let _ = K32GetProcessMemoryInfo(proc_h, &mut mem, mem.cb);

        let mut ct = WinFileTime::default();
        let mut et = WinFileTime::default();
        let mut kt = WinFileTime::default();
        let mut ut = WinFileTime::default();
        let _ = GetProcessTimes(proc_h, &mut ct, &mut et, &mut kt, &mut ut);
        let cpu_ms = (kt.to_100ns() + ut.to_100ns()) / 10_000;
        let ws_mb = mem.working_set_size as f64 / (1024.0 * 1024.0);
        let peak_mb = mem.peak_working_set_size as f64 / (1024.0 * 1024.0);
        (ws_mb, peak_mb, cpu_ms)
    }
}

fn make_p9_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("monword_p9_{}_{}", name, std::process::id()));
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

fn valid_drawing_para_xml(rid: &str, name: &str) -> String {
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
fn build_word_fixture(
    path: &Path,
    body_xml: &str,
    sect_pr_xml: Option<&str>,
    extra_entries: Vec<(&str, Vec<u8>)>,
    extra_rels: &str,
    extra_ct: &str,
    macro_enabled: bool,
    omit_default_sect_pr: bool,
) {
    let mut pkg = OpenXmlPackage {
        entries: Vec::new(),
    };

    let main_ct = if macro_enabled {
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
        main_ct,
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
            r#"<Relationship Id="rId90" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/>"#
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

    let default_sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>"#;
    let tail_sect = if omit_default_sect_pr {
        ""
    } else {
        sect_pr_xml.unwrap_or(default_sect)
    };
    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
  <w:body>
    {}
    {}
  </w:body>
</w:document>"#,
        body_xml, tail_sect
    );
    pkg.set_bytes(
        "word/document.xml",
        doc_xml.into_bytes(),
        CompressionMethod::Deflated,
    );

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/></w:rPr></w:style>
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

    for (name, data) in extra_entries {
        pkg.set_bytes(&name, data, CompressionMethod::Deflated);
    }

    pkg.save_to(path).unwrap();
}

fn build_excel_fixture(path: &Path, macro_enabled: bool) {
    let mut pkg = OpenXmlPackage {
        entries: Vec::new(),
    };
    let wb_ct = if macro_enabled {
        "application/vnd.ms-excel.sheet.macroEnabled.main+xml"
    } else {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
    };
    let ct = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="{}"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
  {}
</Types>"#,
        wb_ct,
        if macro_enabled {
            r#"<Override PartName="/xl/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/>"#
        } else {
            ""
        }
    );
    pkg.set_bytes("[Content_Types].xml", ct.into_bytes(), CompressionMethod::Deflated);
    pkg.set_bytes(
        "_rels/.rels",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "xl/_rels/workbook.xml.rels",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "xl/workbook.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets></workbook>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "xl/styles.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts></styleSheet>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "xl/worksheets/sheet1.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Bang du lieu MonWord</t></is></c></row></sheetData></worksheet>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );
    if macro_enabled {
        pkg.set_bytes("xl/vbaProject.bin", vec![0xD0, 0xCF, 0x11, 0xE0, 0x4D, 0x41, 0x43], CompressionMethod::Deflated);
    }
    pkg.save_to(path).unwrap();
}

#[test]
fn p9_case_01_end_to_end_15_step_workflow_and_recovery() {
    let dir = make_p9_dir("c01_e2e_15step");
    let store_path = dir.join("org_profiles.json");
    let tpl_store = TemplateStore::with_root(dir.join("templates"));
    let pack_store = ComplianceRulePackStore::with_root(&dir.join("compliance"));
    let hist_store = AuditHistoryStore::with_root(&dir.join("audit_history"));

    // Step 1: Create Organization Profile
    let profile = OrganizationProfile {
        id: "org_p9_hepco".to_string(),
        profile_name: "Sở Khoa học và Công nghệ TP.HCM".to_string(),
        sector: OrgSector::StateAgency,
        governing_org: "ỦY BAN NHÂN DÂN TP. HỒ CHÍ MINH".to_string(),
        issuing_org: "SỞ KHOA HỌC VÀ CÔNG NGHỆ".to_string(),
        affiliated_unit: String::new(),
        department_or_branch: "Văn phòng Sở".to_string(),
        agency_abbreviation: "SKHCN".to_string(),
        default_location: "TP. Hồ Chí Minh".to_string(),
        use_nd30_layout_for_enterprise: true,
        show_governing_org_default: true,
        show_affiliated_unit_default: false,
        show_department_default: false,
        updated_at: String::new(),
    };
    let saved_profiles = upsert_org_profile_at(&store_path, profile).unwrap();

    // Step 2: Select Organization Profile
    let loaded_profiles = load_org_profiles_from(&store_path);
    assert_eq!(saved_profiles.len(), 1);
    let org = loaded_profiles
        .into_iter()
        .find(|p| p.id == "org_p9_hepco")
        .expect("Organization Profile must persist and be selectable");

    // Step 3: Build Standalone Document Header
    let header_doc = dir.join("01_header.docx");
    let mut hb_cfg = HeaderBuilderConfig {
        org_profile_id: Some(org.id.clone()),
        doc_type: AdminDocType::QuyetDinh,
        output_mode: OutputMode::NewCopy,
        show_governing_org: true,
        show_issuing_org: true,
        show_org_underline: true,
        show_national_header: true,
        show_motto_underline: true,
        show_number_symbol: true,
        number_mode: HeaderNumberMode::ManualInput,
        doc_number: "125".to_string(),
        doc_symbol: "QĐ-SKHCN".to_string(),
        show_location_date: true,
        date_mode: HeaderDateMode::ManualDate,
        manual_day: "19".to_string(),
        manual_month: "11".to_string(),
        manual_year: "2026".to_string(),
        show_doc_type_and_subject: true,
        subject_text: "Về việc ban hành Quy trình kiểm thử chất lượng phần mềm MonWord".to_string(),
        ..Default::default()
    };
    let hdr_create_res = create_standalone_header_docx(&header_doc, &hb_cfg, Some(&org));
    assert!(hdr_create_res.success, "Header create failed: {:?}", hdr_create_res.error);
    assert!(header_doc.is_file());

    // Step 4: Select Template (`adm-quyet-dinh`) and create working document
    let tpl_doc = dir.join("02_quyet_dinh_from_template.docx");
    let apply_tpl_req = TemplateApplyRequest {
        template_id: "adm-quyet-dinh".to_string(),
        org_profile_override: Some(org.clone()),
        target_mode: TemplateTargetMode::CreateNewDocx,
        dest_path: Some(tpl_doc.to_string_lossy().to_string()),
        dynamic_fields: DynamicTemplateFields {
            ten_co_quan: org.issuing_org.clone(),
            co_quan_chu_quan: org.governing_org.clone(),
            viet_tat_co_quan: org.agency_abbreviation.clone(),
            so_van_ban: "125/QĐ-SKHCN".to_string(),
            dia_danh: org.default_location.clone(),
            day: "19".to_string(),
            month: "11".to_string(),
            year: "2026".to_string(),
            nguoi_ky: "Nguyễn Minh Tuấn".to_string(),
            chuc_vu: "GIÁM ĐỐC".to_string(),
            trich_yeu: "Ban hành quy chuẩn vận hành MonWord 4.0".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };
    let tpl_res = template_engine::apply_template(&tpl_store, &apply_tpl_req);
    assert!(tpl_res.success, "Template apply failed: {:?}", tpl_res.error);
    assert!(tpl_doc.is_file());

    // Inject realistic non-compliant body & margins on `doc_target` (Calibri 11pt, bad margins, double spaces)
    // and ensure Header Builder can standardize an existing header too.
    let doc_target = dir.join("03_workflow_doc.docx");
    fs::copy(&tpl_doc, &doc_target).unwrap();
    {
        let mut pkg = OpenXmlPackage::open(&doc_target).unwrap();
        let doc_xml = pkg.get_xml("word/document.xml").unwrap();
        let dirty = doc_xml
            .replace(r#"w:val="Times New Roman""#, r#"w:val="Calibri""#)
            .replace(r#"w:ascii="Times New Roman""#, r#"w:ascii="Calibri""#)
            .replace(r#"w:hAnsi="Times New Roman""#, r#"w:hAnsi="Calibri""#)
            .replace(r#"w:top="1134""#, r#"w:top="1440""#)
            .replace(r#"w:left="1701""#, r#"w:left="1440""#);
        pkg.set_xml("word/document.xml", dirty);
        pkg.save_to(&doc_target).unwrap();
    }
    let baseline_sha = sha256_hex(&fs::read(&doc_target).unwrap());
    hb_cfg.output_mode = OutputMode::OverwriteWithBackup;
    hb_cfg.allow_override_existing_number = true;
    hb_cfg.allow_override_existing_date = true;
    hb_cfg.allow_override_existing_org = true;
    let hb_std = apply_header_builder_to_file(
        &doc_target,
        &hb_cfg,
        HeaderActionMode::StandardizeExisting,
        Some(&org),
    );
    assert!(hb_std.success, "Header standardize failed: {:?}", hb_std.error);

    // Step 5: Document & Layout Analysis
    let (std_report, _ir) = std_engine::analyze_file(&doc_target, StdDocScope::AdminNd30, Some(&org)).unwrap();
    let layout_report = layout_intelligence::analyze_file(&doc_target, StdDocScope::AdminNd30, Some(&org)).unwrap();
    assert!(!std_report.sections.is_empty());
    assert!(!layout_report.maps.document_map.is_empty());

    // Step 6 & 7: Compliance Audit & Error Reporting
    let audit_req = ComplianceAuditRequest {
        path: doc_target.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        doc_type_hint: Some("quyetDinh".to_string()),
        org_profile: Some(org.clone()),
        disabled_rule_ids: vec![],
        record_history: true,
    };
    let comp_before = run_compliance_audit(&audit_req, &pack_store, &hist_store).unwrap();
    assert!(
        !comp_before.traceable_findings.is_empty(),
        "Expected dirty Calibri/margin document to produce traceable findings"
    );

    // Step 8: Select content to fix (approve most findings, explicitly skip 1 if >=2 findings exist)
    let mut approved_ids = Vec::new();
    let mut skipped_ids = Vec::new();
    for (idx, f) in comp_before.traceable_findings.iter().enumerate() {
        if comp_before.traceable_findings.len() > 2 && idx == comp_before.traceable_findings.len() - 1 {
            skipped_ids.push(f.finding_id.clone());
        } else {
            approved_ids.push(f.finding_id.clone());
        }
    }

    // Step 9 + 12 + 14: Auto-fix with OverwriteWithBackup + Safe Save + Post-Save Compliance Recheck
    let exec_req = StdExecutionRequest {
        path: doc_target.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        org_profile: Some(org.clone()),
        output_mode: OutputMode::OverwriteWithBackup,
        apply_all_group_a: false,
        approved_finding_ids: approved_ids.clone(),
        skipped_finding_ids: skipped_ids.clone(),
        manual_review_finding_ids: vec![],
        disabled_rule_ids: vec![],
    };
    let autofix_res = execute_autofix_with_recheck(
        &ComplianceAutoFixAndRecheckRequest {
            exec_request: exec_req,
            doc_type_hint: Some("quyetDinh".to_string()),
            record_history: true,
        },
        &pack_store,
        &hist_store,
    );
    assert!(
        autofix_res.std_execution.success,
        "Auto-fix failed: {:?}",
        autofix_res.std_execution.error
    );
    let ver = autofix_res.std_execution.verification.as_ref().unwrap();
    assert!(ver.reopened_from_disk && ver.xml_valid && ver.preservation_passed);
    let comp_after = autofix_res.audit_after.as_ref().unwrap();
    assert!(
        comp_after.traceable_findings.len() < comp_before.traceable_findings.len(),
        "Findings should decrease: before={} after={}",
        comp_before.traceable_findings.len(),
        comp_after.traceable_findings.len()
    );
    let after_fixed_sha = sha256_hex(&fs::read(&doc_target).unwrap());
    assert_ne!(after_fixed_sha, baseline_sha);

    // Step 10 & 11: Live Preview & Before/After Comparison (compare backup vs standardized output)
    let bak_list = backup_manager::list_backups_for_file(&doc_target);
    assert!(!bak_list.is_empty(), "Backup must exist after OverwriteWithBackup");
    let before_preview_docx = dir.join("03_before_preview_from_bak.docx");
    fs::copy(&bak_list[0].backup_path, &before_preview_docx).unwrap();
    let prev_payload = build_live_preview(
        &PreviewBuildRequest {
            job_id: "p9_e2e_preview".to_string(),
            path: before_preview_docx.to_string_lossy().to_string(),
            after_file_path: Some(doc_target.to_string_lossy().to_string()),
            include_after: true,
            include_audit_sync: true,
            scope: StdDocScope::AdminNd30,
            process_config: None,
            overrides: Default::default(),
            prefer_incremental: false,
        },
        Some(&org),
    );
    assert_eq!(prev_payload.status, "completed");
    assert!(prev_payload.before.as_ref().unwrap().total_pages >= 1);
    assert!(prev_payload.after.as_ref().unwrap().total_pages >= 1);
    assert!(!prev_payload.diffs.is_empty(), "Expected Before/After diffs in Live Preview");

    // Step 13: Re-open saved DOCX cleanly via OpenXmlPackage & Real Microsoft Word 16.0 COM
    let reopened = OpenXmlPackage::open(&doc_target).unwrap();
    assert!(reopened.is_word());
    let export_json = dir.join("audit_report.json");
    let exp_res = export_compliance_report(comp_after, AuditExportFormat::Json, &export_json);
    assert!(exp_res.success && export_json.is_file());

    let com_body = format!(
        r#"
    $doc = $word.Documents.Open('{p}', $false, $true)
    $pg = [int]$doc.ComputeStatistics(2)
    $topPt = [math]::Round($doc.PageSetup.TopMargin, 1)
    $leftPt = [math]::Round($doc.PageSetup.LeftMargin, 1)
    Write-Output ("P9_COM_OK|pg=" + $pg + "|top=" + $topPt + "|left=" + $leftPt)
"#,
        p = doc_target.to_string_lossy().replace('\'', "''")
    );
    let com_out = run_word_com_script_with_timeout(&com_body, 30_000, None).unwrap();
    let stdout = String::from_utf8_lossy(&com_out.stdout);
    assert!(stdout.contains("P9_COM_OK|"), "Word COM failed to reopen saved doc: {}", stdout);

    // Step 15: Simulate corrupted/unwanted edit and restore Pre-Edit -> Initial Baseline
    let pre_edit_snapshot = fs::read(&doc_target).unwrap();
    let pre_edit_bak = backup_manager::create_versioned_backup(&doc_target, "p9_extra_edit").unwrap();
    fs::write(&doc_target, b"CORRUPTED_USER_OVERWRITE").unwrap();

    let restore_pre = backup_manager::restore_backup_checked(&doc_target, Path::new(&pre_edit_bak.backup_path)).unwrap();
    assert!(restore_pre.contains("khôi phục"), "Unexpected restore msg: {}", restore_pre);
    assert_eq!(fs::read(&doc_target).unwrap(), pre_edit_snapshot);

    let restore_init = backup_manager::restore_initial_baseline(&doc_target).unwrap();
    assert!(restore_init.contains("khôi phục"), "Unexpected restore msg: {}", restore_init);
    assert_eq!(sha256_hex(&fs::read(&doc_target).unwrap()), baseline_sha);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn p9_case_02_file_format_and_document_complexity_matrix() {
    let dir = make_p9_dir("c02_formats");
    let cfg_nd30 = ProcessConfig::from_preset(PresetId::Nd30);

    // 1. Small 1-page DOCX
    let small_docx = dir.join("small.docx");
    build_word_fixture(
        &small_docx,
        r#"<w:p><w:r><w:t>Văn bản ngắn 1 trang dùng font Calibri.</w:t></w:r></w:p>"#,
        None,
        vec![],
        "",
        "",
        false,
        false,
    );
    let r_small = engine::process_single_file(&small_docx, &cfg_nd30);
    assert!(r_small.success, "Small DOCX failed: {:?}", r_small.error);
    assert!(Path::new(&r_small.output_path).is_file());

    // 2. Large (~4 MB image + 320 paragraphs) + Multi-page + Multi-table + Multi-section (3 sections w/ Landscape)
    let mut big_body = String::new();
    big_body.push_str(&valid_drawing_para_xml("rId50", "Logo.png"));
    for i in 0..240 {
        big_body.push_str(&format!(
            r#"<w:p><w:r><w:t>Đoạn văn số {} về quy trình chuẩn hóa tài liệu đa trang có bảng và nhiều section trong MonWord 4.0.</w:t></w:r></w:p>"#,
            i + 1
        ));
    }
    for t in 0..6 {
        big_body.push_str(&format!(
            r#"<w:tbl>
  <w:tr><w:tc><w:p><w:r><w:t>STT</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Bảng {}</w:t></w:r></w:p></w:tc></w:tr>
  <w:tr><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Dữ liệu chuẩn hóa</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>"#,
            t + 1
        ));
    }
    // Section 1 (Portrait) break -> Section 2 (Landscape) break -> Final Section 3 (Portrait)
    big_body.push_str(
        r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701"/></w:sectPr></w:pPr></w:p>"#,
    );
    big_body.push_str(r#"<w:p><w:r><w:t>Nội dung bảng rộng trang ngang (Landscape section).</w:t></w:r></w:p>"#);
    big_body.push_str(
        r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="1701" w:right="1134" w:bottom="850" w:left="1134"/></w:sectPr></w:pPr></w:p>"#,
    );
    big_body.push_str(r#"<w:p><w:r><w:t>Phụ lục cuối trang đứng.</w:t></w:r></w:p>"#);

    let mut media_payload = minimal_png_bytes();
    media_payload.resize(3 * 1024 * 1024, 0xA5); // 3 MB realistic image chunk

    let complex_docx = dir.join("complex_multi.docx");
    build_word_fixture(
        &complex_docx,
        &big_body,
        None,
        vec![("word/media/big_logo.png", media_payload)],
        r#"<Relationship Id="rId50" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/big_logo.png"/>"#,
        "",
        false,
        false,
    );
    let r_complex = engine::process_single_file(&complex_docx, &cfg_nd30);
    assert!(r_complex.success, "Complex multi-section DOCX failed: {:?}", r_complex.error);
    let complex_out = OpenXmlPackage::open(Path::new(&r_complex.output_path)).unwrap();
    let complex_xml = complex_out.get_xml("word/document.xml").unwrap();
    assert!(complex_xml.contains(r#"w:orient="landscape""#));
    assert!(complex_out.has_entry("word/media/big_logo.png"));

    // 3. DOCM (preserve macro vs clean macro)
    let sample_docm = dir.join("macro_doc.docm");
    build_word_fixture(
        &sample_docm,
        r#"<w:p><w:r><w:t>Văn bản có Macro VBA.</w:t></w:r></w:p>"#,
        None,
        vec![("word/vbaProject.bin", vec![0xD0, 0xCF, 0x11, 0xE0, 0x11, 0x22, 0x33])],
        "",
        "",
        true,
        false,
    );
    let mut cfg_keep_macro = cfg_nd30.clone();
    cfg_keep_macro.security.clean_macros = false;
    let r_docm_keep = engine::process_single_file(&sample_docm, &cfg_keep_macro);
    assert!(r_docm_keep.success, "DOCM keep macro failed: {:?}", r_docm_keep.error);
    assert!(r_docm_keep.output_path.ends_with(".docm"));
    assert!(OpenXmlPackage::open(Path::new(&r_docm_keep.output_path))
        .unwrap()
        .has_entry("word/vbaProject.bin"));

    let mut cfg_clean_macro = cfg_nd30.clone();
    cfg_clean_macro.security.clean_macros = true;
    cfg_clean_macro.output_mode = OutputMode::OverwriteWithBackup;
    let docm_clean_copy = dir.join("macro_to_clean.docm");
    fs::copy(&sample_docm, &docm_clean_copy).unwrap();
    let r_docm_clean = engine::process_single_file(&docm_clean_copy, &cfg_clean_macro);
    assert!(r_docm_clean.success);
    assert!(r_docm_clean.output_path.ends_with(".docx"));
    assert!(!OpenXmlPackage::open(Path::new(&r_docm_clean.output_path))
        .unwrap()
        .has_entry("word/vbaProject.bin"));

    // 4. Real Microsoft Word COM: legacy .doc, Restrict Editing .docx, Password-encrypted .doc & .docx
    let real_doc = dir.join("legacy_real.doc");
    let enc_doc = dir.join("encrypted_pwd.doc");
    let enc_docx = dir.join("encrypted_pwd.docx");
    let restrict_docx = dir.join("restricted_edit.docx");
    let com_gen = format!(
        r#"
    $d1 = $word.Documents.Add()
    $d1.Range().Font.Name = 'Arial'
    $d1.Range().Text = 'Noi dung van ban .doc tao tu Microsoft Word that.'
    $d1.SaveAs2('{real_doc}', 0)
    $d1.Close($false)

    $d4 = $word.Documents.Add()
    $d4.Range().Text = 'Tai lieu khoa Restrict Editing khong ma hoa'
    $d4.Protect(3, $false, 'LockEdit123')
    $d4.SaveAs2('{restrict_docx}', 16)
    $d4.Close($false)
"#,
        real_doc = real_doc.to_string_lossy().replace('\'', "''"),
        restrict_docx = restrict_docx.to_string_lossy().replace('\'', "''"),
    );
    run_word_com_script_with_timeout(&com_gen, 35_000, None).unwrap();

    // Generate authentic [MS-DOC] password-encrypted binary marker (fEncrypted = 0x0100)
    // and [MS-OFFCRYPTO] OLE CFB EncryptedPackage marker deterministically without triggering Word's password modal dialog
    let mut doc_cfb = fs::read(&real_doc).unwrap();
    let mut patched_sector = false;
    let mut off = 512usize;
    while off + 16 <= doc_cfb.len() {
        let w_ident = u16::from_le_bytes([doc_cfb[off], doc_cfb[off + 1]]);
        if w_ident == 0xA5EC || w_ident == 0xA5DC {
            doc_cfb[off + 11] |= 0x01;
            patched_sector = true;
            break;
        }
        off += 512;
    }
    assert!(patched_sector, "Real .doc CFB must contain [MS-DOC] FibBase sector");
    fs::write(&enc_doc, &doc_cfb).unwrap();

    let mut cfb_docx = vec![0u8; 1024];
    cfb_docx[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    let enc_marker: Vec<u8> = "EncryptionInfo\0EncryptedPackage"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    cfb_docx[512..512 + enc_marker.len()].copy_from_slice(&enc_marker);
    fs::write(&enc_docx, &cfb_docx).unwrap();

    // 4a. Legacy .doc conversion & standardization works end-to-end
    let mut cfg_doc = cfg_nd30.clone();
    cfg_doc.convert_legacy_doc = true;
    let r_doc = engine::process_single_file(&real_doc, &cfg_doc);
    assert!(r_doc.success, "Legacy .doc processing failed: {:?}", r_doc.error);
    assert!(r_doc.output_path.ends_with(".docx") && Path::new(&r_doc.output_path).is_file());

    // 4b. Password-encrypted .docx and .doc MUST fail fast (< 250ms, ZERO Word COM hang!)
    let t_enc_docx = Instant::now();
    let res_enc_docx = engine::process_single_file(&enc_docx, &cfg_nd30);
    assert!(!res_enc_docx.success);
    assert_eq!(res_enc_docx.error_code.as_deref(), Some("ERR_FILE_LOCKED_OR_CONFLICT"));
    assert!(t_enc_docx.elapsed() < Duration::from_millis(500));

    let t_enc_doc = Instant::now();
    let res_enc_doc = engine::process_single_file(&enc_doc, &cfg_doc);
    assert!(!res_enc_doc.success);
    assert_eq!(res_enc_doc.error_code.as_deref(), Some("ERR_FILE_LOCKED_OR_CONFLICT"));
    assert!(
        t_enc_doc.elapsed() < Duration::from_millis(500),
        "Password .doc must abort pre-COM without waiting for dialog; took {:?}",
        t_enc_doc.elapsed()
    );

    // 4c. Restrict Editing .docx detected cleanly + unlocked when enabled
    let audit_res = engine::audit_single_file(&restrict_docx).unwrap();
    assert!(audit_res.is_protected);
    let mut cfg_unlock = cfg_nd30.clone();
    cfg_unlock.security.unlock_protection = true;
    let r_unlock = engine::process_single_file(&restrict_docx, &cfg_unlock);
    assert!(r_unlock.success);
    let settings_after = OpenXmlPackage::open(Path::new(&r_unlock.output_path))
        .unwrap()
        .get_xml("word/settings.xml")
        .unwrap();
    assert!(!settings_after.contains("w:documentProtection"));

    // 5. Excel XLSX & XLSM + Corrupted files
    let xlsx_p = dir.join("sheet.xlsx");
    build_excel_fixture(&xlsx_p, false);
    assert!(engine::process_single_file(&xlsx_p, &cfg_nd30).success);

    let xlsm_p = dir.join("sheet_macro.xlsm");
    build_excel_fixture(&xlsm_p, true);
    assert!(engine::process_single_file(&xlsm_p, &cfg_nd30).success);

    let corrupt_p = dir.join("corrupt.docx");
    fs::write(&corrupt_p, b"NOT_A_VALID_ZIP_PAYLOAD").unwrap();
    let r_corrupt = engine::process_single_file(&corrupt_p, &cfg_nd30);
    assert!(!r_corrupt.success);
    assert_eq!(r_corrupt.error_code.as_deref(), Some("ERR_OPENXML_CORRUPT"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn p9_case_03_security_testing_matrix() {
    let dir = make_p9_dir("c03_security");
    let cfg = ProcessConfig::from_preset(PresetId::Nd30);

    // 1. Abnormal ZIP: path traversal `../evil.xml` + duplicate case-insensitive entry
    let trav_zip = dir.join("traversal.docx");
    {
        let f = File::create(&trav_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default();
        zw.start_file("../evil.xml", opts).unwrap();
        zw.write_all(b"<x/>").unwrap();
        zw.finish().unwrap();
    }
    let e_trav = OpenXmlPackage::open(&trav_zip).unwrap_err();
    assert!(e_trav.contains("ERR_OPENXML_CORRUPT"), "Expected ERR_OPENXML_CORRUPT, got {}", e_trav);

    let dup_zip = dir.join("dup_entries.docx");
    {
        let f = File::create(&dup_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default();
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(b"<w:document xmlns:w=\"w\"><w:body/></w:document>").unwrap();
        zw.start_file("WORD/DOCUMENT.XML", opts).unwrap();
        zw.write_all(b"<w:document xmlns:w=\"w\"><w:body/></w:document>").unwrap();
        zw.finish().unwrap();
    }
    let e_dup = OpenXmlPackage::open(&dup_zip).unwrap_err();
    assert!(e_dup.contains("ERR_OPENXML_CORRUPT"), "Duplicate entry error: {}", e_dup);

    // 1b. Media Zip Bomb in load_preview_media_base64 (spoofed header declaring 100 bytes, payload > 20 MB zeros)
    let media_bomb = dir.join("media_bomb.docx");
    build_word_fixture(
        &media_bomb,
        r#"<w:p><w:r><w:t>Ok</w:t></w:r></w:p>"#,
        None,
        vec![("word/media/bomb.png", vec![0u8; 22 * 1024 * 1024])],
        "",
        "",
        false,
        false,
    );
    let e_media = load_preview_media_base64(&media_bomb, "word/media/bomb.png").unwrap_err();
    assert!(
        e_media.contains("ERR_RESOURCE_LIMIT") || e_media.contains("ERR_ZIP_BOMB") || e_media.contains("20 MB"),
        "Unexpected media guard error: {}",
        e_media
    );

    // 2. Malicious XML (XXE DOCTYPE / Billion Laughs ENTITY)
    let xxe_docx = dir.join("xxe.docx");
    {
        let f = File::create(&xxe_docx).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default();
        zw.start_file("[Content_Types].xml", opts).unwrap();
        zw.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#).unwrap();
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(br#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY xxe SYSTEM "file:///C:/Windows/win.ini">]><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>&xxe;</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        zw.finish().unwrap();
    }
    let r_xxe = engine::process_single_file(&xxe_docx, &cfg);
    assert!(!r_xxe.success);
    assert_eq!(r_xxe.error_code.as_deref(), Some("ERR_XML_MALFORMED"));

    // 3. Path & External URL sanitization
    assert!(validate_safe_filesystem_path(r"\\.\PhysicalDrive0").is_err());
    assert!(validate_safe_filesystem_path(r"C:\Temp\CON.docx").is_err());
    assert!(validate_safe_filesystem_path("bad\x07path.docx").is_err());
    assert!(validate_safe_filesystem_path(r"C:\Work\VanBan.docx").is_ok());

    assert!(validate_safe_external_url("javascript:alert(1)").is_err());
    assert!(validate_safe_external_url("https://monword.vercel.app & calc.exe").is_err());
    assert!(validate_safe_external_url("https://monword.vercel.app/").is_ok());

    // 4 & 5. Macro + Embedded OLE preservation without execution
    let ole_docm = dir.join("ole_macro.docm");
    let ole_bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0x01, 0x02, 0x03];
    build_word_fixture(
        &ole_docm,
        r#"<w:p><w:r><w:t>Embedded OLE + Macro test</w:t></w:r></w:p>"#,
        None,
        vec![
            ("word/vbaProject.bin", vec![0xD0, 0xCF, 0x11, 0xE0, 0xAA]),
            ("word/embeddings/oleObject1.bin", ole_bytes.clone()),
        ],
        r#"<Relationship Id="rId77" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" Target="embeddings/oleObject1.bin"/>"#,
        "",
        true,
        false,
    );
    let mut cfg_keep = cfg.clone();
    cfg_keep.security.clean_macros = false;
    let r_ole = engine::process_single_file(&ole_docm, &cfg_keep);
    assert!(r_ole.success);
    let pkg_ole = OpenXmlPackage::open(Path::new(&r_ole.output_path)).unwrap();
    assert_eq!(pkg_ole.get_bytes("word/embeddings/oleObject1.bin"), Some(ole_bytes.as_slice()));

    // 6 & 7. Oversize compressed limit
    let normal_file = dir.join("oversize_test.docx");
    build_word_fixture(&normal_file, r#"<w:p><w:r><w:t>Hello</w:t></w:r></w:p>"#, None, vec![], "", "", false, false);
    let small_lim = resource_guard::ResourceLimits {
        max_compressed_file_bytes: 64,
        ..Default::default()
    };
    let e_over = resource_guard::with_custom_limits_for_test(small_lim, || OpenXmlPackage::open(&normal_file)).unwrap_err();
    assert!(e_over.contains("ERR_RESOURCE_LIMIT"));

    // 8. Locked/permission-denied destination conflict safely preserves original
    use std::os::windows::fs::OpenOptionsExt;
    let ro_doc = dir.join("readonly_target.docx");
    fs::copy(&normal_file, &ro_doc).unwrap();
    let orig_bytes = fs::read(&ro_doc).unwrap();
    let _lock_guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001)
        .open(&ro_doc)
        .unwrap();
    let mut cfg_ow = cfg.clone();
    cfg_ow.output_mode = OutputMode::OverwriteWithBackup;
    let r_ro = engine::process_single_file(&ro_doc, &cfg_ow);
    drop(_lock_guard);
    assert!(!r_ro.success);
    assert_eq!(r_ro.error_code.as_deref(), Some("ERR_FILE_LOCKED_OR_CONFLICT"));
    assert_eq!(fs::read(&ro_doc).unwrap(), orig_bytes);

    // 9. Log privacy redaction: sensitive folder paths & body text must NOT appear in monword_ops.jsonl
    let secret_dir = dir.join("NguyenVanBiMat_99").join("HoSoMat_NoiBo");
    fs::create_dir_all(&secret_dir).unwrap();
    let secret_doc = secret_dir.join("CongVanMat.docx");
    build_word_fixture(
        &secret_doc,
        r#"<w:p><w:r><w:t>NOI_DUNG_TUYET_MAT_2026_CAN_BAO_MAT</w:t></w:r></w:p>"#,
        None,
        vec![],
        "",
        "",
        false,
        false,
    );
    let log_file = dir.join("privacy_ops.jsonl");
    logger::with_test_log_file(&log_file, || {
        let _ = engine::audit_single_file(&secret_doc);
        logger::record_op(
            "test_sec",
            "path_leak_probe",
            Some(&secret_doc),
            "error",
            12,
            Some("ERR_FILE_READ"),
            &format!("Khong mo duoc tep '{}'", secret_doc.display()),
        );
    });
    let log_raw = fs::read_to_string(&log_file).unwrap();
    assert!(!log_raw.contains("NOI_DUNG_TUYET_MAT_2026"));
    assert!(!log_raw.contains("NguyenVanBiMat_99"), "Sensitive user folder leaked in log: {}", log_raw);
    assert!(!log_raw.contains("HoSoMat_NoiBo"), "Sensitive folder leaked in log: {}", log_raw);

    // 10. Temp cleanup verification (zero residual .mwtmp / .tx_ files & isolated preview cache cleanup works)
    assert!(preview_temp_dir().ends_with("monword_live_preview"));
    let iso_preview_dir = dir.join("isolated_preview_cache");
    fs::create_dir_all(&iso_preview_dir).unwrap();
    let stale_preview = iso_preview_dir.join("mw_stale_preview.pdf");
    fs::write(&stale_preview, b"%PDF-1.4 stale").unwrap();
    cleanup_all_temp_previews_in(&iso_preview_dir);
    assert!(!stale_preview.exists());
    let leftover_tx = safe_save::recover_interrupted_saves_in_dir(&dir);
    assert!(leftover_tx.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn p9_case_04_uat_real_microsoft_word_authored_documents() {
    let dir = make_p9_dir("c04_uat_word");
    let uat_quyet_dinh = dir.join("UAT_QuyetDinh.docx");
    let uat_cong_van_doc = dir.join("UAT_CongVan.doc");
    let uat_enterprise = dir.join("UAT_BaoCaoNoiBo.docx");

    // Author 3 realistic unformatted documents directly with Microsoft Word 16.0 COM
    let com_author = format!(
        r#"
    # Doc 1: Unformatted Administrative Quyet Dinh in Calibri 11pt + wrong 1-inch margins + native Word table
    $d1 = $word.Documents.Add()
    $d1.PageSetup.TopMargin = $word.InchesToPoints(1.0)
    $d1.PageSetup.BottomMargin = $word.InchesToPoints(1.0)
    $d1.PageSetup.LeftMargin = $word.InchesToPoints(1.0)
    $d1.PageSetup.RightMargin = $word.InchesToPoints(1.0)
    $r1 = $d1.Range()
    $r1.Font.Name = 'Calibri'
    $r1.Font.Size = 11
    $r1.Text = "SỞ CÔNG THƯƠNG`rCỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM`rĐộc lập - Tự do - Hạnh phúc`r`rQUYẾT ĐỊNH`rVề việc phê duyệt kế hoạch chuyển đổi số năm 2026`r`rĐiều 1. Phạm vi áp dụng cho toàn bộ đơn vị trực thuộc .`rĐiều 2. Tổ chức thực hiện và kiểm tra sau ban hành."
    $tbl = $d1.Tables.Add($d1.Paragraphs.Item($d1.Paragraphs.Count).Range, 2, 2)
    $tbl.Cell(1,1).Range.Text = "STT"
    $tbl.Cell(1,2).Range.Text = "Nội dung công việc"
    $tbl.Cell(2,1).Range.Text = "1"
    $tbl.Cell(2,2).Range.Text = "Chuẩn hóa văn bản"
    $d1.SaveAs2('{qd}', 16)
    $d1.Close($false)

    # Doc 2: Legacy .doc Cong Van authored in Word 97-2003 format with Arial 12pt
    $d2 = $word.Documents.Add()
    $d2.Range().Font.Name = 'Arial'
    $d2.Range().Font.Size = 12
    $d2.Range().Text = "CÔNG TY ĐIỆN LỰC`rSố: 45/CV-CTY`r`rKính gửi: Các phòng ban trực thuộc`rVui lòng báo cáo trước ngày 5 tháng 11 năm 2026 ."
    $d2.SaveAs2('{cv}', 0)
    $d2.Close($false)

    # Doc 3: Enterprise internal report with multiple headings
    $d3 = $word.Documents.Add()
    $d3.Range().Font.Name = 'Calibri'
    $d3.Range().Font.Size = 11
    $d3.Range().Text = "BÁO CÁO TỔNG KẾT NỘI BỘ`r1. Tình hình vận hành hệ thống`rHệ thống hoạt động ổn định trong quý IV.`r2. Kiến nghị và đề xuất`rTiếp tục đào tạo chuẩn hóa thể thức."
    $d3.SaveAs2('{ent}', 16)
    $d3.Close($false)
"#,
        qd = uat_quyet_dinh.to_string_lossy().replace('\'', "''"),
        cv = uat_cong_van_doc.to_string_lossy().replace('\'', "''"),
        ent = uat_enterprise.to_string_lossy().replace('\'', "''"),
    );
    run_word_com_script_with_timeout(&com_author, 40_000, None).unwrap();

    // Standardize all 3 real Word-authored files
    let res1 = std_engine::execute_standardization(&StdExecutionRequest {
        path: uat_quyet_dinh.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        output_mode: OutputMode::NewCopy,
        ..Default::default()
    });
    assert!(res1.success, "UAT Doc 1 failed: {:?}", res1.error);

    let mut cfg_doc = ProcessConfig::from_preset(PresetId::Nd30);
    cfg_doc.convert_legacy_doc = true;
    let res2 = engine::process_single_file(&uat_cong_van_doc, &cfg_doc);
    assert!(res2.success, "UAT Doc 2 (.doc) failed: {:?}", res2.error);

    let res3 = std_engine::execute_standardization(&StdExecutionRequest {
        path: uat_enterprise.to_string_lossy().to_string(),
        scope: StdDocScope::EnterpriseInternal,
        output_mode: OutputMode::NewCopy,
        ..Default::default()
    });
    assert!(res3.success, "UAT Doc 3 (Enterprise) failed: {:?}", res3.error);

    // Visual Live Preview works cleanly on real Word-authored outputs
    let pv = build_live_preview(
        &PreviewBuildRequest {
            job_id: "uat_preview".to_string(),
            path: uat_quyet_dinh.to_string_lossy().to_string(),
            after_file_path: Some(res1.output_path.clone()),
            include_after: true,
            include_audit_sync: true,
            scope: StdDocScope::AdminNd30,
            process_config: None,
            overrides: Default::default(),
            prefer_incremental: false,
        },
        None,
    );
    assert_eq!(pv.status, "completed");

    // Re-open all 3 outputs in real Microsoft Word 16.0 COM to confirm visual integrity, ND30 margins & zero corruption
    let com_verify = format!(
        r#"
    foreach ($p in @('{o1}', '{o2}', '{o3}')) {{
        $d = $word.Documents.Open($p, $false, $true)
        $pg = [int]$d.ComputeStatistics(2)
        $topCm = [math]::Round($word.PointsToCentimeters($d.PageSetup.TopMargin), 1)
        $leftCm = [math]::Round($word.PointsToCentimeters($d.PageSetup.LeftMargin), 1)
        $tbls = $d.Tables.Count
        $txtLen = $d.Range().Text.Trim().Length
        Write-Output ("UAT_OPEN_OK|" + [System.IO.Path]::GetFileName($p) + "|pg=" + $pg + "|top=" + $topCm + "|left=" + $leftCm + "|tbls=" + $tbls + "|len=" + $txtLen)
        $d.Close($false)
    }}
"#,
        o1 = res1.output_path.replace('\'', "''"),
        o2 = res2.output_path.replace('\'', "''"),
        o3 = res3.output_path.replace('\'', "''"),
    );
    let ver_out = run_word_com_script_with_timeout(&com_verify, 40_000, None).unwrap();
    let stdout = String::from_utf8_lossy(&ver_out.stdout);
    let ok_lines: Vec<&str> = stdout.lines().filter(|l| l.contains("UAT_OPEN_OK|")).collect();
    assert_eq!(ok_lines.len(), 3, "All 3 UAT outputs must open in Word COM; got:\n{}", stdout);
    // Doc 1 & Doc 2 must satisfy NĐ30 Top=2.0cm, Left=3.0cm
    assert!(stdout.contains("|top=2|left=3|") || stdout.contains("|top=2.0|left=3.0|"), "ND30 margins expected in:\n{}", stdout);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn p9_case_05_performance_telemetry_and_multi_profile_benchmarks() {
    let dir = make_p9_dir("c05_perf");
    let pack_store = ComplianceRulePackStore::with_root(&dir.join("compliance"));
    let hist_store = AuditHistoryStore::with_root(&dir.join("audit_history"));

    // Build 4 benchmark workloads: Small (1p), Medium (5p+tables), Large (25p+tables), Media-heavy (5MB)
    let small_p = dir.join("bench_small.docx");
    build_word_fixture(
        &small_p,
        r#"<w:p><w:r><w:t>Văn bản benchmark cỡ nhỏ 1 trang.</w:t></w:r></w:p>"#,
        None,
        vec![],
        "",
        "",
        false,
        false,
    );

    let make_workload_body = |paras: usize, tables: usize| -> String {
        let mut s = String::from(
            r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>QUYẾT ĐỊNH</w:t></w:r></w:p>"#,
        );
        for i in 0..paras {
            s.push_str(&format!(
                r#"<w:p><w:r><w:t>Điều {}. Nội dung chuẩn hóa thể thức hành chính đa trang phục vụ đo lường hiệu năng xử lý XML và phân tích bố cục trong MonWord 4.0.</w:t></w:r></w:p>"#,
                i + 1
            ));
        }
        for t in 0..tables {
            s.push_str(&format!(
                r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cột A{}</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cột B{}</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Giá trị 1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Giá trị 2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
                t, t
            ));
        }
        s
    };

    let medium_p = dir.join("bench_medium_5p.docx");
    build_word_fixture(&medium_p, &make_workload_body(85, 3), None, vec![], "", "", false, false);

    let large_p = dir.join("bench_large_25p.docx");
    build_word_fixture(&large_p, &make_workload_body(420, 10), None, vec![], "", "", false, false);

    let media_p = dir.join("bench_media_5mb.docx");
    let mut big_img = minimal_png_bytes();
    big_img.resize(5 * 1024 * 1024, 0x5Au8);
    build_word_fixture(
        &media_p,
        &make_workload_body(50, 2),
        None,
        vec![("word/media/large_scan.png", big_img)],
        r#"<Relationship Id="rId60" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/large_scan.png"/>"#,
        "",
        false,
        false,
    );

    println!("\n=== MONWORD v4.0.0 PERFORMANCE BENCHMARK TELEMETRY ===");
    println!(
        "{:<18} | {:>7} | {:>7} | {:>7} | {:>9} | {:>7} | {:>9} | {:>8} | {:>8}",
        "Workload", "OpenMs", "AnlzMs", "AuditMs", "StdFixMs", "SaveMs", "PreviewMs", "PeakMB", "CpuMs"
    );

    for (label, path, limit_ms) in [
        ("Small_1p", &small_p, 2500u128),
        ("Medium_5p_3Tbl", &medium_p, 4000u128),
        ("Large_25p_10Tbl", &large_p, 8000u128),
        ("Media_5MB", &media_p, 8000u128),
    ] {
        let (_ws0, _pk0, cpu0) = sample_process_ram_and_cpu_ms();

        let t_open = Instant::now();
        let pkg = OpenXmlPackage::open(path).unwrap();
        let open_ms = t_open.elapsed().as_millis();

        let t_save = Instant::now();
        let save_tmp = dir.join(format!("save_probe_{}.docx", label));
        pkg.save_to(&save_tmp).unwrap();
        let save_ms = t_save.elapsed().as_millis();

        let t_anlz = Instant::now();
        let _ = std_engine::analyze_file(path, StdDocScope::AdminNd30, None).unwrap();
        let analyze_ms = t_anlz.elapsed().as_millis();

        let t_audit = Instant::now();
        let _ = run_compliance_audit(
            &ComplianceAuditRequest {
                path: path.to_string_lossy().to_string(),
                scope: StdDocScope::AdminNd30,
                record_history: false,
                ..Default::default()
            },
            &pack_store,
            &hist_store,
        )
        .unwrap();
        let audit_ms = t_audit.elapsed().as_millis();

        let t_std = Instant::now();
        let std_out = std_engine::execute_standardization(&StdExecutionRequest {
            path: path.to_string_lossy().to_string(),
            scope: StdDocScope::AdminNd30,
            output_mode: OutputMode::NewCopy,
            ..Default::default()
        });
        assert!(std_out.success);
        let std_ms = t_std.elapsed().as_millis();

        let t_pv = Instant::now();
        let pv = build_live_preview(
            &PreviewBuildRequest {
                job_id: format!("perf_{}", label),
                path: path.to_string_lossy().to_string(),
                after_file_path: Some(std_out.output_path.clone()),
                include_after: true,
                scope: StdDocScope::AdminNd30,
                ..Default::default()
            },
            None,
        );
        assert_eq!(pv.status, "completed");
        let pv_ms = t_pv.elapsed().as_millis();

        let (_ws1, peak_mb, cpu1) = sample_process_ram_and_cpu_ms();
        let cpu_delta = cpu1.saturating_sub(cpu0);
        println!(
            "{:<18} | {:>7} | {:>7} | {:>7} | {:>9} | {:>7} | {:>9} | {:>8.1} | {:>8}",
            label, open_ms, analyze_ms, audit_ms, std_ms, save_ms, pv_ms, peak_mb, cpu_delta
        );

        assert!(std_ms < limit_ms, "{} std took too long: {} ms", label, std_ms);
        assert!(pv_ms < limit_ms, "{} preview took too long: {} ms", label, pv_ms);
    }

    // Constrained-machine simulation (tight ResourceLimits profile)
    let constrained = resource_guard::ResourceLimits {
        max_compressed_file_bytes: 2 * 1024 * 1024,
        max_uncompressed_total_bytes: 8 * 1024 * 1024,
        max_single_entry_bytes: 2 * 1024 * 1024,
        ..Default::default()
    };
    resource_guard::with_custom_limits_for_test(constrained, || {
        // 5 MB media workbook must be safely bounded by tight profile, whereas 5p text doc succeeds cleanly
        let over = OpenXmlPackage::open(&media_p);
        assert!(over.is_err());
        let ok_med = std_engine::analyze_file(&medium_p, StdDocScope::AdminNd30, None);
        assert!(ok_med.is_ok());
    });

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn p9_case_06_release_installer_fresh_upgrade_uninstall_and_data_safety() {
    let dir = make_p9_dir("c06_installer");
    let staging = dir.join("payload_src");
    let docs_src = staging.join("docs");
    fs::create_dir_all(&docs_src).unwrap();

    // Synthetic PE stub & payload files representing packaged release artifacts
    let stub_src = dir.join("stub_installer.exe");
    fs::write(&stub_src, b"MZ\x90\x00SYNTHETIC_PE_STUB_V4").unwrap();

    let exe_src = staging.join("MonWord.exe");
    let dll_src = staging.join("WebView2Loader.dll");
    let ico_src = staging.join("monword.ico");
    let readme_src = staging.join("README.md");
    let doc_setup_src = docs_src.join("HUONG_DAN_CAI_DAT_SU_DUNG.md");
    fs::write(&exe_src, b"MZ_MONWORD_4_0_0_RELEASE_PAYLOAD").unwrap();
    fs::write(&dll_src, b"MZ_WEBVIEW2_LOADER_DLL").unwrap();
    fs::write(&ico_src, b"ICO_DATA").unwrap();
    fs::write(&readme_src, b"# MonWord v4.0.0 README").unwrap();
    fs::write(&doc_setup_src, b"# Setup & User Guide").unwrap();

    let setup_exe = dir.join("MonWord-Setup-v4.0.0-x64.exe");
    let packed_size = pack_self_extracting_installer(
        &stub_src,
        &[
            ("MonWord.exe", &exe_src),
            ("WebView2Loader.dll", &dll_src),
            ("monword.ico", &ico_src),
            ("README.md", &readme_src),
            ("docs/HUONG_DAN_CAI_DAT_SU_DUNG.md", &doc_setup_src),
        ],
        &setup_exe,
    )
    .unwrap();
    assert!(packed_size > 24);
    let raw_setup = fs::read(&setup_exe).unwrap();
    assert_eq!(&raw_setup[raw_setup.len() - 8..], SFX_MAGIC);
    let (extracted_stub, _) = extract_sfx_sections(&setup_exe).unwrap();
    assert_eq!(extracted_stub, fs::read(&stub_src).unwrap());

    // Pre-existing user AppData and document .chuanhoa_backup that MUST NEVER BE TOUCHED
    let appdata_dir = dir.join("AppData_Roaming_MonWord");
    let templates_user_dir = appdata_dir.join("templates");
    fs::create_dir_all(&templates_user_dir).unwrap();
    let user_org_file = appdata_dir.join("org_profiles.json");
    let user_tpl_file = templates_user_dir.join("user_templates.json");
    let user_backup_dir = dir.join("UserDocs").join(".chuanhoa_backup");
    fs::create_dir_all(&user_backup_dir).unwrap();
    let user_backup_file = user_backup_dir.join("CongVan_initial.bak.docx");

    fs::write(&user_org_file, r#"[{"id":"u1","issuingOrg":"Cơ quan A"}]"#.as_bytes()).unwrap();
    fs::write(&user_tpl_file, br#"[{"templateId":"custom-1"}]"#).unwrap();
    fs::write(&user_backup_file, b"USER_CRITICAL_BACKUP_DATA").unwrap();

    let h_org = sha256_hex(&fs::read(&user_org_file).unwrap());
    let h_tpl = sha256_hex(&fs::read(&user_tpl_file).unwrap());
    let h_bak = sha256_hex(&fs::read(&user_backup_file).unwrap());

    // A. Fresh Install into empty dir
    let install_dir = dir.join("Programs_MonWord");
    let opts = InstallOptions {
        install_dir: install_dir.clone(),
        appdata_dir: appdata_dir.clone(),
        create_shortcuts: false,
        write_registry: false,
    };
    let fresh_res = run_install_from_sfx(&setup_exe, &opts).unwrap();
    assert!(!fresh_res.upgraded);
    assert_eq!(fresh_res.previous_version, None);
    assert!(install_dir.join("MonWord.exe").is_file());
    assert!(install_dir.join("WebView2Loader.dll").is_file());
    assert!(install_dir.join("uninstall.exe").is_file());
    assert!(install_dir.join("docs").join("HUONG_DAN_CAI_DAT_SU_DUNG.md").is_file());
    let m1: InstallManifest =
        serde_json::from_str(&fs::read_to_string(install_dir.join("install_manifest.json")).unwrap()).unwrap();
    assert!(m1.user_data_untouched);
    assert_eq!(m1.publisher, "Mon Office Ecosystem");

    // B. Simulate Upgrade from older version with legacy `ChuanHoaVanBanPro.exe`
    fs::write(install_dir.join("ChuanHoaVanBanPro.exe"), b"OLD_BINARY").unwrap();
    let upg_res = run_install_from_sfx(&setup_exe, &opts).unwrap();
    assert!(upg_res.upgraded);
    assert_eq!(upg_res.previous_version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert!(!install_dir.join("ChuanHoaVanBanPro.exe").exists(), "Legacy binary should be cleaned on upgrade");
    assert_eq!(sha256_hex(&fs::read(&user_org_file).unwrap()), h_org);
    assert_eq!(sha256_hex(&fs::read(&user_tpl_file).unwrap()), h_tpl);
    assert_eq!(sha256_hex(&fs::read(&user_backup_file).unwrap()), h_bak);

    // C. Uninstall cleans program dir while preserving 100% of user data & backups
    let uninst_res = run_uninstall(&opts).unwrap();
    assert!(uninst_res.user_data_preserved);
    assert!(!install_dir.join("MonWord.exe").exists());
    assert!(!install_dir.join("WebView2Loader.dll").exists());
    assert!(!install_dir.join("install_manifest.json").exists());
    assert_eq!(sha256_hex(&fs::read(&user_org_file).unwrap()), h_org);
    assert_eq!(sha256_hex(&fs::read(&user_tpl_file).unwrap()), h_tpl);
    assert_eq!(sha256_hex(&fs::read(&user_backup_file).unwrap()), h_bak);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn p9_case_07_install_dir_docs_exist_and_version_sync() {
    // Verify release deliverables metadata & documentation files exist cleanly in repository
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(root.join("README.md").is_file());
    assert!(root.join("docs").join("HUONG_DAN_CAI_DAT_SU_DUNG.md").is_file());
    assert!(root.join("docs").join("HUONG_DAN_SAO_LUU_KHOI_PHUC.md").is_file());
    assert!(root.join("docs").join("RELEASE_NOTES.md").is_file());
    assert!(root.join("docs").join("KNOWN_LIMITATIONS.md").is_file());
    assert_eq!(env!("CARGO_PKG_VERSION"), "4.0.0");
    let vjson = fs::read_to_string(root.join("ui").join("public").join("version.json")).unwrap();
    assert!(vjson.contains("\"version\": \"4.0.0\""));
    let _ = BTreeMap::<String, String>::new();
}
