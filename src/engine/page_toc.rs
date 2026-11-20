use crate::engine::formatter::paper_dimensions_twips;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::xml_helpers::{
    cm_to_twips, pt_to_half_pts, set_run_font_and_size, XmlDocument, XmlElement, XmlNode,
    P_PR_ORDER, R_PR_ORDER, SECT_PR_ORDER,
};
use crate::models::{FormatConfig, OrientationMode, PageNumPosition, PageTocConfig};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

#[derive(Debug, Default, Clone)]
pub struct PageTocStats {
    pub section_count: usize,
    pub landscape_sections: usize,
    pub headings_detected: usize,
    pub page_numbers_applied: bool,
    pub toc_inserted: bool,
}

pub fn apply_page_and_toc(
    pkg: &mut OpenXmlPackage,
    cfg: &PageTocConfig,
    fmt: &FormatConfig,
    logs: &mut Vec<String>,
) -> Result<PageTocStats, String> {
    if !pkg.is_word() {
        return Ok(PageTocStats::default());
    }

    let mut stats = PageTocStats::default();
    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Không tìm thấy word/document.xml".to_string())?;
    let mut doc = XmlDocument::parse(&doc_xml)?;

    // 1. Handle Reset Portrait & Remove all Section Breaks
    if cfg.reset_portrait_and_remove_sections {
        if let Some(body) = doc.root.child_mut("w:body") {
            remove_inline_sect_prs(body);
            if let Some(sect_pr) = body.child_mut("w:sectPr") {
                set_sect_orientation(sect_pr, false, fmt, false);
            }
        }
        logs.push("Đã xóa toàn bộ Section Break và đưa tài liệu về khổ Dọc (Portrait).".to_string());
    }

    // 2. Apply Orientation Mode & Smart Binding Margin Swap
    if let Some(body) = doc.root.child_mut("w:body") {
        match cfg.orientation_mode {
            OrientationMode::Keep => {
                if cfg.auto_swap_binding_margin {
                    apply_margin_swap_existing_sections(body, fmt, &mut stats);
                }
            }
            OrientationMode::AllPortrait => {
                set_all_sections_orientation(body, false, fmt, cfg.auto_swap_binding_margin, &mut stats);
                logs.push("Đã chuyển toàn bộ tài liệu sang khổ Dọc (Portrait).".to_string());
            }
            OrientationMode::AllLandscape => {
                set_all_sections_orientation(body, true, fmt, cfg.auto_swap_binding_margin, &mut stats);
                logs.push(if cfg.auto_swap_binding_margin {
                    "Đã xoay toàn bộ trang Ngang và tự đảo lề đóng gáy 3 cm lên mép trên.".to_string()
                } else {
                    "Đã xoay toàn bộ trang Ngang.".to_string()
                });
            }
            OrientationMode::RotateRanges => {
                let target_pages = parse_page_ranges(&cfg.landscape_ranges);
                if !target_pages.is_empty() {
                    rotate_specific_pages(body, &target_pages, fmt, cfg.auto_swap_binding_margin, &mut stats);
                    logs.push(format!(
                        "Đã xoay ngang các trang [{}] ({} section ngang{}).",
                        cfg.landscape_ranges,
                        stats.landscape_sections,
                        if cfg.auto_swap_binding_margin {
                            ", tự đảo lề đóng gáy"
                        } else {
                            ""
                        }
                    ));
                }
            }
        }
        stats.section_count = count_sections_in_body(body);
    }

    // 3. Sanitize stray outlineLvl & Apply ND30 Headings & Auto-detect Headings
    if cfg.apply_nd30_headings || cfg.auto_detect_headings || cfg.insert_toc {
        if let Some(body) = doc.root.child_mut("w:body") {
            sanitize_stray_outline_levels(body);
        }
    }

    if cfg.apply_nd30_headings {
        ensure_nd30_heading_styles(pkg, fmt)?;
        logs.push("Đã cập nhật bộ Style Heading 1–4 chuẩn Nghị định 30/2020 (màu đen 000000).".to_string());
    }

    if cfg.auto_detect_headings {
        if let Some(body) = doc.root.child_mut("w:body") {
            let detected = auto_detect_headings_in_body(body);
            stats.headings_detected = detected;
            logs.push(format!("Tự động nhận diện và gắn Style cho {} tiêu đề Heading.", detected));
        }
    }

    // 4. Insert TOC if requested
    if cfg.insert_toc {
        ensure_nd30_heading_styles(pkg, fmt)?;
        if let Some(body) = doc.root.child_mut("w:body") {
            insert_toc_block(body, cfg.toc_levels.clamp(1, 6), cfg.toc_page_break_after, fmt);
            stats.toc_inserted = true;
            logs.push(format!(
                "Đã chèn Mục lục tự động (cấp 1–{}).",
                cfg.toc_levels.clamp(1, 6)
            ));
        }
        enable_update_fields_on_open(pkg)?;
    }

    // 5. Remove existing page numbers or apply page numbers / headers / footers
    if cfg.remove_existing_page_numbers {
        if let Some(body) = doc.root.child_mut("w:body") {
            strip_header_footer_refs(body);
        }
        logs.push("Đã gỡ bỏ liên kết số trang / Header / Footer khỏi các Section.".to_string());
    }

    if cfg.page_numbers_enabled || !cfg.header_text.trim().is_empty() || !cfg.footer_text.trim().is_empty() {
        apply_headers_footers_and_page_numbers(pkg, &mut doc, cfg, fmt)?;
        stats.page_numbers_applied = cfg.page_numbers_enabled;
        logs.push(format!(
            "Đã thiết lập số trang ({:?}, cỡ {}pt{}).",
            cfg.page_num_position,
            cfg.page_num_font_size_pt,
            if cfg.skip_first_page {
                ", bỏ qua trang đầu theo NĐ30"
            } else {
                ""
            }
        ));
    }

    pkg.set_xml("word/document.xml", doc.to_xml_string()?);
    Ok(stats)
}

pub fn parse_page_ranges(raw: &str) -> HashSet<usize> {
    let mut pages = HashSet::new();
    for part in raw.split([',', ';']) {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((start_s, end_s)) = trimmed.split_once('-') {
            if let (Ok(s), Ok(e)) = (start_s.trim().parse::<usize>(), end_s.trim().parse::<usize>()) {
                let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
                for p in lo..=hi.min(lo + 500) {
                    if p >= 1 {
                        pages.insert(p);
                    }
                }
            }
        } else if let Ok(p) = trimmed.parse::<usize>() {
            if p >= 1 {
                pages.insert(p);
            }
        }
    }
    pages
}

fn remove_inline_sect_prs(body: &mut XmlElement) {
    for node in &mut body.children {
        if let XmlNode::Element(p) = node {
            if p.name == "w:p" {
                if let Some(p_pr) = p.child_mut("w:pPr") {
                    p_pr.remove_children_by_name("w:sectPr");
                }
            }
        }
    }
}

fn set_sect_orientation(
    sect_pr: &mut XmlElement,
    landscape: bool,
    fmt: &FormatConfig,
    swap_binding_margin: bool,
) {
    let (pw, ph) = paper_dimensions_twips(fmt.paper_size);
    let pg_sz = sect_pr.ensure_child_ordered("w:pgSz", SECT_PR_ORDER);
    if landscape {
        pg_sz.set_attr("w:w", &ph.to_string());
        pg_sz.set_attr("w:h", &pw.to_string());
        pg_sz.set_attr("w:orient", "landscape");
    } else {
        pg_sz.set_attr("w:w", &pw.to_string());
        pg_sz.set_attr("w:h", &ph.to_string());
        pg_sz.remove_attr("w:orient");
    }

    let pg_mar = sect_pr.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
    let top_tw = cm_to_twips(fmt.margin_top_cm);
    let bot_tw = cm_to_twips(fmt.margin_bottom_cm);
    let left_tw = cm_to_twips(fmt.margin_left_cm);
    let right_tw = cm_to_twips(fmt.margin_right_cm);

    if landscape && swap_binding_margin {
        // Rotate binding edge (left 3cm) onto the top edge of landscape sheet
        pg_mar.set_attr("w:top", &left_tw.to_string());
        pg_mar.set_attr("w:bottom", &right_tw.to_string());
        pg_mar.set_attr("w:left", &bot_tw.to_string());
        pg_mar.set_attr("w:right", &top_tw.to_string());
    } else {
        pg_mar.set_attr("w:top", &top_tw.to_string());
        pg_mar.set_attr("w:bottom", &bot_tw.to_string());
        pg_mar.set_attr("w:left", &left_tw.to_string());
        pg_mar.set_attr("w:right", &right_tw.to_string());
    }
}

fn apply_margin_swap_existing_sections(
    body: &mut XmlElement,
    fmt: &FormatConfig,
    stats: &mut PageTocStats,
) {
    for node in &mut body.children {
        if let XmlNode::Element(el) = node {
            if el.name == "w:p" {
                if let Some(sect_pr) = el.child_mut("w:pPr").and_then(|pp| pp.child_mut("w:sectPr")) {
                    let is_land = sect_pr
                        .child("w:pgSz")
                        .and_then(|s| s.get_attr("w:orient"))
                        .map(|o| o == "landscape")
                        .unwrap_or(false);
                    if is_land {
                        stats.landscape_sections += 1;
                    }
                    set_sect_orientation(sect_pr, is_land, fmt, true);
                }
            } else if el.name == "w:sectPr" {
                let is_land = el
                    .child("w:pgSz")
                    .and_then(|s| s.get_attr("w:orient"))
                    .map(|o| o == "landscape")
                    .unwrap_or(false);
                if is_land {
                    stats.landscape_sections += 1;
                }
                set_sect_orientation(el, is_land, fmt, true);
            }
        }
    }
}

fn set_all_sections_orientation(
    body: &mut XmlElement,
    landscape: bool,
    fmt: &FormatConfig,
    swap_binding_margin: bool,
    stats: &mut PageTocStats,
) {
    for node in &mut body.children {
        if let XmlNode::Element(el) = node {
            if el.name == "w:p" {
                if let Some(sect_pr) = el.child_mut("w:pPr").and_then(|pp| pp.child_mut("w:sectPr")) {
                    set_sect_orientation(sect_pr, landscape, fmt, swap_binding_margin);
                    if landscape {
                        stats.landscape_sections += 1;
                    }
                }
            } else if el.name == "w:sectPr" {
                set_sect_orientation(el, landscape, fmt, swap_binding_margin);
                if landscape {
                    stats.landscape_sections += 1;
                }
            }
        }
    }
}

fn rotate_specific_pages(
    body: &mut XmlElement,
    target_pages: &HashSet<usize>,
    fmt: &FormatConfig,
    swap_binding_margin: bool,
    stats: &mut PageTocStats,
) {
    // Strip existing inline section breaks first for deterministic page-range sectioning
    remove_inline_sect_prs(body);
    let base_sect = body
        .child("w:sectPr")
        .cloned()
        .unwrap_or_else(|| XmlElement::new("w:sectPr"));

    let mut current_page = 1usize;
    let mut page_starts: Vec<(usize, usize)> = vec![(0, 1)]; // (body_child_idx, page_num)

    for (idx, node) in body.children.iter().enumerate() {
        let XmlNode::Element(el) = node else {
            continue;
        };
        if el.name == "w:sectPr" {
            continue;
        }
        let breaks_in_el = count_page_breaks_in_node(el);
        if breaks_in_el > 0 {
            current_page += breaks_in_el;
            if idx + 1 < body.children.len() {
                page_starts.push((idx + 1, current_page));
            }
        }
    }

    // Track which body child indices end a page whose orientation differs from the next page
    let total_body_items = body
        .children
        .iter()
        .position(|n| matches!(n, XmlNode::Element(e) if e.name == "w:sectPr"))
        .unwrap_or(body.children.len());

    if total_body_items == 0 {
        return;
    }

    let mut boundary_orientations: Vec<(usize, bool)> = Vec::new();
    for win in page_starts.windows(2) {
        let (_, page_a) = win[0];
        let (start_b, page_b) = win[1];
        let land_a = target_pages.contains(&page_a);
        let land_b = target_pages.contains(&page_b);
        if land_a != land_b && start_b > 0 {
            boundary_orientations.push((start_b - 1, land_a));
        }
    }

    let last_page_land = page_starts
        .last()
        .map(|(_, p)| target_pages.contains(p))
        .unwrap_or(false);

    // Apply inline sectPr at boundary paragraphs (insert clean separator paragraph if boundary is a table)
    // Iterate in reverse so inserting a paragraph does not shift earlier indices
    for (idx, is_land) in boundary_orientations.into_iter().rev() {
        let mut sect = base_sect.clone();
        set_sect_orientation(&mut sect, is_land, fmt, swap_binding_margin);
        if is_land {
            stats.landscape_sections += 1;
        }

        let is_para = matches!(
            body.children.get(idx),
            Some(XmlNode::Element(e)) if e.name == "w:p"
        );
        if is_para {
            if let Some(XmlNode::Element(p)) = body.children.get_mut(idx) {
                let p_pr = p.ensure_child_first("w:pPr");
                p_pr.remove_children_by_name("w:sectPr");
                p_pr.insert_child_ordered(sect, P_PR_ORDER);
            }
        } else {
            let mut sep_p = XmlElement::new("w:p");
            let mut p_pr = XmlElement::new("w:pPr");
            p_pr.insert_child_ordered(sect, P_PR_ORDER);
            sep_p.children.push(XmlNode::Element(p_pr));
            body.children.insert(idx + 1, XmlNode::Element(sep_p));
        }
    }

    // Set final body sectPr orientation
    if let Some(final_sect) = body.child_mut("w:sectPr") {
        set_sect_orientation(final_sect, last_page_land, fmt, swap_binding_margin);
        if last_page_land {
            stats.landscape_sections += 1;
        }
    }
}

fn count_page_breaks_in_node(el: &XmlElement) -> usize {
    let mut count = 0usize;
    if el.name == "w:br" && el.get_attr("w:type") == Some("page") {
        count += 1;
    } else if el.name == "w:lastRenderedPageBreak" {
        count += 1;
    } else if el.name == "w:pPr" && el.child("w:pageBreakBefore").is_some() {
        count += 1;
    }
    for child in &el.children {
        if let XmlNode::Element(sub) = child {
            count += count_page_breaks_in_node(sub);
        }
    }
    count
}

fn count_sections_in_body(body: &XmlElement) -> usize {
    let mut count = 0usize;
    for node in &body.children {
        if let XmlNode::Element(el) = node {
            if el.name == "w:sectPr" {
                count += 1;
            } else if el.name == "w:p" {
                if el
                    .child("w:pPr")
                    .and_then(|pp| pp.child("w:sectPr"))
                    .is_some()
                {
                    count += 1;
                }
            }
        }
    }
    count.max(1)
}

fn strip_header_footer_refs(body: &mut XmlElement) {
    for node in &mut body.children {
        if let XmlNode::Element(el) = node {
            if el.name == "w:sectPr" {
                el.remove_children_by_name("w:headerReference");
                el.remove_children_by_name("w:footerReference");
            } else if el.name == "w:p" {
                if let Some(sect_pr) = el.child_mut("w:pPr").and_then(|pp| pp.child_mut("w:sectPr")) {
                    sect_pr.remove_children_by_name("w:headerReference");
                    sect_pr.remove_children_by_name("w:footerReference");
                }
            }
        }
    }
}

fn apply_headers_footers_and_page_numbers(
    pkg: &mut OpenXmlPackage,
    doc: &mut XmlDocument,
    cfg: &PageTocConfig,
    fmt: &FormatConfig,
) -> Result<(), String> {
    let is_top = matches!(
        cfg.page_num_position,
        PageNumPosition::TopCenter | PageNumPosition::TopRight
    );
    let odd_jc = match cfg.page_num_position {
        PageNumPosition::TopCenter | PageNumPosition::BottomCenter => "center",
        PageNumPosition::TopRight | PageNumPosition::BottomRight => "right",
    };
    let even_jc = if cfg.mirror_odd_even {
        match cfg.page_num_position {
            PageNumPosition::TopRight | PageNumPosition::BottomRight => "left",
            _ => "center",
        }
    } else {
        odd_jc
    };

    let mut hdr_rid_default: Option<String> = None;
    let mut hdr_rid_even: Option<String> = None;
    let mut ftr_rid_default: Option<String> = None;
    let mut ftr_rid_even: Option<String> = None;

    let want_header = (cfg.page_numbers_enabled && is_top) || !cfg.header_text.trim().is_empty();
    let want_footer = (cfg.page_numbers_enabled && !is_top) || !cfg.footer_text.trim().is_empty();

    // Map existing doc rel IDs -> normalized part paths (e.g., "rId4" -> "word/footer1.xml")
    let existing_rels = collect_doc_rel_map(pkg);

    // Strip old PAGE fields from opposite-edge legacy parts BEFORE updating section references,
    // and track which opposite-edge parts still hold meaningful content (text, images, tables, OLE).
    let mut non_empty_hf_parts: HashSet<String> = HashSet::new();
    if cfg.page_numbers_enabled {
        let strip_prefix = if is_top && !want_footer {
            Some("word/footer")
        } else if !is_top && !want_header {
            Some("word/header")
        } else {
            None
        };
        if let Some(prefix) = strip_prefix {
            let targets: Vec<String> = pkg
                .entries
                .iter()
                .filter(|e| {
                    let l = e.name.to_ascii_lowercase();
                    l.starts_with(prefix) && l.ends_with(".xml") && !l.contains("chuanhoa")
                })
                .map(|e| e.name.clone())
                .collect();
            for part in targets {
                if let Some(xml) = pkg.get_xml(&part) {
                    if let Ok(mut hf_doc) = XmlDocument::parse(&xml) {
                        strip_page_fields_from_hf(&mut hf_doc.root);
                        if hf_has_meaningful_content(&hf_doc.root) {
                            non_empty_hf_parts.insert(part.to_ascii_lowercase());
                        }
                        if let Ok(updated) = hf_doc.to_xml_string() {
                            pkg.set_xml(&part, updated);
                        }
                    }
                }
            }
        }
    }

    // Check if any existing target-edge default header/footer already has meaningful non-PAGE content
    // when the user did NOT request replacing it with custom text.
    let mut preserved_header_updated = false;
    let mut preserved_footer_updated = false;

    if want_header && cfg.header_text.trim().is_empty() {
        if let Some(part) = find_existing_default_hf_part(doc, &existing_rels, "w:headerReference") {
            if let Some(xml) = pkg.get_xml(&part) {
                if let Ok(mut hf_doc) = XmlDocument::parse(&xml) {
                    strip_page_fields_from_hf(&mut hf_doc.root);
                    if hf_has_meaningful_content(&hf_doc.root) {
                        append_page_num_to_hf_doc(&mut hf_doc, odd_jc, &fmt.font_name, cfg.page_num_font_size_pt)?;
                        pkg.set_xml(&part, hf_doc.to_xml_string()?);
                        preserved_header_updated = true;
                    }
                }
            }
        }
    }

    if want_footer && cfg.footer_text.trim().is_empty() {
        if let Some(part) = find_existing_default_hf_part(doc, &existing_rels, "w:footerReference") {
            if let Some(xml) = pkg.get_xml(&part) {
                if let Ok(mut hf_doc) = XmlDocument::parse(&xml) {
                    strip_page_fields_from_hf(&mut hf_doc.root);
                    if hf_has_meaningful_content(&hf_doc.root) {
                        append_page_num_to_hf_doc(&mut hf_doc, odd_jc, &fmt.font_name, cfg.page_num_font_size_pt)?;
                        pkg.set_xml(&part, hf_doc.to_xml_string()?);
                        preserved_footer_updated = true;
                    }
                }
            }
        }
    }

    if want_header && !preserved_header_updated {
        let xml = build_header_footer_xml(
            "w:hdr",
            cfg.page_numbers_enabled && is_top,
            odd_jc,
            &cfg.header_text,
            &fmt.font_name,
            cfg.page_num_font_size_pt,
        );
        pkg.set_xml("word/header_chuanhoa.xml", xml);
        ensure_content_type_override(
            pkg,
            "/word/header_chuanhoa.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
        )?;
        hdr_rid_default = Some(ensure_doc_relationship(
            pkg,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header",
            "header_chuanhoa.xml",
        )?);

        if cfg.mirror_odd_even {
            let xml_even = build_header_footer_xml(
                "w:hdr",
                cfg.page_numbers_enabled && is_top,
                even_jc,
                &cfg.header_text,
                &fmt.font_name,
                cfg.page_num_font_size_pt,
            );
            pkg.set_xml("word/header_chuanhoa_even.xml", xml_even);
            ensure_content_type_override(
                pkg,
                "/word/header_chuanhoa_even.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
            )?;
            hdr_rid_even = Some(ensure_doc_relationship(
                pkg,
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header",
                "header_chuanhoa_even.xml",
            )?);
        }
    }

    if want_footer && !preserved_footer_updated {
        let xml = build_header_footer_xml(
            "w:ftr",
            cfg.page_numbers_enabled && !is_top,
            odd_jc,
            &cfg.footer_text,
            &fmt.font_name,
            cfg.page_num_font_size_pt,
        );
        pkg.set_xml("word/footer_chuanhoa.xml", xml);
        ensure_content_type_override(
            pkg,
            "/word/footer_chuanhoa.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
        )?;
        ftr_rid_default = Some(ensure_doc_relationship(
            pkg,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer",
            "footer_chuanhoa.xml",
        )?);

        if cfg.mirror_odd_even {
            let xml_even = build_header_footer_xml(
                "w:ftr",
                cfg.page_numbers_enabled && !is_top,
                even_jc,
                &cfg.footer_text,
                &fmt.font_name,
                cfg.page_num_font_size_pt,
            );
            pkg.set_xml("word/footer_chuanhoa_even.xml", xml_even);
            ensure_content_type_override(
                pkg,
                "/word/footer_chuanhoa_even.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
            )?;
            ftr_rid_even = Some(ensure_doc_relationship(
                pkg,
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer",
                "footer_chuanhoa_even.xml",
            )?);
        }
    }

    if cfg.mirror_odd_even {
        ensure_settings_flag(pkg, "w:evenAndHeaders")?;
    }

    // Ensure r namespace exists on <w:document>
    doc.root.set_attr(
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    );

    if let Some(body) = doc.root.child_mut("w:body") {
        let dist_twips = cm_to_twips(cfg.page_num_distance_cm).max(200).to_string();
        let start_str = cfg.start_at.max(1).to_string();
        let mut is_first_section = true;

        for node in &mut body.children {
            let sect_opt = match node {
                XmlNode::Element(el) if el.name == "w:sectPr" => Some(el),
                XmlNode::Element(el) if el.name == "w:p" => el
                    .child_mut("w:pPr")
                    .and_then(|pp| pp.child_mut("w:sectPr")),
                _ => None,
            };

            let Some(sect_pr) = sect_opt else {
                continue;
            };

            if want_header && !preserved_header_updated {
                sect_pr.remove_children_by_name("w:headerReference");
                if let Some(ref rid) = hdr_rid_default {
                    let mut hr = XmlElement::new("w:headerReference");
                    hr.set_attr("w:type", "default");
                    hr.set_attr("r:id", rid);
                    sect_pr.insert_child_ordered(hr, SECT_PR_ORDER);
                }
                if let Some(ref rid_even) = hdr_rid_even {
                    let mut hr = XmlElement::new("w:headerReference");
                    hr.set_attr("w:type", "even");
                    hr.set_attr("r:id", rid_even);
                    sect_pr.insert_child_ordered(hr, SECT_PR_ORDER);
                }
            } else if cfg.page_numbers_enabled && !is_top && !want_header {
                // Only unlink opposite header references whose stripped part has NO meaningful non-PAGE content left
                retain_non_empty_hf_refs(sect_pr, "w:headerReference", &existing_rels, &non_empty_hf_parts);
            }

            if want_footer && !preserved_footer_updated {
                sect_pr.remove_children_by_name("w:footerReference");
                if let Some(ref rid) = ftr_rid_default {
                    let mut fr = XmlElement::new("w:footerReference");
                    fr.set_attr("w:type", "default");
                    fr.set_attr("r:id", rid);
                    sect_pr.insert_child_ordered(fr, SECT_PR_ORDER);
                }
                if let Some(ref rid_even) = ftr_rid_even {
                    let mut fr = XmlElement::new("w:footerReference");
                    fr.set_attr("w:type", "even");
                    fr.set_attr("r:id", rid_even);
                    sect_pr.insert_child_ordered(fr, SECT_PR_ORDER);
                }
            } else if cfg.page_numbers_enabled && is_top && !want_footer {
                // Only unlink opposite footer references whose stripped part has NO meaningful non-PAGE content left
                retain_non_empty_hf_refs(sect_pr, "w:footerReference", &existing_rels, &non_empty_hf_parts);
            }

            let pg_mar = sect_pr.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
            pg_mar.set_attr("w:header", &dist_twips);
            pg_mar.set_attr("w:footer", &dist_twips);

            if is_first_section {
                let pg_num = sect_pr.ensure_child_ordered("w:pgNumType", SECT_PR_ORDER);
                pg_num.set_attr("w:start", &start_str);

                if cfg.skip_first_page {
                    sect_pr.ensure_child_ordered("w:titlePg", SECT_PR_ORDER);
                } else {
                    sect_pr.remove_children_by_name("w:titlePg");
                }
                is_first_section = false;
            } else {
                sect_pr.remove_children_by_name("w:titlePg");
            }
        }
    }

    Ok(())
}

fn collect_doc_rel_map(pkg: &OpenXmlPackage) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Some(rels_xml) = pkg.get_xml("word/_rels/document.xml.rels") else {
        return map;
    };
    let Ok(doc) = XmlDocument::parse(&rels_xml) else {
        return map;
    };
    for node in &doc.root.children {
        if let XmlNode::Element(el) = node {
            if el.name == "Relationship" {
                if let (Some(id), Some(target)) = (el.get_attr("Id"), el.get_attr("Target")) {
                    let norm = if target.starts_with('/') {
                        target.trim_start_matches('/').to_string()
                    } else if target.to_ascii_lowercase().starts_with("word/") {
                        target.to_string()
                    } else {
                        format!("word/{}", target)
                    };
                    map.insert(id.to_string(), norm);
                }
            }
        }
    }
    map
}

fn find_existing_default_hf_part(
    doc: &XmlDocument,
    rel_map: &std::collections::HashMap<String, String>,
    ref_tag: &str,
) -> Option<String> {
    let body = doc.root.child("w:body")?;
    for node in &body.children {
        let sect_opt = match node {
            XmlNode::Element(el) if el.name == "w:sectPr" => Some(el),
            XmlNode::Element(el) if el.name == "w:p" => el
                .child("w:pPr")
                .and_then(|pp| pp.child("w:sectPr")),
            _ => None,
        };
        let Some(sect_pr) = sect_opt else { continue; };
        for ch in &sect_pr.children {
            if let XmlNode::Element(r) = ch {
                if r.name == ref_tag
                    && r.get_attr("w:type").unwrap_or("default") == "default"
                {
                    if let Some(rid) = r.get_attr("r:id") {
                        if let Some(target) = rel_map.get(rid) {
                            return Some(target.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

fn retain_non_empty_hf_refs(
    sect_pr: &mut XmlElement,
    ref_tag: &str,
    rel_map: &std::collections::HashMap<String, String>,
    non_empty_hf_parts: &HashSet<String>,
) {
    sect_pr.children.retain(|n| {
        let XmlNode::Element(el) = n else {
            return true;
        };
        if el.name != ref_tag {
            return true;
        }
        let Some(rid) = el.get_attr("r:id") else {
            return false;
        };
        rel_map
            .get(rid)
            .map(|target| non_empty_hf_parts.contains(&target.to_ascii_lowercase()))
            .unwrap_or(false)
    });
}

fn hf_has_meaningful_content(root: &XmlElement) -> bool {
    if !root.extract_text().trim().is_empty() {
        return true;
    }
    has_visual_or_table_child(root)
}

fn has_visual_or_table_child(el: &XmlElement) -> bool {
    if matches!(
        el.name.as_str(),
        "w:tbl" | "w:drawing" | "w:pict" | "w:object"
    ) {
        return true;
    }
    el.children.iter().any(|c| match c {
        XmlNode::Element(sub) => has_visual_or_table_child(sub),
        _ => false,
    })
}

fn append_page_num_to_hf_doc(
    hf_doc: &mut XmlDocument,
    jc_val: &str,
    font_name: &str,
    font_size_pt: f64,
) -> Result<(), String> {
    let half_pt = pt_to_half_pts(font_size_pt);
    let p_xml = format!(
        r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:pPr><w:jc w:val="{jc}"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:t>1</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        jc = jc_val,
        font = escape_xml_text(font_name),
        half = half_pt
    );
    let mut p_doc = XmlDocument::parse(&p_xml)?;
    p_doc.root.attrs.retain(|(k, _)| k != "xmlns:w");
    hf_doc.root.children.push(XmlNode::Element(p_doc.root));
    Ok(())
}

fn strip_page_fields_from_hf(root: &mut XmlElement) {
    root.children.retain(|n| !element_contains_page_field(n));
}

fn element_contains_page_field(node: &XmlNode) -> bool {
    let XmlNode::Element(el) = node else {
        return false;
    };
    if el.name == "w:docPartGallery"
        && el
            .get_attr("w:val")
            .map(|v| v.contains("Page Numbers"))
            .unwrap_or(false)
    {
        return true;
    }
    if el.name == "w:fldSimple"
        && el
            .get_attr("w:instr")
            .map(|v| v.contains("PAGE"))
            .unwrap_or(false)
    {
        return true;
    }
    if el.name == "w:instrText" && el.extract_text().contains("PAGE") {
        return true;
    }
    el.children.iter().any(element_contains_page_field)
}

fn build_header_footer_xml(
    root_tag: &str,
    include_page_num: bool,
    jc_val: &str,
    custom_text: &str,
    font_name: &str,
    font_size_pt: f64,
) -> String {
    let half_pt = pt_to_half_pts(font_size_pt);
    let mut body = String::new();

    if !custom_text.trim().is_empty() {
        let escaped = escape_xml_text(custom_text.trim());
        body.push_str(&format!(
            r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:t>{escaped}</w:t></w:r></w:p>"#,
            font = escape_xml_text(font_name),
            half = half_pt,
            escaped = escaped
        ));
    }

    if include_page_num {
        body.push_str(&format!(
            r#"<w:p><w:pPr><w:jc w:val="{jc}"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:t>1</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
            jc = jc_val,
            font = escape_xml_text(font_name),
            half = half_pt
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><{tag} xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{body}</{tag}>"#,
        tag = root_tag,
        body = body
    )
}

pub fn ensure_content_type_override(
    pkg: &mut OpenXmlPackage,
    part_name: &str,
    content_type: &str,
) -> Result<(), String> {
    let ct_xml = pkg
        .get_xml("[Content_Types].xml")
        .unwrap_or_else(|| r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>"#.to_string());
    let mut doc = XmlDocument::parse(&ct_xml)?;

    let already = doc.root.children.iter().any(|n| {
        matches!(n, XmlNode::Element(e) if e.name == "Override" && e.get_attr("PartName") == Some(part_name))
    });
    if !already {
        let mut ov = XmlElement::new("Override");
        ov.set_attr("PartName", part_name);
        ov.set_attr("ContentType", content_type);
        doc.root.children.push(XmlNode::Element(ov));
        pkg.set_xml("[Content_Types].xml", doc.to_xml_string()?);
    }
    Ok(())
}

pub fn ensure_doc_relationship(
    pkg: &mut OpenXmlPackage,
    rel_type: &str,
    target: &str,
) -> Result<String, String> {
    let rels_path = "word/_rels/document.xml.rels";
    let rels_xml = pkg.get_xml(rels_path).unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#.to_string()
    });
    let mut doc = XmlDocument::parse(&rels_xml)?;

    for node in &doc.root.children {
        if let XmlNode::Element(e) = node {
            if e.name == "Relationship"
                && e.get_attr("Target") == Some(target)
                && e.get_attr("Type") == Some(rel_type)
            {
                if let Some(id) = e.get_attr("Id") {
                    return Ok(id.to_string());
                }
            }
        }
    }

    let mut max_id = 0u32;
    for node in &doc.root.children {
        if let XmlNode::Element(e) = node {
            if let Some(id) = e.get_attr("Id") {
                if let Some(num_s) = id.strip_prefix("rId") {
                    if let Ok(num) = num_s.parse::<u32>() {
                        max_id = max_id.max(num);
                    }
                }
            }
        }
    }
    let new_id = format!("rId{}", max_id + 1);
    let mut rel = XmlElement::new("Relationship");
    rel.set_attr("Id", &new_id);
    rel.set_attr("Type", rel_type);
    rel.set_attr("Target", target);
    doc.root.children.push(XmlNode::Element(rel));

    pkg.set_xml(rels_path, doc.to_xml_string()?);
    Ok(new_id)
}

fn ensure_settings_flag(pkg: &mut OpenXmlPackage, tag_name: &str) -> Result<(), String> {
    let settings_xml = pkg.get_xml("word/settings.xml").unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"></w:settings>"#.to_string()
    });
    let mut doc = XmlDocument::parse(&settings_xml)?;
    if doc.root.child(tag_name).is_none() {
        doc.root.children.push(XmlNode::Element(XmlElement::new(tag_name)));
        pkg.set_xml("word/settings.xml", doc.to_xml_string()?);
    }
    Ok(())
}

fn enable_update_fields_on_open(pkg: &mut OpenXmlPackage) -> Result<(), String> {
    let settings_xml = pkg.get_xml("word/settings.xml").unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"></w:settings>"#.to_string()
    });
    let mut doc = XmlDocument::parse(&settings_xml)?;
    let uf = doc.root.ensure_child_first("w:updateFields");
    uf.set_attr("w:val", "true");
    pkg.set_xml("word/settings.xml", doc.to_xml_string()?);
    Ok(())
}

fn ensure_nd30_heading_styles(pkg: &mut OpenXmlPackage, fmt: &FormatConfig) -> Result<(), String> {
    let styles_xml = pkg.get_xml("word/styles.xml").unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"></w:styles>"#.to_string()
    });
    let mut doc = XmlDocument::parse(&styles_xml)?;

    let specs = [
        ("Heading1", "heading 1", 0, true, false, true),  // IN HOA, ĐẬM, ĐỨNG
        ("Heading2", "heading 2", 1, true, false, false), // thường, ĐẬM, ĐỨNG
        ("Heading3", "heading 3", 2, true, true, false),  // thường, ĐẬM, NGHIÊNG
        ("Heading4", "heading 4", 3, true, true, false),  // thường, Đậm, Nghiêng
    ];

    for (style_id, style_name, outline_lvl, bold, italic, all_caps) in specs {
        let existing_idx = doc.root.children.iter().position(|n| {
            matches!(n, XmlNode::Element(e) if e.name == "w:style" && e.get_attr("w:styleId") == Some(style_id))
        });

        let style_el = if let Some(idx) = existing_idx {
            match &mut doc.root.children[idx] {
                XmlNode::Element(e) => e,
                _ => unreachable!(),
            }
        } else {
            let mut s = XmlElement::new("w:style");
            s.set_attr("w:type", "paragraph");
            s.set_attr("w:styleId", style_id);
            let mut name_el = XmlElement::new("w:name");
            name_el.set_attr("w:val", style_name);
            s.children.push(XmlNode::Element(name_el));
            s.children.push(XmlNode::Element(XmlElement::new("w:qFormat")));
            doc.root.children.push(XmlNode::Element(s));
            match doc.root.children.last_mut().unwrap() {
                XmlNode::Element(e) => e,
                _ => unreachable!(),
            }
        };

        let p_pr = style_el.ensure_child_ordered("w:pPr", &["w:name", "w:basedOn", "w:next", "w:qFormat", "w:pPr", "w:rPr"]);
        p_pr.ensure_child_ordered("w:keepNext", P_PR_ORDER);
        let spacing = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
        spacing.set_attr("w:before", "120");
        spacing.set_attr("w:after", "120");
        let ol = p_pr.ensure_child_ordered("w:outlineLvl", P_PR_ORDER);
        ol.set_attr("w:val", &outline_lvl.to_string());

        let r_pr = style_el.ensure_child_ordered("w:rPr", &["w:name", "w:basedOn", "w:next", "w:qFormat", "w:pPr", "w:rPr"]);
        set_run_font_and_size(r_pr, &fmt.font_name, Some(fmt.font_size_pt));
        if bold {
            r_pr.ensure_child_ordered("w:b", R_PR_ORDER);
            r_pr.ensure_child_ordered("w:bCs", R_PR_ORDER);
        } else {
            r_pr.remove_children_by_name("w:b");
            r_pr.remove_children_by_name("w:bCs");
        }
        if italic {
            r_pr.ensure_child_ordered("w:i", R_PR_ORDER);
            r_pr.ensure_child_ordered("w:iCs", R_PR_ORDER);
        } else {
            r_pr.remove_children_by_name("w:i");
            r_pr.remove_children_by_name("w:iCs");
        }
        if all_caps {
            r_pr.ensure_child_ordered("w:caps", R_PR_ORDER);
        } else {
            r_pr.remove_children_by_name("w:caps");
        }
        let color = r_pr.ensure_child_ordered("w:color", R_PR_ORDER);
        color.set_attr("w:val", "000000");
        color.remove_attr("w:themeColor");
        color.remove_attr("w:themeShade");
        color.remove_attr("w:themeTint");
    }

    // Also clear blue themeColor on linked Heading1Char..Heading4Char character styles
    for node in &mut doc.root.children {
        if let XmlNode::Element(style_el) = node {
            if style_el.name == "w:style" {
                if let Some(sid) = style_el.get_attr("w:styleId") {
                    if matches!(
                        sid,
                        "Heading1Char" | "Heading2Char" | "Heading3Char" | "Heading4Char"
                    ) {
                        if let Some(r_pr) = style_el.child_mut("w:rPr") {
                            set_run_font_and_size(r_pr, &fmt.font_name, Some(fmt.font_size_pt));
                            let color = r_pr.ensure_child_ordered("w:color", R_PR_ORDER);
                            color.set_attr("w:val", "000000");
                            color.remove_attr("w:themeColor");
                            color.remove_attr("w:themeShade");
                            color.remove_attr("w:themeTint");
                        }
                    }
                }
            }
        }
    }

    pkg.set_xml("word/styles.xml", doc.to_xml_string()?);
    Ok(())
}

fn sanitize_stray_outline_levels(el: &mut XmlElement) {
    sanitize_outline_recursive(el, false);
}

fn sanitize_outline_recursive(el: &mut XmlElement, in_table: bool) {
    for node in &mut el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:p" => {
                let is_heading_style = child
                    .child("w:pPr")
                    .and_then(|pp| pp.child("w:pStyle"))
                    .and_then(|s| s.get_attr("w:val"))
                    .map(|v| v.starts_with("Heading"))
                    .unwrap_or(false);
                if in_table || !is_heading_style {
                    if let Some(p_pr) = child.child_mut("w:pPr") {
                        p_pr.remove_children_by_name("w:outlineLvl");
                    }
                }
            }
            "w:tbl" => {
                sanitize_outline_recursive(child, true);
            }
            "w:tr" | "w:tc" | "w:sdt" | "w:sdtContent" | "w:txbxContent" => {
                sanitize_outline_recursive(child, in_table);
            }
            _ => {}
        }
    }
}

fn is_paragraph_mostly_bold(p: &XmlElement) -> bool {
    let mut total_chars = 0usize;
    let mut bold_chars = 0usize;
    let para_bold = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:rPr"))
        .and_then(|rp| rp.child("w:b"))
        .is_some();

    for node in &p.children {
        if let XmlNode::Element(r) = node {
            if r.name == "w:r" {
                let t = r.extract_text();
                let len = t.trim().chars().count();
                if len == 0 {
                    continue;
                }
                total_chars += len;
                let run_bold = r
                    .child("w:rPr")
                    .and_then(|rp| rp.child("w:b"))
                    .map(|b| b.get_attr("w:val") != Some("0") && b.get_attr("w:val") != Some("false"))
                    .unwrap_or(para_bold);
                if run_bold {
                    bold_chars += len;
                }
            }
        }
    }
    total_chars > 0 && bold_chars * 10 >= total_chars * 8
}

fn auto_detect_headings_in_body(body: &mut XmlElement) -> usize {
    let re_h1 = re_heading_1();
    let re_h2_dieu_muc = re_heading_2_dieu_muc();
    let re_h2_num = re_heading_2_numbered();
    let re_h3 = re_heading_3();
    let re_h4 = re_heading_4();

    let mut count = 0usize;
    let mut after_admin_header_table = false;
    let mut in_signature_zone = false;

    for node in &mut body.children {
        let XmlNode::Element(el) = node else {
            continue;
        };
        if el.name == "w:tbl" {
            after_admin_header_table = crate::engine::formatter::is_admin_header_table(el);
            in_signature_zone = false;
            continue;
        }
        if el.name != "w:p" {
            continue;
        }
        let p = el;
        let text = p.extract_text();
        let trimmed = text.trim();
        let upper = trimmed.to_uppercase();
        let jc = p
            .child("w:pPr")
            .and_then(|pp| pp.child("w:jc"))
            .and_then(|j| j.get_attr("w:val"))
            .unwrap_or("")
            .to_string();
        let has_tabs = p.child("w:pPr").and_then(|pp| pp.child("w:tabs")).is_some();

        if after_admin_header_table {
            let is_title_line = trimmed.is_empty()
                || jc == "center"
                || trimmed.chars().all(|c| c == '-' || c == '_')
                || upper.starts_with("QUYẾT ĐỊNH")
                || upper.starts_with("ĐỀ ÁN")
                || upper.starts_with("VỀ VIỆC")
                || upper.starts_with("(BAN HÀNH")
                || upper.starts_with("HĐQT ");
            if is_title_line {
                continue;
            }
            after_admin_header_table = false;
        }

        if crate::engine::formatter::is_signature_trigger_paragraph(p, trimmed) {
            in_signature_zone = true;
            continue;
        } else if in_signature_zone {
            let still_sig = !crate::engine::formatter::starts_with_heading_numbering(trimmed)
                && (trimmed.is_empty()
                    || trimmed.starts_with('-')
                    || trimmed.starts_with('+')
                    || trimmed.chars().count() <= 60
                    || has_tabs);
            if still_sig {
                continue;
            }
            in_signature_zone = false;
        }

        let char_len = trimmed.chars().count();
        if trimmed.is_empty() || char_len > 160 {
            continue;
        }
        if upper.contains("CỘNG HÒA XÃ HỘI") || upper.contains("ĐỘC LẬP - TỰ DO") {
            continue;
        }

        let bold_para = is_paragraph_mostly_bold(p);

        let detected = if re_h1.is_match(trimmed) && !trimmed.ends_with(';') {
            Some(("Heading1", "0"))
        } else if re_h4.is_match(trimmed) && char_len <= 120 && !trimmed.ends_with(';') {
            Some(("Heading4", "3"))
        } else if re_h3.is_match(trimmed) && char_len <= 120 && !trimmed.ends_with(';') {
            Some(("Heading3", "2"))
        } else if re_h2_dieu_muc.is_match(trimmed) {
            // Skip full legislative/decision clauses like "Điều 1. Ban hành kèm theo..." unless short/bold
            if char_len <= 95 && (bold_para || (!trimmed.ends_with(';') && !trimmed.ends_with('.'))) {
                Some(("Heading2", "1"))
            } else {
                None
            }
        } else if re_h2_num.is_match(trimmed) {
            if char_len <= 120 && !trimmed.ends_with(';') && (bold_para || !trimmed.ends_with('.')) {
                Some(("Heading2", "1"))
            } else {
                None
            }
        } else {
            None
        };

        if let Some((style_id, lvl)) = detected {
            let p_pr = p.ensure_child_first("w:pPr");
            let p_style = p_pr.ensure_child_ordered("w:pStyle", P_PR_ORDER);
            p_style.set_attr("w:val", style_id);
            let ol = p_pr.ensure_child_ordered("w:outlineLvl", P_PR_ORDER);
            ol.set_attr("w:val", lvl);
            if style_id == "Heading1" {
                p_pr.remove_children_by_name("w:ind");
            }
            count += 1;
        }
    }
    count
}

fn insert_toc_block(body: &mut XmlElement, levels: u8, page_break_after: bool, fmt: &FormatConfig) {
    // Remove an existing generated TOC sdt if re-running
    body.children.retain(|n| {
        !matches!(n, XmlNode::Element(e) if e.name == "w:sdt"
            && e.child("w:sdtPr").and_then(|sp| sp.child("w:docPartObj"))
                .and_then(|dp| dp.child("w:docPartGallery"))
                .and_then(|g| g.get_attr("w:val")) == Some("Table of Contents"))
    });

    let half = pt_to_half_pts(fmt.font_size_pt);
    let font = escape_xml_text(&fmt.font_name);
    let page_break_xml = if page_break_after {
        r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#
    } else {
        ""
    };

    let toc_xml = format!(
        r#"<w:sdt xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
            <w:sdtPr><w:docPartObj><w:docPartGallery w:val="Table of Contents"/><w:docPartUnique/></w:docPartObj></w:sdtPr>
            <w:sdtContent>
                <w:p><w:pPr><w:jc w:val="center"/><w:spacing w:before="120" w:after="160"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/><w:b/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/></w:rPr><w:t>MỤC LỤC</w:t></w:r></w:p>
                <w:p><w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-{levels}" \h \z </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t xml:space="preserve">(Mở tệp bằng Microsoft Word hoặc nhấn F9 để cập nhật số trang mục lục)</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
                {page_break}
            </w:sdtContent>
        </w:sdt>"#,
        font = font,
        half = half,
        levels = levels,
        page_break = page_break_xml
    );

    if let Ok(toc_doc) = XmlDocument::parse(&toc_xml) {
        let mut sdt_el = toc_doc.root;
        sdt_el.remove_attr("xmlns:w");
        // Insert immediately before the first Heading1 (or Heading2) so cover/Quốc hiệu/Quyết định stay intact above TOC
        let first_h1 = body.children.iter().position(|n| {
            matches!(n, XmlNode::Element(p) if p.name == "w:p"
                && p.child("w:pPr")
                    .and_then(|pp| pp.child("w:pStyle"))
                    .and_then(|s| s.get_attr("w:val")) == Some("Heading1"))
        });
        let first_h2 = body.children.iter().position(|n| {
            matches!(n, XmlNode::Element(p) if p.name == "w:p"
                && p.child("w:pPr")
                    .and_then(|pp| pp.child("w:pStyle"))
                    .and_then(|s| s.get_attr("w:val")) == Some("Heading2"))
        });
        let insert_at = first_h1.or(first_h2).unwrap_or(0);
        body.children.insert(insert_at, XmlNode::Element(sdt_el));
    }
}

fn escape_xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn re_heading_1() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(PHẦN|CHƯƠNG)\s+[IVXLCDM\d]+([.:–\-\s]|$)|^[IVXLCDM]{1,5}\.\s+\S+")
            .expect("valid h1 regex")
    })
}

fn re_heading_2_dieu_muc() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(MỤC|ĐIỀU)\s+\d+[.:–\-\s]?")
            .expect("valid h2 dieu/muc regex")
    })
}

fn re_heading_2_numbered() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d{1,2}\.\s+\S+")
            .expect("valid h2 numbered regex")
    })
}

fn re_heading_3() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d{1,2}\.\d{1,2}\.?\s+\S+|^[a-zđ]\)\s+\S+")
            .expect("valid h3 regex")
    })
}

fn re_heading_4() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d{1,2}\.\d{1,2}\.\d{1,2}\.?\s+\S+|^[a-zđ]{2}\)\s+\S+")
            .expect("valid h4 regex")
    })
}
