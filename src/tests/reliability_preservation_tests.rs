use crate::engine::compliance_audit::{
    exporter, AuditHistoryStore, ComplianceRulePackStore,
};
use crate::engine::header_builder::apply_header_builder_to_file;
use crate::engine::layout_intelligence;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::org_profile;
use crate::engine::reliability::backup_manager::{
    create_versioned_backup_with_policy, list_backups_for_file, restore_backup_checked,
    restore_initial_baseline, restore_latest_pre_edit, RetentionPolicy,
};
use crate::engine::reliability::error_code::{classify_error_message, AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::{read_logs_from_file, with_test_log_file};
use crate::engine::reliability::preservation::{
    verify_openxml_preservation, PreservationPolicy, PreservationSnapshot,
};
use crate::engine::reliability::resource_guard::{
    is_batch_cancelled, request_batch_cancel, reset_batch_cancel, with_custom_limits_for_test,
    ResourceLimits,
};
use crate::engine::reliability::safe_save::{
    recover_interrupted_saves_in_dir, save_openxml_verified, with_fault_mode_for_test, FaultMode,
};
use crate::engine::std_engine::{self};
use crate::engine::system_office;
use crate::engine::template_engine::{self, TemplateStore};
use crate::engine::xml_helpers::XmlDocument;
use crate::engine::{self};
use crate::engine::compliance_audit::sha256_util::sha256_hex;
use crate::models::{
    AuditExportFormat, AuditFileDigest, AuditRuleProfileUsed, BackupKind, ComplianceAuditReport,
    DocumentConsistencyMetrics, HeaderActionMode, HeaderBuilderConfig, LayoutExecutionRequest,
    OrgSector, OrganizationProfile, OutputMode, PresetId, ProcessConfig, StdDocScope,
    StdExecutionRequest, TemplateApplyRequest, TemplateTargetMode,
};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn make_temp_rel_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chuanhoa_rel_test_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Valid minimal 1x1 PNG binary for realistic media embedding tests.
fn minimal_png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn build_simple_docx(path: &Path, paragraph_text: &str) {
    let mut pkg = OpenXmlPackage { entries: Vec::new() };
    pkg.set_bytes(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
</Types>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "word/_rels/document.xml.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
</Relationships>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );
    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>{}</w:t></w:r></w:p>
    <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="720" w:footer="720"/></w:sectPr>
  </w:body>
</w:document>"#,
        paragraph_text
    );
    pkg.set_bytes("word/document.xml", doc_xml.into_bytes(), CompressionMethod::Deflated);
    pkg.set_bytes(
        "word/styles.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
</w:styles>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "word/settings.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.save_to(path).unwrap();
}

/// Builds a rich multi-component `.docx` or `.docm` fixture covering all OpenXML parts required in Phase 8.
fn build_rich_component_package(path: &Path, include_macro_and_ole_binaries: bool) {
    let mut pkg = OpenXmlPackage { entries: Vec::new() };

    let main_ct = if include_macro_and_ole_binaries {
        "application/vnd.ms-word.document.macroEnabled.main+xml"
    } else {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
    };
    let macro_cts = if include_macro_and_ole_binaries {
        r#"  <Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/>
  <Override PartName="/word/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/>
  <Override PartName="/word/vbaData.xml" ContentType="application/vnd.ms-word.vbaData+xml"/>"#
    } else {
        ""
    };

    let ct = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="png" ContentType="image/png"/>
  {macro_cts}
  <Override PartName="/word/document.xml" ContentType="{main_ct}"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
  <Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>
  <Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/>
  <Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>
  <Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
  <Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>
</Types>"#,
        macro_cts = macro_cts,
        main_ct = main_ct
    );
    pkg.set_bytes("[Content_Types].xml", ct.as_bytes().to_vec(), CompressionMethod::Deflated);

    pkg.set_bytes(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );

    let extra_rels = if include_macro_and_ole_binaries {
        r#"  <Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" Target="embeddings/oleObject1.bin"/>
  <Relationship Id="rId11" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/>"#
    } else {
        ""
    };
    let doc_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
  <Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
  <Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/>
  <Relationship Id="rId6" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>
  <Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
  <Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
  <Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
{extra_rels}
  <Relationship Id="rId12" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.org/nd30-ref" TargetMode="External"/>
</Relationships>"#,
        extra_rels = extra_rels
    );
    pkg.set_bytes(
        "word/_rels/document.xml.rels",
        doc_rels.into_bytes(),
        CompressionMethod::Deflated,
    );

    let ole_run = if include_macro_and_ole_binaries {
        r#"      <w:r>
        <w:object w:dxaOrig="1440" w:dyaOrig="1440">
          <v:shape id="_x0000_i1025" style="width:72pt;height:72pt"><v:imagedata r:id="rId9"/></v:shape>
          <o:OLEObject Type="Embed" ProgID="Package" ShapeID="_x0000_i1025" DrawAspect="Icon" ObjectID="_1" r:id="rId10"/>
        </w:object>
      </w:r>"#
    } else {
        ""
    };

    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
            xmlns:v="urn:schemas-microsoft-com:vml"
            xmlns:o="urn:schemas-microsoft-com:office:office">
  <w:body>
    <w:p>
      <w:pPr><w:jc w:val="center"/></w:pPr>
      <w:r><w:t>CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:jc w:val="center"/></w:pPr>
      <w:r><w:t>Độc lập - Tự do - Hạnh phúc</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="MyCustomCorpStyle"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="9"/></w:numPr></w:pPr>
      <w:commentRangeStart w:id="1"/>
      <w:r><w:t>Điều 1. Nội dung văn bản có chú thích, trường động và liên kết </w:t></w:r>
      <w:commentRangeEnd w:id="1"/>
      <w:r><w:commentReference w:id="1"/></w:r>
      <w:r><w:footnoteReference w:id="2"/></w:r>
      <w:r><w:endnoteReference w:id="3"/></w:r>
      <w:hyperlink r:id="rId12"><w:r><w:t>Tham chiếu NĐ30</w:t></w:r></w:hyperlink>
    </w:p>
    <w:p>
      <w:r><w:fldSimple w:instr=" SEQ VanBan \* ARABIC "><w:r><w:t>1</w:t></w:r></w:fldSimple></w:r>
      <w:r>
        <w:drawing>
          <wp:inline distT="0" distB="0" distL="0" distR="0">
            <wp:extent cx="952500" cy="952500"/>
            <wp:docPr id="1" name="Picture 1"/>
            <wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr>
            <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
              <pic:pic>
                <pic:nvPicPr><pic:cNvPr id="1" name="img.png"/><pic:cNvPicPr/></pic:nvPicPr>
                <pic:blipFill><a:blip r:embed="rId9"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
                <pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="952500" cy="952500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>
              </pic:pic>
            </a:graphicData></a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
{ole_run}
    </w:p>
    <w:p>
      <w:pPr>
        <w:sectPr>
          <w:headerReference w:type="default" r:id="rId7"/>
          <w:footerReference w:type="default" r:id="rId8"/>
          <w:pgSz w:w="11906" w:h="16838"/>
          <w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="720" w:footer="720"/>
        </w:sectPr>
      </w:pPr>
    </w:p>
    <w:p>
      <w:r><w:t>Đoạn thuộc Section 2 cần giữ nguyên cấu trúc đa section.</w:t></w:r>
    </w:p>
    <w:sectPr>
      <w:headerReference w:type="default" r:id="rId7"/>
      <w:footerReference w:type="default" r:id="rId8"/>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="720" w:footer="720"/>
    </w:sectPr>
  </w:body>
</w:document>"#,
        ole_run = ole_run
    );
    pkg.set_bytes("word/document.xml", doc_xml.into_bytes(), CompressionMethod::Deflated);

    pkg.set_bytes(
        "word/styles.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="MyCustomCorpStyle"><w:name w:val="MyCustomCorpStyle"/><w:rPr><w:b/></w:rPr></w:style>
</w:styles>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes(
        "word/numbering.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="9"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum>
  <w:num w:numId="9"><w:abstractNumId w:val="9"/></w:num>
</w:numbering>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes(
        "word/settings.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes(
        "word/footnotes.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:footnote w:id="2"><w:p><w:r><w:t>Chú thích chân trang mẫu phải bảo toàn.</w:t></w:r></w:p></w:footnote>
</w:footnotes>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes(
        "word/endnotes.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:endnote w:id="3"><w:p><w:r><w:t>Chú thích cuối tài liệu phải bảo toàn.</w:t></w:r></w:p></w:endnote>
</w:endnotes>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes(
        "word/comments.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:comment w:id="1" w:author="Reviewer"><w:p><w:r><w:t>Bình luận thẩm định cần bảo toàn.</w:t></w:r></w:p></w:comment>
</w:comments>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes(
        "word/header1.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:t>ĐẦU TRANG GỐC CƠ QUAN</w:t></w:r></w:p>
</w:hdr>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes(
        "word/footer1.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:t>Tài liệu nội bộ cần bảo toàn footer</w:t></w:r></w:p>
</w:ftr>"#
            .as_bytes()
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.set_bytes("word/media/image1.png", minimal_png_bytes(), CompressionMethod::Stored);
    if include_macro_and_ole_binaries {
        pkg.set_bytes(
            "word/embeddings/oleObject1.bin",
            b"FAKE_OLE_COMPOUND_BINARY_PAYLOAD_0123456789".to_vec(),
            CompressionMethod::Deflated,
        );
        pkg.set_bytes(
            "word/vbaProject.bin",
            b"FAKE_VBA_PROJECT_MACRO_PAYLOAD_SECURE_KEEP".to_vec(),
            CompressionMethod::Deflated,
        );
        pkg.set_bytes(
            "word/vbaData.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><wne:vbaSuppData xmlns:wne="http://schemas.microsoft.com/office/word/2006/wordml"/>"#
                .to_vec(),
            CompressionMethod::Deflated,
        );
    }
    pkg.set_bytes(
        "customXml/item1.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><corpMeta><codeId>MONWORD-2026</codeId></corpMeta>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );
    pkg.set_bytes(
        "docProps/custom.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"/>"#
            .to_vec(),
        CompressionMethod::Deflated,
    );

    pkg.save_to(path).unwrap();
}

#[test]
fn rel_case_01_safe_save_atomic_swap_and_no_premature_delete() {
    let dir = make_temp_rel_dir("c01_safe_save");
    let target = dir.join("important.docx");
    build_simple_docx(&target, "NOI_DUNG_QUAN_TRONG_BAN_GOC");
    let orig_bytes = fs::read(&target).unwrap();
    let orig_sha = sha256_hex(&orig_bytes);

    let mut updated_pkg = OpenXmlPackage::open(&target).unwrap();
    updated_pkg.set_xml(
        "word/document.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>NOI_DUNG_MOI_HOP_LE</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"#.to_string(),
    );

    // 1a. Simulated temp validation failure BEFORE touching destination
    let res_tmp_fail = with_fault_mode_for_test(
        FaultMode::FailTempValidation("Lỗi kiểm định tệp tạm mô phỏng".to_string()),
        || save_openxml_verified(&updated_pkg, &target, None, &PreservationPolicy::default()),
    );
    assert!(res_tmp_fail.is_err());
    assert!(res_tmp_fail.unwrap_err().contains("ERR_POST_SAVE_VERIFY"));
    assert_eq!(sha256_hex(&fs::read(&target).unwrap()), orig_sha);

    // 1b. Simulated commit rename failure AFTER staging original -> must auto-rollback original
    let res_rename_fail = with_fault_mode_for_test(FaultMode::FailRenameNewToTarget, || {
        save_openxml_verified(&updated_pkg, &target, None, &PreservationPolicy::default())
    });
    assert!(res_rename_fail.is_err());
    assert!(target.is_file(), "Tệp gốc KHÔNG được bị xóa khi đổi tên thất bại");
    assert_eq!(sha256_hex(&fs::read(&target).unwrap()), orig_sha);

    // 1c. Malformed XML in package must be rejected BEFORE touching existing destination
    let mut bad_xml_pkg = OpenXmlPackage::open(&target).unwrap();
    bad_xml_pkg.set_xml(
        "word/document.xml",
        "<w:document><w:body><w:p><w:unclosed>".to_string(),
    );
    let res_bad_xml =
        save_openxml_verified(&bad_xml_pkg, &target, None, &PreservationPolicy::default());
    assert!(res_bad_xml.is_err());
    assert_eq!(sha256_hex(&fs::read(&target).unwrap()), orig_sha);

    // 1d. Normal verified save replaces cleanly
    let saved_len =
        save_openxml_verified(&updated_pkg, &target, None, &PreservationPolicy::default()).unwrap();
    assert!(saved_len > 0);
    let reopened = OpenXmlPackage::open(&target).unwrap();
    assert!(reopened
        .get_xml("word/document.xml")
        .unwrap()
        .contains("NOI_DUNG_MOI_HOP_LE"));

    // Verify no stale tmp/prev files left behind
    let stale: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.contains(".mwtmp.") || n.contains(".mwprev.")
        })
        .collect();
    assert!(stale.is_empty(), "Unexpected stale swap files: {:?}", stale);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rel_case_02_locked_destination_conflict_and_power_loss_recovery() {
    let dir = make_temp_rel_dir("c02_lock_recovery");

    // 2a. Locked destination file (Windows exclusive sharing mode 0 simulating Word lock)
    let locked_doc = dir.join("dang_mo_word.docx");
    build_simple_docx(&locked_doc, "Van ban dang mo khoa doc quyen");
    let orig_locked_bytes = fs::read(&locked_doc).unwrap();
    {
        let _exclusive_handle = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&locked_doc)
            .unwrap();

        let mut cfg = ProcessConfig::from_preset(PresetId::Nd30);
        cfg.output_mode = OutputMode::OverwriteWithBackup;
        let res = engine::process_single_file(&locked_doc, &cfg);
        assert!(!res.success);
        assert_eq!(
            res.error_code.as_deref(),
            Some("ERR_FILE_LOCKED_OR_CONFLICT"),
            "Err: {:?}",
            res.error
        );
        assert!(res.error.unwrap_or_default().contains("Hướng xử lý:"));
    }
    assert_eq!(
        fs::read(&locked_doc).unwrap(),
        orig_locked_bytes,
        "Tệp bị khóa vẫn phải nguyên vẹn 100% sau khi thất bại"
    );

    // 2b. Power-loss / mid-swap crash simulation & automatic recovery
    let crash_doc = dir.join("mat_dien_giua_chung.docx");
    build_simple_docx(&crash_doc, "DU_LIEU_TRUOC_KHI_MAT_DIEN");
    let orig_crash_bytes = fs::read(&crash_doc).unwrap();

    let mut new_pkg = OpenXmlPackage::open(&crash_doc).unwrap();
    new_pkg.set_xml(
        "word/document.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>DU_LIEU_MOI</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"#.to_string(),
    );

    let sim_res = with_fault_mode_for_test(FaultMode::AbortProcessMidSwap, || {
        save_openxml_verified(&new_pkg, &crash_doc, None, &PreservationPolicy::default())
    });
    assert!(sim_res.is_err());
    assert!(
        !crash_doc.exists(),
        "Tại thời điểm mất điện giữa swap, target đang chuyển sang .mwprev"
    );

    // Automatic recovery recovers original document from journal + staged .mwprev
    let notes = recover_interrupted_saves_in_dir(&dir);
    assert!(!notes.is_empty(), "Phải ghi nhận thông báo khôi phục giao dịch");
    assert!(crash_doc.is_file(), "Tệp gốc phải được khôi phục lại");
    assert_eq!(fs::read(&crash_doc).unwrap(), orig_crash_bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rel_case_03_backup_uniqueness_versioning_retention_and_corrupt_detection() {
    let dir = make_temp_rel_dir("c03_backup");
    let target = dir.join("baocao_ver.docx");
    build_simple_docx(&target, "NOI_DUNG_BAN_GOC_V1");
    let v1_bytes = fs::read(&target).unwrap();

    let policy = RetentionPolicy {
        max_pre_edit_versions: 15,
    };

    // Rapid burst of 20 backups within milliseconds -> 0 collisions, 0 overwrites
    let mut created_paths = HashSet::new();
    for _ in 0..20 {
        let rec = create_versioned_backup_with_policy(&target, "burst_test", policy).unwrap();
        assert!(created_paths.insert(rec.backup_path.clone()));
    }
    assert_eq!(created_paths.len(), 20);

    let listed = list_backups_for_file(&target);
    assert_eq!(
        listed.len(),
        16,
        "Chính sách retention phải giữ 1 InitialBaseline + 15 PreEditVersion = 16"
    );
    let init_rec = listed
        .iter()
        .find(|r| r.kind == BackupKind::InitialBaseline)
        .expect("InitialBaseline v1 không bao giờ được bị retention xóa");
    assert_eq!(init_rec.version_number, 1);
    assert!(init_rec.integrity_ok);
    assert_eq!(fs::read(&init_rec.backup_path).unwrap(), v1_bytes);

    // Edit file to v2, create pre-edit backup, then edit file to v3
    build_simple_docx(&target, "NOI_DUNG_PHIEN_BAN_V2");
    let v2_bytes = fs::read(&target).unwrap();
    let v2_rec = create_versioned_backup_with_policy(&target, "edit_v2", policy).unwrap();

    build_simple_docx(&target, "NOI_DUNG_PHIEN_BAN_V3");

    // Restore latest pre-edit -> gets v2 back
    restore_latest_pre_edit(&target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), v2_bytes);

    // Restore initial baseline -> gets v1 back
    restore_initial_baseline(&target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), v1_bytes);

    // Corrupt a backup file -> restore_backup_checked MUST refuse and protect current target
    let corrupt_bak = PathBuf::from(&v2_rec.backup_path);
    fs::write(&corrupt_bak, b"CORRUPTED_TRUNCATED_ZIP_BYTES").unwrap();
    let err_res = restore_backup_checked(&target, &corrupt_bak);
    assert!(err_res.is_err());
    let err_msg = err_res.unwrap_err();
    assert!(
        err_msg.contains("ERR_BACKUP_INVALID"),
        "Unexpected err: {}",
        err_msg
    );
    assert_eq!(
        fs::read(&target).unwrap(),
        v1_bytes,
        "Tệp đích không bị ghi đè bởi bản sao lưu hỏng"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rel_case_04_docx_and_docm_full_13_component_preservation_with_real_word_com() {
    let dir = make_temp_rel_dir("c04_preserve_13");
    let src_docm = dir.join("hopdong_macro.docm");
    build_rich_component_package(&src_docm, true);

    let orig_pkg = OpenXmlPackage::open(&src_docm).unwrap();
    let baseline = PreservationSnapshot::capture(&orig_pkg).unwrap();
    assert!(baseline.is_macro_content_type);
    assert_eq!(baseline.section_count, 2);
    assert_eq!(baseline.footnote_ref_count, 1);
    assert_eq!(baseline.endnote_ref_count, 1);
    assert_eq!(baseline.comment_ref_count, 1);
    assert_eq!(baseline.ole_object_count, 1);
    assert!(!baseline.media_hashes.is_empty());
    assert!(!baseline.embedding_hashes.is_empty());
    assert!(!baseline.macro_hashes.is_empty());

    // 4a. One-Click process (with clean_macros = false & page numbers enabled!)
    let mut cfg = ProcessConfig::from_preset(PresetId::Nd30);
    cfg.output_mode = OutputMode::NewCopy;
    cfg.security.clean_macros = false;
    cfg.page_toc.page_numbers_enabled = true;
    let res_1click = engine::process_single_file(&src_docm, &cfg);
    assert!(res_1click.success, "1-Click err: {:?}", res_1click.error);
    assert!(res_1click.output_path.ends_with(".docm"));

    let out_1click = OpenXmlPackage::open(Path::new(&res_1click.output_path)).unwrap();
    verify_openxml_preservation(&baseline, &out_1click, &PreservationPolicy::default())
        .expect("1-Click phải bảo toàn đủ 13 thành phần DOCM");
    // Also verify existing non-PAGE footer content was NOT unlinked or erased when top page number added
    let doc_xml_1c = out_1click.get_xml("word/document.xml").unwrap();
    assert!(
        doc_xml_1c.contains("w:footerReference"),
        "Không được xóa w:footerReference khi footer1.xml vẫn chứa chữ gốc"
    );

    // 4b. Header Builder
    let mut hb_cfg = HeaderBuilderConfig::default();
    hb_cfg.output_mode = OutputMode::NewCopy;
    hb_cfg.selected_candidate_index = Some(0);
    let org = OrganizationProfile {
        id: "rel_org".to_string(),
        profile_name: "Cơ quan Bảo toàn".to_string(),
        sector: OrgSector::StateAgency,
        issuing_org: "SỞ KHOA HỌC".to_string(),
        ..Default::default()
    };
    let res_hb = apply_header_builder_to_file(
        &src_docm,
        &hb_cfg,
        HeaderActionMode::StandardizeExisting,
        Some(&org),
    );
    assert!(res_hb.success, "Header Builder err: {:?}", res_hb.error);
    assert!(res_hb.output_path.ends_with(".docm"));
    let out_hb = OpenXmlPackage::open(Path::new(&res_hb.output_path)).unwrap();
    verify_openxml_preservation(&baseline, &out_hb, &PreservationPolicy::default())
        .expect("Header Builder phải bảo toàn đủ 13 thành phần DOCM");

    // 4c. Standardization Engine
    let res_std = std_engine::execute_standardization(&StdExecutionRequest {
        path: src_docm.to_string_lossy().to_string(),
        scope: StdDocScope::AdminNd30,
        org_profile: Some(org.clone()),
        output_mode: OutputMode::NewCopy,
        apply_all_group_a: true,
        ..Default::default()
    });
    assert!(res_std.success, "Std Engine err: {:?}", res_std.error);
    let out_std = OpenXmlPackage::open(Path::new(&res_std.output_path)).unwrap();
    verify_openxml_preservation(&baseline, &out_std, &PreservationPolicy::default())
        .expect("Std Engine phải bảo toàn đủ 13 thành phần DOCM");

    // 4d. Template Engine (ApplyToExistingDocx preserving .docm)
    let tpl_store = TemplateStore::default_store();
    let res_tpl = template_engine::apply_template(
        &tpl_store,
        &TemplateApplyRequest {
            template_id: "adm-cong-van".to_string(),
            org_profile_override: Some(org.clone()),
            target_mode: TemplateTargetMode::ApplyToExistingDocx,
            target_path: Some(src_docm.to_string_lossy().to_string()),
            output_mode: OutputMode::NewCopy,
            insert_framing_on_existing: true,
            ..Default::default()
        },
    );
    assert!(res_tpl.success, "Template Engine err: {:?}", res_tpl.error);
    let out_tpl = OpenXmlPackage::open(Path::new(&res_tpl.output_path)).unwrap();
    verify_openxml_preservation(&baseline, &out_tpl, &PreservationPolicy::default())
        .expect("Template Engine phải bảo toàn đủ 13 thành phần DOCM");

    // 4e. Layout Intelligence
    let res_lay = layout_intelligence::execute_proposals(
        &LayoutExecutionRequest {
            path: src_docm.to_string_lossy().to_string(),
            scope: Some(StdDocScope::AdminNd30),
            output_mode: OutputMode::NewCopy,
            apply_high_confidence_auto: true,
            accepted_proposal_ids: vec![],
            rejected_proposal_ids: vec![],
        },
        Some(&org),
    );
    assert!(res_lay.success, "Layout Engine err: {:?}", res_lay.error);
    let out_lay = OpenXmlPackage::open(Path::new(&res_lay.output_path)).unwrap();
    verify_openxml_preservation(&baseline, &out_lay, &PreservationPolicy::default())
        .expect("Layout Intelligence phải bảo toàn đủ 13 thành phần DOCM");

    // 4f. Verify real Microsoft Word COM opens the full-component `.docx` output cleanly
    // (using valid Word-loadable OpenXML parts so Word renders all sections, footnotes, endnotes, comments, shapes & non-PAGE footer)
    let src_docx = dir.join("hopdong_clean.docx");
    build_rich_component_package(&src_docx, false);
    let docx_base = PreservationSnapshot::capture(&OpenXmlPackage::open(&src_docx).unwrap()).unwrap();
    let res_docx_1c = engine::process_single_file(&src_docx, &cfg);
    assert!(res_docx_1c.success, "DOCX 1-Click err: {:?}", res_docx_1c.error);
    let out_docx = OpenXmlPackage::open(Path::new(&res_docx_1c.output_path)).unwrap();
    verify_openxml_preservation(&docx_base, &out_docx, &PreservationPolicy::default()).unwrap();

    let ps_check = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = New-Object -ComObject Word.Application
$word.Visible = $false
$word.DisplayAlerts = 0
$word.AutomationSecurity = 3
try {{
  $doc = $word.Documents.Open('{p}', $false, $true)
  Write-Output ("WORD_DOCX_OPEN_OK|secs=" + $doc.Sections.Count + "|fn=" + $doc.Footnotes.Count + "|en=" + $doc.Endnotes.Count + "|cmt=" + $doc.Comments.Count)
  $doc.Close($false)
}} finally {{
  $word.Quit()
}}
"#,
        p = Path::new(&res_docx_1c.output_path).to_string_lossy().replace('\'', "''")
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &ps_check])
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("WORD_DOCX_OPEN_OK|secs=2|fn=1|en=1|cmt=1"),
        "Word COM failed: status={:?}, stdout={}, stderr={}",
        out.status,
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rel_case_05_resource_protection_oversize_zip_bomb_and_xml_bomb() {
    let dir = make_temp_rel_dir("c05_resources");
    let valid_doc = dir.join("small.docx");
    build_simple_docx(&valid_doc, "Noi dung ngan gon");

    // 5a. Compressed file size guard
    let small_limit = ResourceLimits {
        max_compressed_file_bytes: 512,
        ..ResourceLimits::default()
    };
    let size_err = with_custom_limits_for_test(small_limit, || OpenXmlPackage::open(&valid_doc));
    assert!(size_err.is_err());
    assert!(size_err.unwrap_err().contains("ERR_RESOURCE_LIMIT"));

    // 5b. Real high-ratio Zip Bomb fixture (20 MB zeros compressed to ~20 KB -> >900:1 ratio)
    let zip_bomb_path = dir.join("zip_bomb.docx");
    {
        let f = File::create(&zip_bomb_path).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zw.start_file("word/document.xml", opts).unwrap();
        let payload = vec![b' '; 20 * 1024 * 1024];
        zw.write_all(&payload).unwrap();
        zw.finish().unwrap();
    }
    let started = Instant::now();
    let bomb_res = OpenXmlPackage::open(&zip_bomb_path);
    assert!(bomb_res.is_err());
    let bomb_err = bomb_res.unwrap_err();
    assert!(bomb_err.contains("ERR_ZIP_BOMB"), "Err: {}", bomb_err);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "Zip bomb rejection took {:?}",
        started.elapsed()
    );

    // 5c. Deep XML nesting bomb (> 256 levels)
    let mut deep_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><root>"#);
    for _ in 0..300 {
        deep_xml.push_str("<w:n>");
    }
    for _ in 0..300 {
        deep_xml.push_str("</w:n>");
    }
    deep_xml.push_str("</root>");
    let depth_err = XmlDocument::parse(&deep_xml);
    assert!(depth_err.is_err());
    assert!(depth_err.unwrap_err().contains("ERR_RESOURCE_LIMIT"));

    // 5d. Excess XML element count guard
    let elem_limit = ResourceLimits {
        max_xml_elements: 25,
        ..ResourceLimits::default()
    };
    let mut many_xml = String::from(r#"<w:body>"#);
    for _ in 0..40 {
        many_xml.push_str("<w:p/>");
    }
    many_xml.push_str("</w:body>");
    let elem_err = with_custom_limits_for_test(elem_limit, || XmlDocument::parse(&many_xml));
    assert!(elem_err.is_err());
    assert!(elem_err.unwrap_err().contains("ERR_RESOURCE_LIMIT"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rel_case_06_word_com_timeout_process_cleanup_and_task_cancellation() {
    // 6a. Process timeout kills sleeping worker well before 30s and returns ERR_COM_TIMEOUT
    let t0 = Instant::now();
    let timeout_res = system_office::run_hidden_powershell_with_timeout(
        "Start-Sleep -Seconds 30",
        550,
        None,
        None,
    );
    let elapsed = t0.elapsed();
    assert!(timeout_res.is_err());
    let msg = timeout_res.unwrap_err();
    assert!(msg.contains("ERR_COM_TIMEOUT"), "Msg: {}", msg);
    assert!(
        elapsed < Duration::from_secs(12),
        "Timed out took too long: {:?}",
        elapsed
    );

    // 6b. Cooperative per-task cancellation stops child worker cleanly without disturbing parallel tests
    let local_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_trigger = local_cancel.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(220));
        cancel_trigger.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let t1 = Instant::now();
    let cancel_probe = || local_cancel.load(std::sync::atomic::Ordering::SeqCst);
    let cancel_res = system_office::run_hidden_powershell_with_timeout(
        "Start-Sleep -Seconds 30",
        10_000,
        Some(&cancel_probe),
        None,
    );
    let _ = canceller.join();

    assert!(cancel_res.is_err());
    assert!(
        cancel_res.unwrap_err().contains("ERR_TASK_CANCELLED"),
        "Cancel elapsed {:?}",
        t1.elapsed()
    );
    assert!(t1.elapsed() < Duration::from_secs(12));

    // Also verify global BatchTaskCancel state transitions
    reset_batch_cancel();
    assert!(!is_batch_cancelled());
    assert!(request_batch_cancel());
    assert!(is_batch_cancelled());
    reset_batch_cancel();
    assert!(!is_batch_cancelled());
}

#[test]
fn rel_case_07_standardized_errors_and_no_false_positive_success() {
    let dir = make_temp_rel_dir("c07_errors");

    // 7a. Verify all error codes produce clear cause + remediation guidance
    let all_codes = [
        AppErrorCode::ErrFileRead,
        AppErrorCode::ErrFileWrite,
        AppErrorCode::ErrFileLockedOrConflict,
        AppErrorCode::ErrOpenXmlCorrupt,
        AppErrorCode::ErrXmlMalformed,
        AppErrorCode::ErrWordCom,
        AppErrorCode::ErrComTimeout,
        AppErrorCode::ErrIpcInvalid,
        AppErrorCode::ErrResourceLimit,
        AppErrorCode::ErrZipBomb,
        AppErrorCode::ErrRenderPreview,
        AppErrorCode::ErrDocConvert,
        AppErrorCode::ErrPostSaveVerify,
        AppErrorCode::ErrBackupInvalid,
        AppErrorCode::ErrTaskCancelled,
    ];
    for code in all_codes {
        let formatted = AppStructuredError::new(code, "Nguyên nhân kiểm thử").format_user_message();
        assert!(formatted.contains(code.as_str()), "Missing code in {}", formatted);
        assert!(formatted.contains("Hướng xử lý:"), "Missing remediation in {}", formatted);
    }
    let cls = classify_error_message("Sharing violation os error 32", AppErrorCode::ErrFileWrite);
    assert_eq!(cls.code, AppErrorCode::ErrFileLockedOrConflict);

    // 7b. Process missing file -> success = false + ERR_FILE_READ
    let missing = dir.join("khong_ton_tai.docx");
    let res_missing = engine::process_single_file(&missing, &ProcessConfig::from_preset(PresetId::Nd30));
    assert!(!res_missing.success);
    assert_eq!(res_missing.error_code.as_deref(), Some("ERR_FILE_READ"));

    // 7c. Critical step failure (`also_export_pdf = true` when PDF destination is blocked by a directory!)
    // MUST NOT report `success: true`!
    let src = dir.join("pdf_fail_test.docx");
    build_simple_docx(&src, "Văn bản kiểm tra không báo thành công giả khi xuất PDF lỗi.");
    let blocked_pdf_as_dir = dir.join("pdf_fail_test_chuanhoa.pdf");
    fs::create_dir_all(&blocked_pdf_as_dir).unwrap();

    let mut cfg_pdf = ProcessConfig::from_preset(PresetId::Nd30);
    cfg_pdf.output_mode = OutputMode::NewCopy;
    cfg_pdf.also_export_pdf = true;
    let res_pdf_fail = engine::process_single_file(&src, &cfg_pdf);
    assert!(
        !res_pdf_fail.success,
        "Không được báo thành công giả khi bước xuất PDF thất bại!"
    );
    assert!(matches!(
        res_pdf_fail.error_code.as_deref(),
        Some("ERR_DOC_CONVERT") | Some("ERR_WORD_COM") | Some("ERR_COM_TIMEOUT")
    ));

    // 7d. Audit export CSV to directory path (fails) -> success = false + error_code
    let rep = ComplianceAuditReport {
        audit_id: "a1".to_string(),
        checked_at: "2026-11-19".to_string(),
        legal_disclaimer: "test".to_string(),
        file_info: AuditFileDigest {
            file_name: "a.docx".to_string(),
            file_ext: "docx".to_string(),
            file_size_bytes: 100,
            doc_sha256: "abc".to_string(),
            detected_doc_type: "Công văn".to_string(),
            requested_scope: StdDocScope::AdminNd30,
            effective_scope: StdDocScope::AdminNd30,
            section_count: 1,
            estimated_page_count: 1,
            paragraph_count: 1,
            table_count: 0,
        },
        rule_profile: AuditRuleProfileUsed {
            pack_id: "p".to_string(),
            pack_version: "1".to_string(),
            effective_date: "2026".to_string(),
            org_profile_id: None,
            org_profile_name: None,
            mandatory_count: 1,
            conditional_count: 0,
            recommended_count: 0,
            not_applicable_count: 0,
            disabled_rule_ids: vec![],
        },
        matrix: vec![],
        traceable_findings: vec![],
        advanced_issues: vec![],
        repeated_clusters: vec![],
        consistency: DocumentConsistencyMetrics {
            font_uniformity_pct: 100.0,
            size_uniformity_pct: 100.0,
            spacing_uniformity_pct: 100.0,
            heading_hierarchy_valid: true,
            cross_component_consistent: true,
            technical_quality_index: 100,
        },
        recheck: None,
    };
    let exp_err = exporter::export_report(&rep, AuditExportFormat::Csv, &dir);
    assert!(!exp_err.success);
    assert!(exp_err.error_code.is_some());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rel_case_08_privacy_safe_structured_logging_no_sensitive_leak() {
    let dir = make_temp_rel_dir("c08_logging");
    let log_file = dir.join("monword_ops_test.jsonl");
    let src = dir.join("tai_lieu_mat.docx");
    let missing = dir.join("tep_bi_thieu.docx");

    let secret_canary_1 = "BI_MAT_TUYET_DOI_KHDN_778899";
    let secret_canary_2 = "NGUOI_KY_NGUYEN_VAN_MAT";
    build_simple_docx(&src, &format!("{} — Người ký: {}", secret_canary_1, secret_canary_2));

    with_test_log_file(&log_file, || {
        let mut cfg = ProcessConfig::from_preset(PresetId::Nd30);
        cfg.output_mode = OutputMode::NewCopy;
        let res = engine::process_single_file(&src, &cfg);
        assert!(res.success, "Err: {:?}", res.error);

        let fail_res = engine::process_single_file(&missing, &cfg);
        assert!(!fail_res.success);
    });

    assert!(log_file.is_file(), "Tệp JSONL log phải được tạo trên đĩa");
    let raw_log = fs::read_to_string(&log_file).unwrap();
    assert!(!raw_log.contains(secret_canary_1), "Log rò rỉ nội dung mật 1!");
    assert!(!raw_log.contains(secret_canary_2), "Log rò rỉ nội dung mật 2!");

    let entries = read_logs_from_file(&log_file, 50);
    assert!(entries.len() >= 3);
    assert!(entries.iter().any(|e| {
        e.task == "process_single_file"
            && e.file_name == "tai_lieu_mat.docx"
            && e.status == "ok"
            && !e.timestamp.is_empty()
            && !e.module.is_empty()
    }));
    assert!(entries.iter().any(|e| {
        e.task == "process_single_file"
            && e.file_name == "tep_bi_thieu.docx"
            && e.status == "error"
            && e.error_code.as_deref() == Some("ERR_FILE_READ")
    }));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rel_case_09_json_stores_atomic_persistence_and_regression_guard() {
    let dir = make_temp_rel_dir("c09_stores");

    // 9a. Organization Profile atomic save + preservation on simulated rename failure
    let org_path = dir.join("org_profiles.json");
    let list1 = org_profile::upsert_org_profile_at(
        &org_path,
        OrganizationProfile {
            id: "org-1".to_string(),
            issuing_org: "SỞ XÂY DỰNG".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(list1.len(), 1);

    let failed_update = with_fault_mode_for_test(FaultMode::FailRenameNewToTarget, || {
        org_profile::upsert_org_profile_at(
            &org_path,
            OrganizationProfile {
                id: "org-2".to_string(),
                issuing_org: "SỞ TÀI CHÍNH".to_string(),
                ..Default::default()
            },
        )
    });
    assert!(failed_update.is_err());
    let loaded = org_profile::load_org_profiles_from(&org_path);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].issuing_org, "SỞ XÂY DỰNG");

    // 9b. AuditHistoryStore & ComplianceRulePackStore atomic save
    let hist = AuditHistoryStore::with_root(&dir);
    assert!(hist.list_entries().unwrap().is_empty());

    let pack_store = ComplianceRulePackStore::with_root(&dir);
    let mut pack = ComplianceRulePackStore::builtin_pack();
    pack.pack_version = "2026.9-test".to_string();
    pack_store.save_pack_override(&pack).unwrap();
    assert_eq!(pack_store.load_active_pack().pack_version, "2026.9-test");

    let _ = fs::remove_dir_all(&dir);
}
