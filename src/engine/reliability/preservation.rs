use crate::engine::compliance_audit::sha256_util::sha256_hex;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::error_code::{AppErrorCode, AppStructuredError};
use crate::engine::xml_helpers::{XmlDocument, XmlElement, XmlNode};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub struct PreservationPolicy {
    pub allow_macro_strip: bool,
    pub allow_section_reset: bool,
    pub allow_hf_strip: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PreservationSnapshot {
    pub is_word: bool,
    pub is_macro_content_type: bool,
    pub media_hashes: BTreeMap<String, String>,
    pub embedding_hashes: BTreeMap<String, String>,
    pub macro_hashes: BTreeMap<String, String>,
    pub critical_xml_hashes: BTreeMap<String, String>,
    pub protected_extra_entries: BTreeSet<String>,
    pub header_footer_entries: BTreeSet<String>,
    pub style_ids: BTreeSet<String>,
    pub numbering_abstract_ids: BTreeSet<String>,
    pub numbering_ids: BTreeSet<String>,
    pub rel_entries: BTreeSet<String>,
    pub doc_rel_targets: BTreeSet<(String, String)>,
    pub section_count: usize,
    pub footnote_ref_count: usize,
    pub endnote_ref_count: usize,
    pub comment_ref_count: usize,
    pub ole_object_count: usize,
    pub protected_field_count: usize,
    pub hyperlink_count: usize,
}

impl PreservationSnapshot {
    pub fn capture(pkg: &OpenXmlPackage) -> Result<Self, String> {
        let mut snap = PreservationSnapshot {
            is_word: pkg.is_word(),
            ..Default::default()
        };

        if let Some(ct) = pkg.get_xml("[Content_Types].xml") {
            let low = ct.to_ascii_lowercase();
            snap.is_macro_content_type =
                low.contains("macroenabled") || low.contains("vbaproject");
        }

        for entry in &pkg.entries {
            let norm = normalize_part_name(&entry.name);
            let low = norm.to_ascii_lowercase();
            if low.ends_with(".rels") {
                snap.rel_entries.insert(low.clone());
            }
            if low.starts_with("word/media/") || low.starts_with("xl/media/") {
                snap.media_hashes.insert(low, sha256_hex(&entry.data));
            } else if low.starts_with("word/embeddings/") || low.starts_with("xl/embeddings/") {
                snap.embedding_hashes.insert(low, sha256_hex(&entry.data));
            } else if low.contains("vbaproject.bin") || low.contains("vbadata.xml") {
                snap.macro_hashes.insert(low, sha256_hex(&entry.data));
            } else if matches!(
                low.as_str(),
                "word/footnotes.xml"
                    | "word/endnotes.xml"
                    | "word/comments.xml"
                    | "word/commentsextended.xml"
                    | "word/commentsids.xml"
                    | "word/commentsextensible.xml"
            ) {
                snap.critical_xml_hashes.insert(low, sha256_hex(&entry.data));
            } else if low.starts_with("word/header") || low.starts_with("word/footer") {
                if low.ends_with(".xml") {
                    snap.header_footer_entries.insert(low);
                }
            } else if low.starts_with("customxml/")
                || low.starts_with("word/charts/")
                || low.starts_with("word/diagrams/")
                || low.starts_with("word/customxml/")
                || low == "docprops/custom.xml"
            {
                snap.protected_extra_entries.insert(low);
            }
        }

        if let Some(styles_xml) = pkg.get_xml("word/styles.xml") {
            if let Ok(doc) = XmlDocument::parse(&styles_xml) {
                for node in &doc.root.children {
                    if let XmlNode::Element(el) = node {
                        if el.name == "w:style" {
                            if let Some(sid) = el.get_attr("w:styleId") {
                                if !sid.trim().is_empty() {
                                    snap.style_ids.insert(sid.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(num_xml) = pkg.get_xml("word/numbering.xml") {
            if let Ok(doc) = XmlDocument::parse(&num_xml) {
                for node in &doc.root.children {
                    if let XmlNode::Element(el) = node {
                        if el.name == "w:abstractNum" {
                            if let Some(id) = el.get_attr("w:abstractNumId") {
                                snap.numbering_abstract_ids.insert(id.to_string());
                            }
                        } else if el.name == "w:num" {
                            if let Some(id) = el.get_attr("w:numId") {
                                snap.numbering_ids.insert(id.to_string());
                            }
                        }
                    }
                }
            }
        }

        if let Some(rels_xml) = pkg.get_xml("word/_rels/document.xml.rels") {
            if let Ok(doc) = XmlDocument::parse(&rels_xml) {
                snap.doc_rel_targets = collect_protected_rel_targets(&doc.root);
            }
        }

        if let Some(doc_xml) = pkg.get_xml("word/document.xml") {
            let doc = XmlDocument::parse(&doc_xml).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrXmlMalformed,
                    format!("word/document.xml nguồn không hợp lệ: {}", e),
                )
                .format_user_message()
            })?;
            let counters = count_doc_structural_elements(&doc.root);
            snap.section_count = counters.section_count;
            snap.footnote_ref_count = counters.footnote_ref_count;
            snap.endnote_ref_count = counters.endnote_ref_count;
            snap.comment_ref_count = counters.comment_ref_count;
            snap.ole_object_count = counters.ole_object_count;
            snap.protected_field_count = counters.protected_field_count;
            snap.hyperlink_count = counters.hyperlink_count;
        }

        Ok(snap)
    }
}

pub fn validate_all_package_xml_entries(pkg: &OpenXmlPackage) -> Result<(), String> {
    if !pkg.has_entry("[Content_Types].xml") {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            "Gói OpenXML thiếu phần bắt buộc [Content_Types].xml.",
        )
        .format_user_message());
    }
    if !pkg.has_entry("_rels/.rels") {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            "Gói OpenXML thiếu phần bắt buộc _rels/.rels.",
        )
        .format_user_message());
    }
    if !pkg.is_word() && !pkg.is_excel() {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            "Gói OpenXML thiếu word/document.xml hoặc xl/workbook.xml.",
        )
        .format_user_message());
    }

    for entry in &pkg.entries {
        let low = normalize_part_name(&entry.name).to_ascii_lowercase();
        if (low.ends_with(".xml") || low.ends_with(".rels")) && !low.starts_with("word/embeddings/") {
            let raw = if entry.data.starts_with(&[0xEF, 0xBB, 0xBF]) {
                &entry.data[3..]
            } else {
                entry.data.as_slice()
            };
            let xml_str = std::str::from_utf8(raw).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrXmlMalformed,
                    format!("Phần '{}' không phải UTF-8 hợp lệ: {}", entry.name, e),
                )
                .format_user_message()
            })?;
            XmlDocument::parse(xml_str).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrXmlMalformed,
                    format!("Phần '{}' lỗi cú pháp XML: {}", entry.name, e),
                )
                .format_user_message()
            })?;
        }
    }
    Ok(())
}

pub fn verify_openxml_preservation(
    before: &PreservationSnapshot,
    after_pkg: &OpenXmlPackage,
    policy: &PreservationPolicy,
) -> Result<(), String> {
    validate_all_package_xml_entries(after_pkg)?;

    let mut violations: Vec<String> = Vec::new();
    let after = PreservationSnapshot::capture(after_pkg)?;

    // 1. Media preservation (100% byte hash)
    for (k, h_before) in &before.media_hashes {
        match after.media_hashes.get(k) {
            Some(h_after) if h_after == h_before => {}
            Some(_) => violations.push(format!("Media '{}' bị biến đổi nội dung nhị phân", k)),
            None => violations.push(format!("Bị mất Media '{}'", k)),
        }
    }

    // 2. Embedded OLE objects (100% byte hash + document ref count)
    for (k, h_before) in &before.embedding_hashes {
        match after.embedding_hashes.get(k) {
            Some(h_after) if h_after == h_before => {}
            Some(_) => violations.push(format!("Embedded object '{}' bị thay đổi nhị phân", k)),
            None => violations.push(format!("Bị mất Embedded object '{}'", k)),
        }
    }
    if after.ole_object_count < before.ole_object_count {
        violations.push(format!(
            "Số đối tượng nhúng (OLE/Object) giảm từ {} xuống {}",
            before.ole_object_count, after.ole_object_count
        ));
    }

    // 3. VBA Macros (.docm / .xlsm) unless explicitly stripped
    if !policy.allow_macro_strip {
        for (k, h_before) in &before.macro_hashes {
            match after.macro_hashes.get(k) {
                Some(h_after) if h_after == h_before => {}
                Some(_) => violations.push(format!("Macro VBA '{}' bị thay đổi", k)),
                None => violations.push(format!("Bị mất Macro VBA '{}'", k)),
            }
        }
        if before.is_macro_content_type && !after.is_macro_content_type {
            violations.push(
                "[Content_Types].xml mất khai báo macroEnabled/vbaProject dù không bật gỡ Macro"
                    .to_string(),
            );
        }
    }

    // 4. Footnotes / Endnotes / Comments binary parts + body reference counts
    for (k, h_before) in &before.critical_xml_hashes {
        match after.critical_xml_hashes.get(k) {
            Some(h_after) if h_after == h_before => {}
            Some(_) => violations.push(format!("Thành phần chú thích/bình luận '{}' bị thay đổi", k)),
            None => violations.push(format!("Bị mất '{}'", k)),
        }
    }
    if after.footnote_ref_count < before.footnote_ref_count {
        violations.push(format!(
            "Số footnoteReference giảm từ {} xuống {}",
            before.footnote_ref_count, after.footnote_ref_count
        ));
    }
    if after.endnote_ref_count < before.endnote_ref_count {
        violations.push(format!(
            "Số endnoteReference giảm từ {} xuống {}",
            before.endnote_ref_count, after.endnote_ref_count
        ));
    }
    if after.comment_ref_count < before.comment_ref_count {
        violations.push(format!(
            "Số commentReference giảm từ {} xuống {}",
            before.comment_ref_count, after.comment_ref_count
        ));
    }

    // 5. Extra XML / CustomXML / Charts / Diagrams
    for k in &before.protected_extra_entries {
        if !after.protected_extra_entries.contains(k) {
            violations.push(format!("Bị mất thành phần mở rộng '{}'", k));
        }
    }

    // 6. Relationships preservation (.rels files & protected target links)
    for rel in &before.rel_entries {
        if !policy.allow_macro_strip && rel.contains("vbaproject") {
            if !after.rel_entries.contains(rel) {
                violations.push(format!("Bị mất quan hệ Macro '{}'", rel));
            }
        } else if !rel.contains("vbaproject") && !after.rel_entries.contains(rel) {
            violations.push(format!("Bị mất tệp quan hệ '{}'", rel));
        }
    }
    for (rel_type, target) in &before.doc_rel_targets {
        if policy.allow_macro_strip && target.to_ascii_lowercase().contains("vbaproject") {
            continue;
        }
        if !after
            .doc_rel_targets
            .contains(&(rel_type.clone(), target.clone()))
        {
            violations.push(format!("Mất liên kết .rels tới '{}'", target));
        }
    }

    // 7. Word-specific structure: Styles, Numbering, Sections, Header/Footer, Fields
    if before.is_word {
        if !before.style_ids.is_empty() && after.style_ids.is_empty() {
            violations.push("Mất word/styles.xml hoặc toàn bộ Style định nghĩa".to_string());
        } else {
            let missing_styles: Vec<&String> = before
                .style_ids
                .iter()
                .filter(|s| !after.style_ids.contains(*s))
                .collect();
            if !missing_styles.is_empty() {
                violations.push(format!(
                    "Mất {} style gốc (VD: {})",
                    missing_styles.len(),
                    missing_styles[0]
                ));
            }
        }

        if !before.numbering_abstract_ids.is_empty() || !before.numbering_ids.is_empty() {
            for id in &before.numbering_abstract_ids {
                if !after.numbering_abstract_ids.contains(id) {
                    violations.push(format!("Mất numbering abstractNumId={}", id));
                    break;
                }
            }
            for id in &before.numbering_ids {
                if !after.numbering_ids.contains(id) {
                    violations.push(format!("Mất numbering numId={}", id));
                    break;
                }
            }
        }

        if !policy.allow_section_reset && after.section_count < before.section_count {
            violations.push(format!(
                "Số lượng Section giảm từ {} xuống {} khi không yêu cầu xóa Section Break",
                before.section_count, after.section_count
            ));
        }

        if !policy.allow_hf_strip {
            for hf in &before.header_footer_entries {
                if !after.header_footer_entries.contains(hf) {
                    violations.push(format!("Bị mất phần đầu/chân trang '{}'", hf));
                }
            }
        }

        if after.protected_field_count < before.protected_field_count {
            violations.push(format!(
                "Số trường dữ liệu (Field REF/SEQ/MERGEFIELD/...) giảm từ {} xuống {}",
                before.protected_field_count, after.protected_field_count
            ));
        }
        if after.hyperlink_count < before.hyperlink_count {
            violations.push(format!(
                "Số Hyperlink giảm từ {} xuống {}",
                before.hyperlink_count, after.hyperlink_count
            ));
        }
    }

    if !violations.is_empty() {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrPostSaveVerify,
            format!(
                "Kiểm tra bảo toàn dữ liệu thất bại: {}.",
                violations.join("; ")
            ),
        )
        .format_user_message());
    }

    Ok(())
}

fn normalize_part_name(name: &str) -> String {
    name.trim_start_matches('/').replace('\\', "/")
}

fn collect_protected_rel_targets(rels_root: &XmlElement) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for node in &rels_root.children {
        if let XmlNode::Element(el) = node {
            if el.name == "Relationship" {
                let rel_type = el.get_attr("Type").unwrap_or("");
                let target = el.get_attr("Target").unwrap_or("");
                let low_t = target.to_ascii_lowercase();
                let low_r = rel_type.to_ascii_lowercase();
                let is_protected = low_r.contains("/image")
                    || low_r.contains("/oleobject")
                    || low_r.contains("/package")
                    || low_r.contains("/vbaproject")
                    || low_r.contains("/footnotes")
                    || low_r.contains("/endnotes")
                    || low_r.contains("/comments")
                    || low_r.contains("/styles")
                    || low_r.contains("/numbering")
                    || low_r.contains("/hyperlink")
                    || low_t.starts_with("media/")
                    || low_t.starts_with("embeddings/");
                if is_protected {
                    out.insert((low_r, target.to_string()));
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct DocStructuralCounters {
    section_count: usize,
    footnote_ref_count: usize,
    endnote_ref_count: usize,
    comment_ref_count: usize,
    ole_object_count: usize,
    protected_field_count: usize,
    hyperlink_count: usize,
}

fn count_doc_structural_elements(el: &XmlElement) -> DocStructuralCounters {
    let mut c = DocStructuralCounters::default();
    walk_doc_counters(el, &mut c);
    c
}

fn walk_doc_counters(el: &XmlElement, c: &mut DocStructuralCounters) {
    match el.name.as_str() {
        "w:sectPr" => c.section_count += 1,
        "w:footnoteReference" => c.footnote_ref_count += 1,
        "w:endnoteReference" => c.endnote_ref_count += 1,
        "w:commentReference" => c.comment_ref_count += 1,
        "w:object" | "o:OLEObject" => {
            c.ole_object_count += 1;
            return;
        }
        "w:hyperlink" => c.hyperlink_count += 1,
        "w:fldSimple" => {
            if let Some(instr) = el.get_attr("w:instr") {
                if is_protected_field_instr(instr) {
                    c.protected_field_count += 1;
                }
            }
        }
        "w:instrText" => {
            let instr = el.extract_text();
            if is_protected_field_instr(&instr) {
                c.protected_field_count += 1;
            }
        }
        _ => {}
    }
    for child in &el.children {
        if let XmlNode::Element(sub) = child {
            walk_doc_counters(sub, c);
        }
    }
}

fn is_protected_field_instr(raw: &str) -> bool {
    let upper = raw.trim().to_ascii_uppercase();
    let keyword = upper.split_whitespace().next().unwrap_or("");
    matches!(
        keyword,
        "REF" | "PAGEREF" | "MERGEFIELD" | "SEQ" | "HYPERLINK" | "DOCPROPERTY" | "FILENAME"
    )
}
