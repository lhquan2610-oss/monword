use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::xml_helpers::{XmlDocument, XmlElement, XmlNode};
use crate::models::{LockMode, SecurityConfig};

#[derive(Debug, Default, Clone)]
pub struct SecurityStats {
    pub macros_removed: usize,
    pub protection_unlocked: bool,
    pub protection_locked: bool,
    pub metadata_scrubbed: bool,
}

pub fn apply_security_and_macros(
    pkg: &mut OpenXmlPackage,
    cfg: &SecurityConfig,
    logs: &mut Vec<String>,
) -> Result<SecurityStats, String> {
    let mut stats = SecurityStats::default();

    // 1. Clean VBA Macros (.docm -> .docx / .xlsm -> .xlsx)
    if cfg.clean_macros {
        stats.macros_removed = strip_vba_macros(pkg, logs)?;
    }

    // 2. Unlock protection (Restrict Editing / Workbook & Sheet Protection)
    if cfg.unlock_protection {
        if unlock_package_protection(pkg)? {
            stats.protection_unlocked = true;
            logs.push("Đã mở khóa toàn bộ bảo vệ chỉnh sửa (Restrict Editing / Sheet Protection).".to_string());
        }
    }

    // 3. Apply lock mode on Word document
    if cfg.lock_mode != LockMode::None && pkg.is_word() {
        apply_word_lock(pkg, cfg.lock_mode)?;
        stats.protection_locked = true;
        logs.push(format!("Đã áp dụng chế độ bảo vệ tài liệu: {:?}.", cfg.lock_mode));
    }

    // 4. Scrub personal metadata / update author & title
    if cfg.scrub_personal_info || !cfg.author.trim().is_empty() || !cfg.title.trim().is_empty() {
        update_or_scrub_metadata(pkg, cfg)?;
        stats.metadata_scrubbed = true;
        logs.push(if cfg.scrub_personal_info {
            "Đã xóa sạch thông tin cá nhân ẩn trong Metadata (Core/App Properties).".to_string()
        } else {
            "Đã cập nhật Metadata Tác giả / Tiêu đề tệp.".to_string()
        });
    }

    Ok(stats)
}

fn strip_vba_macros(pkg: &mut OpenXmlPackage, logs: &mut Vec<String>) -> Result<usize, String> {
    let macro_paths: Vec<String> = pkg
        .entries
        .iter()
        .filter(|e| {
            let low = e.name.to_ascii_lowercase();
            low.contains("vbaproject") || low.contains("vbadata")
        })
        .map(|e| e.name.clone())
        .collect();

    let count = macro_paths.len();
    for path in &macro_paths {
        pkg.remove_entry(path);
    }

    // Update [Content_Types].xml
    if let Some(ct_xml) = pkg.get_xml("[Content_Types].xml") {
        if let Ok(mut doc) = XmlDocument::parse(&ct_xml) {
            for node in &mut doc.root.children {
                if let XmlNode::Element(el) = node {
                    if el.name == "Override" {
                        if let Some(ct) = el.get_attr("ContentType").map(|s| s.to_string()) {
                            if ct == "application/vnd.ms-word.document.macroEnabled.main+xml" {
                                el.set_attr(
                                    "ContentType",
                                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                                );
                            } else if ct == "application/vnd.ms-excel.sheet.macroEnabled.main+xml" {
                                el.set_attr(
                                    "ContentType",
                                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
                                );
                            }
                        }
                    }
                }
            }

            doc.root.children.retain(|n| {
                let XmlNode::Element(el) = n else {
                    return true;
                };
                let part = el.get_attr("PartName").unwrap_or("").to_ascii_lowercase();
                let ct = el.get_attr("ContentType").unwrap_or("").to_ascii_lowercase();
                let ext = el.get_attr("Extension").unwrap_or("").to_ascii_lowercase();
                !(part.contains("vbaproject")
                    || part.contains("vbadata")
                    || ct.contains("vbaproject")
                    || ct.contains("vbadata")
                    || ext == "bin")
            });

            if let Ok(updated) = doc.to_xml_string() {
                pkg.set_xml("[Content_Types].xml", updated);
            }
        }
    }

    // Clean relationships pointing to vbaProject / vbaData
    for rels_path in ["word/_rels/document.xml.rels", "xl/_rels/workbook.xml.rels"] {
        if let Some(rels_xml) = pkg.get_xml(rels_path) {
            if let Ok(mut doc) = XmlDocument::parse(&rels_xml) {
                doc.root.children.retain(|n| {
                    let XmlNode::Element(el) = n else {
                        return true;
                    };
                    let target = el.get_attr("Target").unwrap_or("").to_ascii_lowercase();
                    let rtype = el.get_attr("Type").unwrap_or("").to_ascii_lowercase();
                    !(target.contains("vbaproject")
                        || target.contains("vbadata")
                        || rtype.contains("vbaproject")
                        || rtype.contains("vbadata"))
                });
                if let Ok(updated) = doc.to_xml_string() {
                    pkg.set_xml(rels_path, updated);
                }
            }
        }
    }

    if count > 0 {
        logs.push(format!(
            "Office Macro Cleaner: đã gỡ bỏ {} thành phần VBA Macro & chuyển chuẩn XML sạch.",
            count
        ));
    } else {
        logs.push("Office Macro Cleaner: không phát hiện vbaProject.bin, đã chuẩn hóa ContentTypes sạch.".to_string());
    }

    Ok(count)
}

fn unlock_package_protection(pkg: &mut OpenXmlPackage) -> Result<bool, String> {
    let mut unlocked = false;

    if let Some(settings_xml) = pkg.get_xml("word/settings.xml") {
        let mut doc = XmlDocument::parse(&settings_xml)?;
        let before = doc.root.children.len();
        doc.root.remove_children_by_name("w:documentProtection");
        if doc.root.children.len() < before {
            unlocked = true;
            pkg.set_xml("word/settings.xml", doc.to_xml_string()?);
        }
    }

    if let Some(wb_xml) = pkg.get_xml("xl/workbook.xml") {
        let mut doc = XmlDocument::parse(&wb_xml)?;
        let before = doc.root.children.len();
        doc.root.remove_children_by_name("workbookProtection");
        if doc.root.children.len() < before {
            unlocked = true;
            pkg.set_xml("xl/workbook.xml", doc.to_xml_string()?);
        }
    }

    let sheet_names: Vec<String> = pkg
        .entries
        .iter()
        .filter(|e| {
            let l = e.name.to_ascii_lowercase();
            l.starts_with("xl/worksheets/sheet") && l.ends_with(".xml")
        })
        .map(|e| e.name.clone())
        .collect();

    for sheet_name in sheet_names {
        if let Some(xml) = pkg.get_xml(&sheet_name) {
            if let Ok(mut doc) = XmlDocument::parse(&xml) {
                let before = doc.root.children.len();
                doc.root.remove_children_by_name("sheetProtection");
                if doc.root.children.len() < before {
                    unlocked = true;
                    if let Ok(updated) = doc.to_xml_string() {
                        pkg.set_xml(&sheet_name, updated);
                    }
                }
            }
        }
    }

    Ok(unlocked)
}

fn apply_word_lock(pkg: &mut OpenXmlPackage, mode: LockMode) -> Result<(), String> {
    let edit_val = match mode {
        LockMode::None => return Ok(()),
        LockMode::ReadOnly => "readOnly",
        LockMode::CommentsOnly => "comments",
        LockMode::TrackedChanges => "trackedChanges",
    };

    let settings_xml = pkg.get_xml("word/settings.xml").unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"></w:settings>"#.to_string()
    });
    let mut doc = XmlDocument::parse(&settings_xml)?;
    doc.root.remove_children_by_name("w:documentProtection");

    let mut prot = XmlElement::new("w:documentProtection");
    prot.set_attr("w:edit", edit_val);
    prot.set_attr("w:enforcement", "1");
    doc.root.children.push(XmlNode::Element(prot));

    pkg.set_xml("word/settings.xml", doc.to_xml_string()?);
    Ok(())
}

fn update_or_scrub_metadata(pkg: &mut OpenXmlPackage, cfg: &SecurityConfig) -> Result<(), String> {
    if let Some(core_xml) = pkg.get_xml("docProps/core.xml") {
        if let Ok(mut doc) = XmlDocument::parse(&core_xml) {
            if cfg.scrub_personal_info {
                let creator = doc.root.ensure_child_first("dc:creator");
                creator.set_text(if cfg.author.trim().is_empty() {
                    ""
                } else {
                    cfg.author.trim()
                });
                let last_mod = doc.root.ensure_child_first("cp:lastModifiedBy");
                last_mod.set_text("");
                doc.root.remove_children_by_name("cp:lastPrinted");
            } else if !cfg.author.trim().is_empty() {
                let creator = doc.root.ensure_child_first("dc:creator");
                creator.set_text(cfg.author.trim());
            }

            if !cfg.title.trim().is_empty() {
                let title = doc.root.ensure_child_first("dc:title");
                title.set_text(cfg.title.trim());
            }

            pkg.set_xml("docProps/core.xml", doc.to_xml_string()?);
        }
    }

    if cfg.scrub_personal_info {
        if let Some(app_xml) = pkg.get_xml("docProps/app.xml") {
            if let Ok(mut doc) = XmlDocument::parse(&app_xml) {
                doc.root.remove_children_by_name("Company");
                doc.root.remove_children_by_name("Manager");
                if let Some(tt) = doc.root.child_mut("TotalTime") {
                    tt.set_text("0");
                }
                pkg.set_xml("docProps/app.xml", doc.to_xml_string()?);
            }
        }
    }

    Ok(())
}
