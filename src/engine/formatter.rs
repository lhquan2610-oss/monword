use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::xml_helpers::{
    cm_to_twips, line_to_twips, pt_to_half_pts, pt_to_twips, set_run_font_and_size, XmlDocument,
    XmlElement, XmlNode, P_PR_ORDER, R_PR_ORDER, SECT_PR_ORDER, TBL_CELL_MAR_ORDER,
    TBL_PR_ORDER, TC_PR_ORDER, TR_PR_ORDER,
};
use crate::models::{FormatConfig, PaperSize};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct FormatStats {
    pub paragraphs_formatted: usize,
    pub admin_blocks_protected: usize,
    pub tables_optimized: usize,
    pub runs_updated: usize,
    pub sections_updated: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminTableKind {
    NationalHeader,
    SignatureOrRecipient,
    None,
}

#[derive(Debug, Clone, Copy)]
struct SectionMetrics {
    printable_width_twips: i32,
    margin_left_twips: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdminTextRole {
    NationalOrAgencyCaps,
    MottoDateOrNumber,
    Recipient,
    GeneralAdmin,
}

#[derive(Debug, Default, Clone)]
struct OriginalStyleSizes {
    default_pt: Option<f64>,
    default_para_style: Option<String>,
    style_pts: HashMap<String, f64>,
    style_bold: HashMap<String, bool>,
    style_caps: HashMap<String, bool>,
}

impl OriginalStyleSizes {
    fn from_styles_xml(xml: Option<&str>) -> Self {
        let Some(xml_str) = xml else {
            return Self::default();
        };
        let Ok(doc) = XmlDocument::parse(xml_str) else {
            return Self::default();
        };

        let mut out = Self::default();

        if let Some(r_pr) = doc
            .root
            .child("w:docDefaults")
            .and_then(|d| d.child("w:rPrDefault"))
            .and_then(|d| d.child("w:rPr"))
        {
            out.default_pt = read_rpr_sz_pt(r_pr);
        }

        struct RawStyle {
            sz_pt: Option<f64>,
            bold: Option<bool>,
            caps: Option<bool>,
            based_on: Option<String>,
        }

        let mut raw_styles: HashMap<String, RawStyle> = HashMap::new();
        for node in &doc.root.children {
            let XmlNode::Element(el) = node else {
                continue;
            };
            if el.name != "w:style" {
                continue;
            }
            let Some(style_id) = el.get_attr("w:styleId") else {
                continue;
            };
            if el.get_attr("w:type") == Some("paragraph") && el.get_attr("w:default") == Some("1") {
                out.default_para_style = Some(style_id.to_string());
            }
            let based_on = el
                .child("w:basedOn")
                .and_then(|b| b.get_attr("w:val"))
                .map(|s| s.to_string());
            let r_pr = el.child("w:rPr");
            raw_styles.insert(
                style_id.to_string(),
                RawStyle {
                    sz_pt: r_pr.and_then(read_rpr_sz_pt),
                    bold: r_pr.and_then(|r| read_rpr_flag(r, "w:b")),
                    caps: r_pr.and_then(|r| read_rpr_flag(r, "w:caps")),
                    based_on,
                },
            );
        }

        for style_id in raw_styles.keys() {
            let mut cur = Some(style_id.as_str());
            let mut sz: Option<f64> = None;
            let mut bold: Option<bool> = None;
            let mut caps: Option<bool> = None;
            for _ in 0..16 {
                let Some(id) = cur else {
                    break;
                };
                let Some(st) = raw_styles.get(id) else {
                    break;
                };
                if sz.is_none() {
                    sz = st.sz_pt;
                }
                if bold.is_none() {
                    bold = st.bold;
                }
                if caps.is_none() {
                    caps = st.caps;
                }
                if sz.is_some() && bold.is_some() && caps.is_some() {
                    break;
                }
                cur = st.based_on.as_deref();
            }
            if let Some(pt) = sz {
                out.style_pts.insert(style_id.clone(), pt);
            }
            if let Some(b) = bold {
                out.style_bold.insert(style_id.clone(), b);
            }
            if let Some(c) = caps {
                out.style_caps.insert(style_id.clone(), c);
            }
        }

        out
    }

    fn resolve_pt(&self, p_style: Option<&str>, r_style: Option<&str>) -> Option<f64> {
        if let Some(rs) = r_style {
            if let Some(&pt) = self.style_pts.get(rs) {
                return Some(pt);
            }
        }
        if let Some(ps) = p_style {
            if let Some(&pt) = self.style_pts.get(ps) {
                return Some(pt);
            }
        } else if let Some(ref def_ps) = self.default_para_style {
            if let Some(&pt) = self.style_pts.get(def_ps) {
                return Some(pt);
            }
        }
        self.default_pt
    }

    fn resolve_bold(&self, p_style: Option<&str>, r_style: Option<&str>) -> bool {
        if let Some(rs) = r_style {
            if let Some(&b) = self.style_bold.get(rs) {
                return b;
            }
        }
        if let Some(ps) = p_style {
            if let Some(&b) = self.style_bold.get(ps) {
                return b;
            }
        }
        false
    }

    fn resolve_caps(&self, p_style: Option<&str>, r_style: Option<&str>) -> bool {
        if let Some(rs) = r_style {
            if let Some(&c) = self.style_caps.get(rs) {
                return c;
            }
        }
        if let Some(ps) = p_style {
            if let Some(&c) = self.style_caps.get(ps) {
                return c;
            }
        }
        false
    }
}

fn read_rpr_sz_pt(r_pr: &XmlElement) -> Option<f64> {
    r_pr.child("w:sz")
        .and_then(|s| s.get_attr("w:val"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|half| half / 2.0)
}

fn read_rpr_flag(r_pr: &XmlElement, tag: &str) -> Option<bool> {
    let el = r_pr.child(tag)?;
    let val = el.get_attr("w:val");
    Some(!matches!(val, Some("0") | Some("false")))
}

pub fn apply_formatting(
    pkg: &mut OpenXmlPackage,
    cfg: &FormatConfig,
    logs: &mut Vec<String>,
) -> Result<FormatStats, String> {
    if !cfg.enabled {
        return Ok(FormatStats::default());
    }

    if pkg.is_excel() {
        return apply_excel_font_formatting(pkg, cfg, logs);
    }

    let orig_styles = OriginalStyleSizes::from_styles_xml(pkg.get_xml("word/styles.xml").as_deref());

    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Không tìm thấy word/document.xml trong tệp Word".to_string())?;
    let mut doc = XmlDocument::parse(&doc_xml)?;
    let mut stats = FormatStats::default();

    if let Some(body) = doc.root.child_mut("w:body") {
        let child_metrics = compute_body_section_metrics(body, cfg);
        let mut para_idx = 0usize;
        let mut paras_since_admin_header: Option<usize> = None;

        format_body_children(
            body,
            cfg,
            &orig_styles,
            &child_metrics,
            &mut para_idx,
            &mut paras_since_admin_header,
            &mut stats,
        );

        // Update final body-level sectPr
        if let Some(sect_pr) = body.child_mut("w:sectPr") {
            update_sect_pr_page_and_margins(sect_pr, cfg);
            stats.sections_updated += 1;
        } else {
            let mut sect_pr = XmlElement::new("w:sectPr");
            update_sect_pr_page_and_margins(&mut sect_pr, cfg);
            body.children.push(XmlNode::Element(sect_pr));
            stats.sections_updated += 1;
        }
    }

    pkg.set_xml("word/document.xml", doc.to_xml_string()?);

    if cfg.sync_styles_default {
        if let Some(styles_xml) = pkg.get_xml("word/styles.xml") {
            if let Ok(mut styles_doc) = XmlDocument::parse(&styles_xml) {
                sync_styles_defaults(&mut styles_doc.root, cfg);
                if let Ok(updated) = styles_doc.to_xml_string() {
                    pkg.set_xml("word/styles.xml", updated);
                }
            }
        }
    }

    logs.push(format!(
        "Định dạng hoàn tất: {} đoạn văn, bảo vệ {} khối hành chính, tối ưu {} bảng, {} section.",
        stats.paragraphs_formatted,
        stats.admin_blocks_protected,
        stats.tables_optimized,
        stats.sections_updated
    ));

    Ok(stats)
}

fn compute_body_section_metrics(body: &XmlElement, cfg: &FormatConfig) -> Vec<SectionMetrics> {
    let (pw, ph) = paper_dimensions_twips(cfg.paper_size);
    let left_tw = cm_to_twips(cfg.margin_left_cm);
    let right_tw = cm_to_twips(cfg.margin_right_cm);

    let portrait_metrics = SectionMetrics {
        printable_width_twips: (pw - left_tw - right_tw).max(3000),
        margin_left_twips: left_tw,
    };
    let landscape_metrics = SectionMetrics {
        printable_width_twips: (ph - left_tw - right_tw).max(3000),
        margin_left_twips: left_tw,
    };

    let mut result = vec![portrait_metrics; body.children.len()];
    let mut section_start = 0usize;

    for (idx, node) in body.children.iter().enumerate() {
        let XmlNode::Element(el) = node else {
            continue;
        };
        let sect_opt = if el.name == "w:sectPr" {
            Some(el)
        } else if el.name == "w:p" {
            el.child("w:pPr").and_then(|pp| pp.child("w:sectPr"))
        } else {
            None
        };

        if let Some(sect_pr) = sect_opt {
            let is_land = sect_pr
                .child("w:pgSz")
                .and_then(|s| s.get_attr("w:orient"))
                .map(|o| o == "landscape")
                .unwrap_or(false);
            let m = if is_land {
                landscape_metrics
            } else {
                portrait_metrics
            };
            for slot in &mut result[section_start..=idx] {
                *slot = m;
            }
            section_start = idx + 1;
        }
    }

    result
}

fn format_body_children(
    body: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    child_metrics: &[SectionMetrics],
    para_idx: &mut usize,
    paras_since_admin_header: &mut Option<usize>,
    stats: &mut FormatStats,
) {
    let default_metrics = SectionMetrics {
        printable_width_twips: (paper_dimensions_twips(cfg.paper_size).0
            - cm_to_twips(cfg.margin_left_cm)
            - cm_to_twips(cfg.margin_right_cm))
        .max(3000),
        margin_left_twips: cm_to_twips(cfg.margin_left_cm),
    };

    let mut in_signature_zone = false;
    let mut after_admin_header_table = false;

    for (idx, node) in body.children.iter_mut().enumerate() {
        let XmlNode::Element(el) = node else {
            continue;
        };
        let metrics = child_metrics.get(idx).copied().unwrap_or(default_metrics);
        match el.name.as_str() {
            "w:p" => {
                let current_idx = *para_idx;
                *para_idx += 1;
                let text = el.extract_text();
                let trimmed = text.trim();
                let upper = collapse_spaces_upper(trimmed);
                if is_national_header_cue(&upper) {
                    *paras_since_admin_header = Some(0);
                    in_signature_zone = false;
                }

                let p_style = el
                    .child("w:pPr")
                    .and_then(|pp| pp.child("w:pStyle"))
                    .and_then(|s| s.get_attr("w:val"))
                    .unwrap_or("");
                let is_heading = p_style.starts_with("Heading")
                    || p_style.starts_with("TOC")
                    || p_style == "Title"
                    || p_style == "Subtitle";
                let has_tabs = el
                    .child("w:pPr")
                    .and_then(|pp| pp.child("w:tabs"))
                    .is_some()
                    || has_tab_node(el);

                if cfg.protect_admin_blocks && !is_heading {
                    if after_admin_header_table {
                        let is_title_line = !starts_with_heading_numbering(trimmed)
                            && (trimmed.is_empty()
                                || has_drawing_or_hr(el)
                                || paragraph_has_center_or_right_jc(el)
                                || trimmed.chars().all(|c| c == '-' || c == '_')
                                || upper.starts_with("QUYẾT ĐỊNH")
                                || upper.starts_with("ĐỀ ÁN")
                                || upper.starts_with("VỀ VIỆC")
                                || upper.starts_with("(BAN HÀNH")
                                || upper.starts_with("HĐQT "));
                        if !is_title_line {
                            after_admin_header_table = false;
                        }
                    }

                    if is_signature_trigger_paragraph(el, trimmed) {
                        in_signature_zone = true;
                        after_admin_header_table = false;
                    } else if in_signature_zone {
                        let still_sig = !starts_with_heading_numbering(trimmed)
                            && (trimmed.is_empty()
                                || trimmed.starts_with('-')
                                || trimmed.starts_with('+')
                                || trimmed.chars().count() <= 60
                                || has_tabs);
                        if !still_sig {
                            in_signature_zone = false;
                        }
                    }
                } else if is_heading {
                    after_admin_header_table = false;
                    in_signature_zone = false;
                }

                let near_admin_header = current_idx < 12
                    || paras_since_admin_header
                        .map(|n| n < 15)
                        .unwrap_or(false);

                if cfg.protect_admin_blocks && after_admin_header_table && !is_heading {
                    format_protected_title_paragraph(el, cfg, orig_styles, stats);
                } else if cfg.protect_admin_blocks && in_signature_zone && !is_heading {
                    format_protected_signature_paragraph(
                        el,
                        cfg,
                        orig_styles,
                        metrics.printable_width_twips,
                        stats,
                    );
                } else {
                    format_paragraph(
                        el,
                        cfg,
                        orig_styles,
                        metrics,
                        0,
                        false,
                        near_admin_header,
                        stats,
                    );
                }

                if paragraph_has_page_break(el) {
                    in_signature_zone = false;
                    after_admin_header_table = false;
                }

                if let Some(n) = paras_since_admin_header {
                    let next = *n + 1;
                    if next >= 15 {
                        *paras_since_admin_header = None;
                    } else {
                        *n = next;
                    }
                }
            }
            "w:tbl" => {
                let is_hdr_tbl = cfg.protect_admin_blocks && is_admin_header_table(el);
                in_signature_zone = false;
                format_table(
                    el,
                    cfg,
                    orig_styles,
                    metrics,
                    0,
                    para_idx,
                    paras_since_admin_header,
                    stats,
                );
                after_admin_header_table = is_hdr_tbl;
            }
            "w:sdt" | "w:sdtContent" | "w:txbxContent" => {
                let is_txbx = el.name == "w:txbxContent";
                format_nested_container(
                    el,
                    cfg,
                    orig_styles,
                    metrics,
                    0,
                    is_txbx,
                    para_idx,
                    paras_since_admin_header,
                    stats,
                );
            }
            _ => {}
        }
    }
}

fn has_drawing_or_hr(el: &XmlElement) -> bool {
    el.children.iter().any(|node| match node {
        XmlNode::Element(sub) => {
            matches!(
                sub.name.as_str(),
                "w:drawing" | "w:pict" | "mc:AlternateContent" | "w:pBdr"
            ) || has_drawing_or_hr(sub)
        }
        _ => false,
    })
}

fn format_protected_title_paragraph(
    p: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    stats: &mut FormatStats,
) {
    let keep_indent_zero = paragraph_has_center_or_right_jc(p) || has_drawing_or_hr(p);
    {
        let p_pr = p.ensure_child_first("w:pPr");
        if let Some(sect_pr) = p_pr.child_mut("w:sectPr") {
            update_sect_pr_page_and_margins(sect_pr, cfg);
            stats.sections_updated += 1;
        }
        p_pr.remove_children_by_name("w:outlineLvl");
        if keep_indent_zero {
            if let Some(ind) = p_pr.child_mut("w:ind") {
                if ind.get_attr("w:firstLine").is_some()
                    || ind.get_attr("w:firstLineChars").is_some()
                {
                    ind.set_attr("w:firstLine", "0");
                    ind.remove_attr("w:firstLineChars");
                }
            }
        }
        if let Some(spacing) = p_pr.child_mut("w:spacing") {
            spacing.set_attr("w:after", "40");
            spacing.set_attr("w:line", "240");
            spacing.set_attr("w:lineRule", "auto");
        } else {
            let spacing = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
            spacing.set_attr("w:before", "0");
            spacing.set_attr("w:after", "0");
            spacing.set_attr("w:line", "240");
            spacing.set_attr("w:lineRule", "auto");
        }
    }
    let role = detect_admin_text_role(p, false);
    pin_protected_paragraph_fonts_and_sizes(p, &cfg.font_name, orig_styles, role, stats);
    stats.admin_blocks_protected += 1;
    stats.paragraphs_formatted += 1;
}

fn format_protected_signature_paragraph(
    p: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    max_tab_twips: i32,
    stats: &mut FormatStats,
) {
    let text = p.extract_text();
    let trimmed = text.trim();
    let is_empty = trimmed.is_empty();
    clamp_paragraph_tabs(p, max_tab_twips);
    {
        let p_pr = p.ensure_child_first("w:pPr");
        if let Some(sect_pr) = p_pr.child_mut("w:sectPr") {
            update_sect_pr_page_and_margins(sect_pr, cfg);
            stats.sections_updated += 1;
        }
        p_pr.remove_children_by_name("w:outlineLvl");
        let spacing = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
        spacing.set_attr("w:before", "0");
        spacing.set_attr("w:after", "0");
        spacing.set_attr("w:line", if is_empty { "200" } else { "240" });
        spacing.set_attr("w:lineRule", "auto");
    }
    let upper = collapse_spaces_upper(trimmed);
    let role = if upper.contains("NƠI NHẬN")
        || (!is_empty && (trimmed.starts_with('-') || trimmed.starts_with('+')))
    {
        AdminTextRole::Recipient
    } else {
        AdminTextRole::GeneralAdmin
    };
    pin_protected_paragraph_fonts_and_sizes(p, &cfg.font_name, orig_styles, role, stats);
    stats.admin_blocks_protected += 1;
    stats.paragraphs_formatted += 1;
}

fn paragraph_has_page_break(el: &XmlElement) -> bool {
    el.children.iter().any(|node| match node {
        XmlNode::Element(sub) => {
            (sub.name == "w:br" && sub.get_attr("w:type") == Some("page"))
                || paragraph_has_page_break(sub)
        }
        _ => false,
    })
}

#[allow(clippy::too_many_arguments)]
fn format_nested_container(
    container: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    metrics: SectionMetrics,
    table_depth: usize,
    in_textbox: bool,
    para_idx: &mut usize,
    paras_since_admin_header: &mut Option<usize>,
    stats: &mut FormatStats,
) {
    for node in &mut container.children {
        let XmlNode::Element(el) = node else {
            continue;
        };
        match el.name.as_str() {
            "w:p" => {
                let current_idx = *para_idx;
                *para_idx += 1;
                let upper = collapse_spaces_upper(&el.extract_text());
                if table_depth == 0 && is_national_header_cue(&upper) {
                    *paras_since_admin_header = Some(0);
                }
                let near_admin_header = current_idx < 12
                    || paras_since_admin_header
                        .map(|n| n < 15)
                        .unwrap_or(false);
                format_paragraph(
                    el,
                    cfg,
                    orig_styles,
                    metrics,
                    table_depth,
                    in_textbox,
                    near_admin_header,
                    stats,
                );
                if table_depth == 0 {
                    if let Some(n) = paras_since_admin_header {
                        let next = *n + 1;
                        if next >= 15 {
                            *paras_since_admin_header = None;
                        } else {
                            *n = next;
                        }
                    }
                }
            }
            "w:tbl" => {
                format_table(
                    el,
                    cfg,
                    orig_styles,
                    metrics,
                    table_depth,
                    para_idx,
                    paras_since_admin_header,
                    stats,
                );
            }
            "w:sdt" | "w:sdtContent" | "w:txbxContent" => {
                let child_txbx = in_textbox || el.name == "w:txbxContent";
                format_nested_container(
                    el,
                    cfg,
                    orig_styles,
                    metrics,
                    table_depth,
                    child_txbx,
                    para_idx,
                    paras_since_admin_header,
                    stats,
                );
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn format_paragraph(
    p: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    metrics: SectionMetrics,
    table_depth: usize,
    in_textbox: bool,
    near_admin_header: bool,
    stats: &mut FormatStats,
) {
    let p_style = p
        .child("w:pPr")
        .and_then(|p_pr| p_pr.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"))
        .map(|s| s.to_string());
    let style_id = p_style.as_deref().unwrap_or("");

    let is_heading = style_id.starts_with("Heading")
        || style_id.starts_with("TOC")
        || style_id == "Title"
        || style_id == "Subtitle";

    let protected = table_depth == 0
        && cfg.protect_admin_blocks
        && (in_textbox || is_protected_admin_paragraph_ctx(p, near_admin_header));

    if protected {
        stats.admin_blocks_protected += 1;
    }

    // Check inline sectPr inside pPr
    if let Some(p_pr) = p.child_mut("w:pPr") {
        if let Some(sect_pr) = p_pr.child_mut("w:sectPr") {
            update_sect_pr_page_and_margins(sect_pr, cfg);
            stats.sections_updated += 1;
        }
    }

    let has_center_or_right = paragraph_has_center_or_right_jc(p);

    if table_depth > 0 && cfg.smart_tables {
        let p_pr = p.ensure_child_first("w:pPr");
        p_pr.remove_children_by_name("w:keepNext");
        p_pr.remove_children_by_name("w:pageBreakBefore");
        p_pr.remove_children_by_name("w:outlineLvl");
        let spacing = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
        spacing.set_attr("w:before", "20");
        spacing.set_attr("w:after", "20");
        spacing.set_attr("w:line", "240");
        spacing.set_attr("w:lineRule", "auto");

        let ind = p_pr.ensure_child_ordered("w:ind", P_PR_ORDER);
        ind.set_attr("w:firstLine", "0");
        ind.remove_attr("w:firstLineChars");

        let r_pr_p = p_pr.ensure_child_ordered("w:rPr", P_PR_ORDER);
        set_run_font_and_size(r_pr_p, &cfg.font_name, Some(cfg.table_font_size_pt));

        update_runs_in_element(p, &cfg.font_name, Some(cfg.table_font_size_pt), stats);
    } else if protected || is_heading {
        // Keep alignment, spacing, and indentation untouched on protected blocks or headings;
        // unify font family and pin inherited font size on protected blocks so sync_styles_defaults
        // does not balloon them to 14pt.
        if has_center_or_right {
            if let Some(p_pr) = p.child_mut("w:pPr") {
                if let Some(ind) = p_pr.child_mut("w:ind") {
                    if ind.get_attr("w:firstLine").is_some()
                        || ind.get_attr("w:firstLineChars").is_some()
                    {
                        ind.set_attr("w:firstLine", "0");
                        ind.remove_attr("w:firstLineChars");
                    }
                }
            }
        }

        if protected {
            clamp_paragraph_tabs(p, metrics.printable_width_twips);
            let role = detect_admin_text_role(p, false);
            pin_protected_paragraph_fonts_and_sizes(
                p,
                &cfg.font_name,
                orig_styles,
                role,
                stats,
            );
        } else {
            if let Some(p_pr) = p.child_mut("w:pPr") {
                if let Some(r_pr_p) = p_pr.child_mut("w:rPr") {
                    set_run_font_and_size(r_pr_p, &cfg.font_name, None);
                }
            }
            update_runs_in_element(p, &cfg.font_name, None, stats);
        }
    } else {
        let p_pr = p.ensure_child_first("w:pPr");
        let spacing = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
        spacing.set_attr("w:before", &pt_to_twips(cfg.space_before_pt).to_string());
        spacing.set_attr("w:after", &pt_to_twips(cfg.space_after_pt).to_string());
        spacing.set_attr("w:line", &line_to_twips(cfg.line_spacing).to_string());
        spacing.set_attr("w:lineRule", "auto");

        let ind = p_pr.ensure_child_ordered("w:ind", P_PR_ORDER);
        if has_center_or_right {
            ind.set_attr("w:firstLine", "0");
        } else {
            let first_line_twips = cm_to_twips(cfg.first_line_indent_cm).max(0);
            ind.set_attr("w:firstLine", &first_line_twips.to_string());
        }
        ind.remove_attr("w:firstLineChars");
        ind.remove_attr("w:hanging");
        ind.remove_attr("w:hangingChars");

        if !has_center_or_right {
            let jc = p_pr.ensure_child_ordered("w:jc", P_PR_ORDER);
            jc.set_attr("w:val", "both");
        }

        let r_pr_p = p_pr.ensure_child_ordered("w:rPr", P_PR_ORDER);
        set_run_font_and_size(r_pr_p, &cfg.font_name, Some(cfg.font_size_pt));

        update_runs_in_element(p, &cfg.font_name, Some(cfg.font_size_pt), stats);
    }

    stats.paragraphs_formatted += 1;
}

fn clamp_paragraph_tabs(p: &mut XmlElement, max_pos_twips: i32) {
    let Some(p_pr) = p.child_mut("w:pPr") else {
        return;
    };
    let Some(tabs) = p_pr.child_mut("w:tabs") else {
        return;
    };
    for node in &mut tabs.children {
        if let XmlNode::Element(tab) = node {
            if tab.name == "w:tab" {
                if let Some(pos) = tab.get_attr("w:pos").and_then(|v| v.parse::<i32>().ok()) {
                    if pos > max_pos_twips {
                        tab.set_attr("w:pos", &max_pos_twips.to_string());
                    }
                }
            }
        }
    }
}

fn detect_admin_text_role(p: &XmlElement, in_national_table: bool) -> AdminTextRole {
    let upper = collapse_spaces_upper(&p.extract_text());
    if upper.contains("NƠI NHẬN") {
        return AdminTextRole::Recipient;
    }
    if upper.contains("ĐỘC LẬP - TỰ DO")
        || upper.contains("ĐỘC LẬP – TỰ DO")
        || upper.contains("ĐỘC LẬP-TỰ DO")
        || upper.starts_with("SỐ:")
        || upper.starts_with("SỐ :")
        || (upper.contains("NGÀY ") && upper.contains("THÁNG "))
    {
        return AdminTextRole::MottoDateOrNumber;
    }
    if upper.contains("CỘNG HÒA XÃ HỘI")
        || upper.contains("CỘNG HOÀ XÃ HỘI")
        || upper.contains("ĐẢNG CỘNG SẢN")
        || in_national_table
    {
        return AdminTextRole::NationalOrAgencyCaps;
    }
    AdminTextRole::GeneralAdmin
}

fn role_threshold_and_fallback(role: AdminTextRole) -> (f64, f64) {
    match role {
        AdminTextRole::NationalOrAgencyCaps => (11.5, 12.5),
        AdminTextRole::MottoDateOrNumber => (11.5, 13.0),
        AdminTextRole::Recipient => (10.5, 11.0),
        AdminTextRole::GeneralAdmin => (11.5, 14.0),
    }
}

fn pin_protected_paragraph_fonts_and_sizes(
    p: &mut XmlElement,
    font_name: &str,
    orig_styles: &OriginalStyleSizes,
    role: AdminTextRole,
    stats: &mut FormatStats,
) {
    let p_style = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"))
        .map(|s| s.to_string());
    let (min_threshold, fallback_pt) = role_threshold_and_fallback(role);

    if let Some(p_pr) = p.child_mut("w:pPr") {
        if let Some(r_pr_p) = p_pr.child_mut("w:rPr") {
            let has_sz = r_pr_p.child("w:sz").is_some();
            let target = if has_sz {
                None
            } else {
                let inherited = orig_styles.resolve_pt(p_style.as_deref(), None);
                Some(match inherited {
                    Some(pt) if pt >= min_threshold => pt,
                    _ => fallback_pt,
                })
            };
            set_run_font_and_size(r_pr_p, font_name, target);
        }
    }

    pin_runs_in_element(
        p,
        font_name,
        orig_styles,
        p_style.as_deref(),
        min_threshold,
        fallback_pt,
        stats,
    );
}

fn pin_runs_in_element(
    el: &mut XmlElement,
    font_name: &str,
    orig_styles: &OriginalStyleSizes,
    p_style: Option<&str>,
    min_threshold: f64,
    fallback_pt: f64,
    stats: &mut FormatStats,
) {
    for node in &mut el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:r" => {
                let is_script = child
                    .child("w:rPr")
                    .and_then(|r| r.child("w:vertAlign"))
                    .is_some();
                let has_sz = child
                    .child("w:rPr")
                    .and_then(|r| r.child("w:sz"))
                    .is_some();
                let r_style = child
                    .child("w:rPr")
                    .and_then(|r| r.child("w:rStyle"))
                    .and_then(|s| s.get_attr("w:val"))
                    .map(|s| s.to_string());

                let target_size = if is_script || has_sz {
                    None
                } else {
                    let inherited = orig_styles.resolve_pt(p_style, r_style.as_deref());
                    Some(match inherited {
                        Some(pt) if pt >= min_threshold => pt,
                        _ => fallback_pt,
                    })
                };

                let r_pr = child.ensure_child_first("w:rPr");
                set_run_font_and_size(r_pr, font_name, target_size);
                stats.runs_updated += 1;
            }
            "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                pin_runs_in_element(
                    child,
                    font_name,
                    orig_styles,
                    p_style,
                    min_threshold,
                    fallback_pt,
                    stats,
                );
            }
            _ => {}
        }
    }
}

fn update_runs_in_element(
    el: &mut XmlElement,
    font_name: &str,
    size_pt: Option<f64>,
    stats: &mut FormatStats,
) {
    for node in &mut el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:r" => {
                // Skip updating size on superscript/subscript runs to preserve footnotes/formulas
                let is_script = child
                    .child("w:rPr")
                    .and_then(|r| r.child("w:vertAlign"))
                    .is_some();
                let r_pr = child.ensure_child_first("w:rPr");
                let target_size = if is_script { None } else { size_pt };
                set_run_font_and_size(r_pr, font_name, target_size);
                stats.runs_updated += 1;
            }
            "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                update_runs_in_element(child, font_name, size_pt, stats);
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn format_table(
    tbl: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    metrics: SectionMetrics,
    table_depth: usize,
    para_idx: &mut usize,
    paras_since_admin_header: &mut Option<usize>,
    stats: &mut FormatStats,
) {
    if table_depth == 0 && cfg.protect_admin_blocks {
        match classify_admin_table(tbl) {
            AdminTableKind::NationalHeader => {
                *paras_since_admin_header = Some(0);
                format_national_header_table(tbl, cfg, orig_styles, metrics, para_idx, stats);
                return;
            }
            AdminTableKind::SignatureOrRecipient => {
                format_signature_admin_table(tbl, cfg, orig_styles, metrics, para_idx, stats);
                return;
            }
            AdminTableKind::None => {}
        }
    }

    if cfg.smart_tables {
        let has_explicit_grid = tbl
            .child("w:tblGrid")
            .map(|g| g.children_elements().any(|e| e.name == "w:gridCol"))
            .unwrap_or(false)
            || tbl
                .children_elements()
                .filter(|e| e.name == "w:tr")
                .take(1)
                .flat_map(|tr| tr.children_elements())
                .filter(|e| e.name == "w:tc")
                .any(|tc| tc.child("w:tcPr").and_then(|tp| tp.child("w:tcW")).is_some());
        let col_widths = read_table_col_widths_twips(tbl, metrics.printable_width_twips);
        let grid_sum: i32 = if has_explicit_grid {
            col_widths.iter().sum()
        } else {
            0
        };

        {
            let tbl_pr = tbl.ensure_child_first("w:tblPr");
            tbl_pr.remove_children_by_name("w:tblpPr");
            if grid_sum == 0 || grid_sum >= 7500 {
                if tbl_pr
                    .child("w:tblLayout")
                    .and_then(|tl| tl.get_attr("w:type"))
                    == Some("fixed")
                {
                    tbl_pr.remove_children_by_name("w:tblLayout");
                }
                let tbl_w = tbl_pr.ensure_child_ordered("w:tblW", TBL_PR_ORDER);
                tbl_w.set_attr("w:w", "5000");
                tbl_w.set_attr("w:type", "pct");
                if let Some(tbl_ind) = tbl_pr.child_mut("w:tblInd") {
                    if tbl_ind
                        .get_attr("w:w")
                        .and_then(|v| v.parse::<i32>().ok())
                        .map(|w| w < 0)
                        .unwrap_or(false)
                    {
                        tbl_ind.set_attr("w:w", "0");
                    }
                }
            }
        }

        if grid_sum >= 7500 && grid_sum > metrics.printable_width_twips {
            let scaled = proportional_scale_widths(&col_widths, metrics.printable_width_twips);
            update_grid_and_cell_widths(tbl, &scaled, true);
        }
        stats.tables_optimized += 1;
    }

    let row_count = tbl
        .children
        .iter()
        .filter(|n| matches!(n, XmlNode::Element(e) if e.name == "w:tr"))
        .count();

    let mut row_idx = 0usize;
    for node in &mut tbl.children {
        let XmlNode::Element(tr) = node else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        if cfg.smart_tables {
            let mut row_text_len = 0usize;
            let mut max_cell_chars = 0usize;
            let mut max_paras_in_cell = 0usize;
            for tc in tr.children_elements().filter(|e| e.name == "w:tc") {
                let c_len = tc.extract_text().chars().count();
                row_text_len += c_len;
                max_cell_chars = max_cell_chars.max(c_len);
                let p_count = tc
                    .children_elements()
                    .filter(|e| e.name == "w:p")
                    .count();
                max_paras_in_cell = max_paras_in_cell.max(p_count);
            }

            let tr_pr = tr.ensure_child_first("w:trPr");
            if row_idx > 0 {
                tr_pr.remove_children_by_name("w:trHeight");
            } else {
                let oversized_header_h = tr_pr
                    .child("w:trHeight")
                    .and_then(|h| h.get_attr("w:val"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .map(|v| v > 800)
                    .unwrap_or(false);
                if oversized_header_h {
                    tr_pr.remove_children_by_name("w:trHeight");
                }
            }

            if row_idx == 0
                || (max_paras_in_cell <= 1 && max_cell_chars <= 65 && row_text_len <= 130)
            {
                tr_pr.ensure_child_ordered("w:cantSplit", TR_PR_ORDER);
            } else {
                tr_pr.remove_children_by_name("w:cantSplit");
            }

            if row_idx == 0 && cfg.repeat_table_headers && row_count >= 2 {
                tr_pr.ensure_child_ordered("w:tblHeader", TR_PR_ORDER);
            }
        }
        for cell_node in &mut tr.children {
            if let XmlNode::Element(tc) = cell_node {
                if tc.name == "w:tc" {
                    format_nested_container(
                        tc,
                        cfg,
                        orig_styles,
                        metrics,
                        table_depth + 1,
                        false,
                        para_idx,
                        paras_since_admin_header,
                        stats,
                    );
                }
            }
        }
        row_idx += 1;
    }
}

pub fn classify_admin_table(tbl: &XmlElement) -> AdminTableKind {
    let mut row_count = 0usize;
    let mut max_cols = 0usize;

    for node in &tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                row_count += 1;
                let mut cols_in_row = 0usize;
                for cell_node in &tr.children {
                    if let XmlNode::Element(tc) = cell_node {
                        if tc.name == "w:tc" {
                            let span = tc
                                .child("w:tcPr")
                                .and_then(|tp| tp.child("w:gridSpan"))
                                .and_then(|gs| gs.get_attr("w:val"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .max(1);
                            cols_in_row += span;
                        }
                    }
                }
                max_cols = max_cols.max(cols_in_row);
            }
        }
    }

    if row_count == 0 || max_cols == 0 {
        return AdminTableKind::None;
    }

    let upper = collapse_spaces_upper(&tbl.extract_text());

    const NAT_HEADER_CUES: &[&str] = &[
        "CỘNG HÒA XÃ HỘI",
        "CỘNG HOÀ XÃ HỘI",
        "ĐỘC LẬP - TỰ DO",
        "ĐỘC LẬP – TỰ DO",
        "ĐỘC LẬP-TỰ DO",
        "ĐẢNG CỘNG SẢN VIỆT NAM",
    ];
    if row_count <= 5 && max_cols <= 3 && NAT_HEADER_CUES.iter().any(|c| upper.contains(c)) {
        return AdminTableKind::NationalHeader;
    }

    const SIG_RECIP_CUES: &[&str] = &[
        "NƠI NHẬN",
        "TM.",
        "KT.",
        "TL.",
        "TUQ.",
        "CHỦ TỊCH",
        "TỔNG GIÁM ĐỐC",
        "GIÁM ĐỐC",
        "HIỆU TRƯỞNG",
        "BỘ TRƯỞNG",
        "THỨ TRƯỞNG",
        "TRƯỞNG PHÒNG",
        "THỦ TRƯỞNG",
        "CỤC TRƯỞNG",
        "BÍ THƯ",
        "HỘI ĐỒNG QUẢN TRỊ",
        "ĐẠI DIỆN BÊN",
        "NGƯỜI LẬP",
    ];
    if row_count <= 4
        && max_cols <= 4
        && !has_visible_grid_borders(tbl)
        && SIG_RECIP_CUES.iter().any(|c| upper.contains(c))
    {
        return AdminTableKind::SignatureOrRecipient;
    }

    AdminTableKind::None
}

fn is_border_visible(edge: &XmlElement) -> bool {
    let val = edge.get_attr("w:val").unwrap_or("none");
    !matches!(val, "none" | "nil" | "")
}

fn count_visible_border_edges(borders: &XmlElement) -> usize {
    const EDGE_TAGS: &[&str] = &[
        "w:top",
        "w:left",
        "w:bottom",
        "w:right",
        "w:insideH",
        "w:insideV",
        "w:start",
        "w:end",
    ];
    borders
        .children_elements()
        .filter(|e| EDGE_TAGS.contains(&e.name.as_str()) && is_border_visible(e))
        .count()
}

fn has_visible_grid_borders(tbl: &XmlElement) -> bool {
    let tbl_pr = tbl.child("w:tblPr");
    let tbl_borders = tbl_pr.and_then(|tp| tp.child("w:tblBorders"));

    if let Some(tb) = tbl_borders {
        let has_inside = tb
            .child("w:insideH")
            .map(is_border_visible)
            .unwrap_or(false)
            || tb
                .child("w:insideV")
                .map(is_border_visible)
                .unwrap_or(false);
        if has_inside || count_visible_border_edges(tb) >= 2 {
            return true;
        }
    }

    let mut cells_with_multi_borders = 0usize;
    let mut cells_with_explicit_none = 0usize;
    let mut total_cells = 0usize;

    for node in &tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                for cell_node in &tr.children {
                    if let XmlNode::Element(tc) = cell_node {
                        if tc.name == "w:tc" {
                            total_cells += 1;
                            if let Some(tc_b) = tc.child("w:tcPr").and_then(|tp| tp.child("w:tcBorders")) {
                                let vis = count_visible_border_edges(tc_b);
                                if vis >= 3 {
                                    return true;
                                }
                                if vis >= 2 {
                                    cells_with_multi_borders += 1;
                                }
                                if vis == 0 && tc_b.children_elements().next().is_some() {
                                    cells_with_explicit_none += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if cells_with_multi_borders >= 2 {
        return true;
    }

    if tbl_borders.is_none() && total_cells > 0 && cells_with_explicit_none < total_cells {
        let is_grid_style = tbl_pr
            .and_then(|tp| tp.child("w:tblStyle"))
            .and_then(|s| s.get_attr("w:val"))
            .map(|v| v.to_ascii_lowercase().contains("grid"))
            .unwrap_or(false);
        if is_grid_style {
            return true;
        }
    }

    false
}

fn format_national_header_table(
    tbl: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    metrics: SectionMetrics,
    para_idx: &mut usize,
    stats: &mut FormatStats,
) {
    // 1. Protect and pin fonts/sizes on every cell paragraph, remove tblHeader, add cantSplit
    for node in &mut tbl.children {
        let XmlNode::Element(tr) = node else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        let tr_pr = tr.ensure_child_first("w:trPr");
        tr_pr.remove_children_by_name("w:tblHeader");
        tr_pr.ensure_child_ordered("w:cantSplit", TR_PR_ORDER);

        for cell_node in &mut tr.children {
            let XmlNode::Element(tc) = cell_node else {
                continue;
            };
            if tc.name != "w:tc" {
                continue;
            }
            for p_node in &mut tc.children {
                let XmlNode::Element(p) = p_node else {
                    continue;
                };
                if p.name != "w:p" {
                    continue;
                }
                *para_idx += 1;
                let centered_or_right = paragraph_has_center_or_right_jc(p);
                {
                    let p_pr = p.ensure_child_first("w:pPr");
                    p_pr.remove_children_by_name("w:outlineLvl");
                    if centered_or_right {
                        if let Some(ind) = p_pr.child_mut("w:ind") {
                            if ind.get_attr("w:firstLine").is_some()
                                || ind.get_attr("w:firstLineChars").is_some()
                            {
                                ind.set_attr("w:firstLine", "0");
                                ind.remove_attr("w:firstLineChars");
                            }
                        }
                    }
                    if let Some(spacing) = p_pr.child_mut("w:spacing") {
                        spacing.set_attr("w:after", "0");
                        spacing.set_attr("w:line", "240");
                        spacing.set_attr("w:lineRule", "auto");
                    } else {
                        let spacing = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
                        spacing.set_attr("w:before", "0");
                        spacing.set_attr("w:after", "0");
                        spacing.set_attr("w:line", "240");
                        spacing.set_attr("w:lineRule", "auto");
                    }
                }
                let role = detect_admin_text_role(p, true);
                pin_protected_paragraph_fonts_and_sizes(p, &cfg.font_name, orig_styles, role, stats);
                stats.paragraphs_formatted += 1;
                stats.admin_blocks_protected += 1;
            }
        }
    }

    // 2. Analyze column layout (support standard 2-col and 3-col with empty middle spacer)
    let col_widths = read_table_col_widths_twips(tbl, metrics.printable_width_twips);
    let num_cols = col_widths.len();
    if num_cols < 2 || num_cols > 3 {
        // Fallback: only shrink table if wider than printable width
        if let Some(total_w) = read_table_total_width_twips(tbl, &col_widths, metrics.printable_width_twips) {
            if total_w > metrics.printable_width_twips {
                scale_table_widths_to_fit(tbl, &col_widths, metrics.printable_width_twips);
            }
        }
        return;
    }

    let (left_col_idx, mid_empty_idx, right_col_idx) = if num_cols == 2 {
        (0usize, None, 1usize)
    } else if is_column_empty(tbl, 1) {
        (0usize, Some(1usize), 2usize)
    } else {
        // 3 columns all containing text: only scale if total overflows
        if let Some(total_w) = read_table_total_width_twips(tbl, &col_widths, metrics.printable_width_twips) {
            if total_w > metrics.printable_width_twips {
                scale_table_widths_to_fit(tbl, &col_widths, metrics.printable_width_twips);
            }
        }
        return;
    };

    let (tbl_mar_l, tbl_mar_r) = read_tbl_horiz_margins(tbl);
    let current_left_pad = max_col_horiz_pad(tbl, left_col_idx, tbl_mar_l, tbl_mar_r);
    let current_right_pad = max_col_horiz_pad(tbl, right_col_idx, tbl_mar_l, tbl_mar_r);

    let req_left_raw = measure_column_max_line_twips(tbl, left_col_idx, orig_styles) + 35;
    let req_right_raw = measure_column_max_line_twips(tbl, right_col_idx, orig_styles) + 35;

    let current_total_w = read_table_total_width_twips(tbl, &col_widths, metrics.printable_width_twips)
        .unwrap_or_else(|| col_widths.iter().sum());
    let grid_sum: i32 = col_widths.iter().sum();
    let tbl_w_is_pct = tbl
        .child("w:tblPr")
        .and_then(|tp| tp.child("w:tblW"))
        .and_then(|w| w.get_attr("w:type"))
        == Some("pct");
    let effective_col_widths: Vec<i32> = if tbl_w_is_pct && grid_sum > 0 && current_total_w > 0 {
        col_widths
            .iter()
            .map(|&w| ((w as f64 / grid_sum as f64) * current_total_w as f64).round() as i32)
            .collect()
    } else {
        col_widths.clone()
    };
    let tbl_ind_twips = tbl
        .child("w:tblPr")
        .and_then(|tp| tp.child("w:tblInd"))
        .and_then(|ti| ti.get_attr("w:w"))
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let floating_x_oob = tbl
        .child("w:tblPr")
        .and_then(|tp| tp.child("w:tblpPr"))
        .filter(|p| p.get_attr("w:horzAnchor") == Some("page"))
        .and_then(|p| p.get_attr("w:tblpX"))
        .and_then(|v| v.parse::<i32>().ok())
        .map(|x| x < metrics.margin_left_twips)
        .unwrap_or(false);

    let left_col_w = effective_col_widths.get(left_col_idx).copied().unwrap_or(0);
    let right_col_w = effective_col_widths.get(right_col_idx).copied().unwrap_or(0);

    let needs_adjustment = current_total_w > metrics.printable_width_twips + 40
        || grid_sum > metrics.printable_width_twips + 40
        || tbl_ind_twips < 0
        || floating_x_oob
        || (left_col_w - current_left_pad) < req_left_raw
        || (right_col_w - current_right_pad) < req_right_raw;

    if !needs_adjustment {
        return;
    }

    // 3. Apply Method A: zero horizontal cell margins so 12pt ND30 fits in 16.0 cm (9071 twips)
    zero_table_horiz_cell_margins(tbl);

    // Clear positive left/right indents on centered paragraphs in this table if any
    clear_center_horiz_indents_in_table(tbl);

    let mut req_left = measure_column_max_line_twips(tbl, left_col_idx, orig_styles) + 35;
    let mut req_right = measure_column_max_line_twips(tbl, right_col_idx, orig_styles) + 35;

    let gap_mid = if mid_empty_idx.is_some() { 20 } else { 0 };
    let usable_width = (metrics.printable_width_twips - gap_mid).max(3000);

    // 4. Cascade step A: downscale oversized uppercase headers (>12.5pt -> 12.5pt -> 12pt)
    // and oversized motto/date (>13pt -> 13pt) when sum exceeds usable width
    if req_left + req_right > usable_width {
        cap_national_header_sizes(tbl, orig_styles, 12.5, 13.0);
        req_left = measure_column_max_line_twips(tbl, left_col_idx, orig_styles) + 35;
        req_right = measure_column_max_line_twips(tbl, right_col_idx, orig_styles) + 35;
    }

    if req_left + req_right > usable_width {
        cap_national_header_sizes(tbl, orig_styles, 12.0, 13.0);
        req_left = measure_column_max_line_twips(tbl, left_col_idx, orig_styles) + 35;
        req_right = measure_column_max_line_twips(tbl, right_col_idx, orig_styles) + 35;
    }

    // If right column date line still exceeds motto/Quoc hieu at 13pt and blocks fit, allow date to 12pt
    if req_left + req_right > usable_width {
        cap_date_line_size(tbl, 12.0);
        req_right = measure_column_max_line_twips(tbl, right_col_idx, orig_styles) + 35;
    }

    // 5. Cascade step B: fallback for ultra-long agency names or narrow pages
    let max_right = (usable_width as f64 * 0.62).round() as i32;
    if req_right > max_right {
        fit_oversized_column_paragraphs(tbl, right_col_idx, max_right, orig_styles);
        req_right = measure_column_max_line_twips(tbl, right_col_idx, orig_styles) + 35;
    }

    let avail_left = (usable_width - req_right.min(max_right)).max(1500);
    if req_left > avail_left {
        fit_oversized_column_paragraphs(tbl, left_col_idx, avail_left, orig_styles);
        req_left = measure_column_max_line_twips(tbl, left_col_idx, orig_styles) + 35;
    }

    // 6. Step C: allocate column widths totaling exactly printable_width_twips
    let min_right = req_right.min(max_right);
    let min_left = req_left.min(usable_width - min_right);
    let ideal_right = (usable_width as f64 * 0.56).round() as i32;

    let (col_left, col_right) = if min_left + min_right <= usable_width {
        let slack = usable_width - min_left - min_right;
        let max_allowed_right = (usable_width - min_left).min(max_right);
        if slack <= 60 {
            let r = (min_right + slack / 2).clamp(min_right, max_allowed_right);
            (usable_width - r, r)
        } else {
            let lo = (min_right + 20).min(max_allowed_right);
            let hi = (usable_width - min_left - 20).min(max_right).max(lo);
            let r = ideal_right.clamp(lo, hi);
            (usable_width - r, r)
        }
    } else {
        let r = min_right;
        (usable_width - r, r)
    };

    let target_widths = if mid_empty_idx.is_some() {
        vec![col_left, gap_mid, col_right]
    } else {
        vec![col_left, col_right]
    };

    apply_fixed_table_widths(tbl, &target_widths, metrics);
}

fn format_signature_admin_table(
    tbl: &mut XmlElement,
    cfg: &FormatConfig,
    orig_styles: &OriginalStyleSizes,
    metrics: SectionMetrics,
    para_idx: &mut usize,
    stats: &mut FormatStats,
) {
    for node in &mut tbl.children {
        let XmlNode::Element(tr) = node else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        let tr_pr = tr.ensure_child_first("w:trPr");
        tr_pr.remove_children_by_name("w:tblHeader");
        tr_pr.ensure_child_ordered("w:cantSplit", TR_PR_ORDER);

        for cell_node in &mut tr.children {
            let XmlNode::Element(tc) = cell_node else {
                continue;
            };
            if tc.name != "w:tc" {
                continue;
            }
            let cell_upper = collapse_spaces_upper(&tc.extract_text());
            let is_recipient_cell = cell_upper.contains("NƠI NHẬN");

            for p_node in &mut tc.children {
                let XmlNode::Element(p) = p_node else {
                    continue;
                };
                if p.name != "w:p" {
                    continue;
                }
                *para_idx += 1;
                if paragraph_has_center_or_right_jc(p) {
                    if let Some(p_pr) = p.child_mut("w:pPr") {
                        if let Some(ind) = p_pr.child_mut("w:ind") {
                            if ind.get_attr("w:firstLine").is_some()
                                || ind.get_attr("w:firstLineChars").is_some()
                            {
                                ind.set_attr("w:firstLine", "0");
                                ind.remove_attr("w:firstLineChars");
                            }
                        }
                    }
                }
                let p_upper = collapse_spaces_upper(&p.extract_text());
                let role = if is_recipient_cell
                    && !p_upper.starts_with("NƠI NHẬN")
                    && !p_upper.contains("TM.")
                    && !p_upper.contains("KT.")
                {
                    AdminTextRole::Recipient
                } else if p_upper.contains("NƠI NHẬN") {
                    AdminTextRole::Recipient
                } else {
                    AdminTextRole::GeneralAdmin
                };
                pin_protected_paragraph_fonts_and_sizes(p, &cfg.font_name, orig_styles, role, stats);
                stats.paragraphs_formatted += 1;
                stats.admin_blocks_protected += 1;
            }
        }
    }

    let col_widths = read_table_col_widths_twips(tbl, metrics.printable_width_twips);
    if let Some(total_w) = read_table_total_width_twips(tbl, &col_widths, metrics.printable_width_twips) {
        if total_w > metrics.printable_width_twips {
            scale_table_widths_to_fit(tbl, &col_widths, metrics.printable_width_twips);
        }
    }
}

fn read_table_col_widths_twips(tbl: &XmlElement, printable_width_twips: i32) -> Vec<i32> {
    if let Some(grid) = tbl.child("w:tblGrid") {
        let cols: Vec<i32> = grid
            .children_elements()
            .filter(|e| e.name == "w:gridCol")
            .filter_map(|e| e.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()))
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }

    for node in &tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                let mut cols = Vec::new();
                for c_node in &tr.children {
                    if let XmlNode::Element(tc) = c_node {
                        if tc.name == "w:tc" {
                            let span = tc
                                .child("w:tcPr")
                                .and_then(|tp| tp.child("w:gridSpan"))
                                .and_then(|gs| gs.get_attr("w:val"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .max(1);
                            let w = tc
                                .child("w:tcPr")
                                .and_then(|tp| tp.child("w:tcW"))
                                .map(|tw| parse_width_twips(tw, printable_width_twips))
                                .flatten()
                                .unwrap_or(printable_width_twips / 2);
                            let per_col = w / span as i32;
                            for _ in 0..span {
                                cols.push(per_col);
                            }
                        }
                    }
                }
                if !cols.is_empty() {
                    return cols;
                }
            }
        }
    }

    Vec::new()
}

fn parse_width_twips(w_el: &XmlElement, ref_width_twips: i32) -> Option<i32> {
    let val = w_el.get_attr("w:w")?.parse::<i32>().ok()?;
    let w_type = w_el.get_attr("w:type").unwrap_or("dxa");
    match w_type {
        "dxa" => Some(val),
        "pct" => Some((val as f64 / 5000.0 * ref_width_twips as f64).round() as i32),
        _ => None,
    }
}

fn read_table_total_width_twips(
    tbl: &XmlElement,
    col_widths: &[i32],
    printable_width_twips: i32,
) -> Option<i32> {
    if let Some(tbl_w) = tbl.child("w:tblPr").and_then(|tp| tp.child("w:tblW")) {
        if let Some(w) = parse_width_twips(tbl_w, printable_width_twips) {
            if w > 0 {
                return Some(w);
            }
        }
    }
    if !col_widths.is_empty() {
        return Some(col_widths.iter().sum());
    }
    None
}

fn is_column_empty(tbl: &XmlElement, target_col: usize) -> bool {
    for node in &tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                let mut cur_col = 0usize;
                for c_node in &tr.children {
                    if let XmlNode::Element(tc) = c_node {
                        if tc.name == "w:tc" {
                            let span = tc
                                .child("w:tcPr")
                                .and_then(|tp| tp.child("w:gridSpan"))
                                .and_then(|gs| gs.get_attr("w:val"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .max(1);
                            if span == 1 && cur_col == target_col && !tc.extract_text().trim().is_empty() {
                                return false;
                            }
                            cur_col += span;
                        }
                    }
                }
            }
        }
    }
    true
}

fn read_tbl_horiz_margins(tbl: &XmlElement) -> (i32, i32) {
    let Some(mar) = tbl.child("w:tblPr").and_then(|tp| tp.child("w:tblCellMar")) else {
        return (108, 108);
    };
    let left = mar
        .child("w:left")
        .or_else(|| mar.child("w:start"))
        .and_then(|e| e.get_attr("w:w"))
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(108);
    let right = mar
        .child("w:right")
        .or_else(|| mar.child("w:end"))
        .and_then(|e| e.get_attr("w:w"))
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(108);
    (left, right)
}

fn max_col_horiz_pad(tbl: &XmlElement, target_col: usize, tbl_l: i32, tbl_r: i32) -> i32 {
    let mut max_pad = 0i32;
    for node in &tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                let mut cur_col = 0usize;
                for c_node in &tr.children {
                    if let XmlNode::Element(tc) = c_node {
                        if tc.name == "w:tc" {
                            let span = tc
                                .child("w:tcPr")
                                .and_then(|tp| tp.child("w:gridSpan"))
                                .and_then(|gs| gs.get_attr("w:val"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .max(1);
                            if span == 1 && cur_col == target_col {
                                let tc_mar = tc.child("w:tcPr").and_then(|tp| tp.child("w:tcMar"));
                                let l = tc_mar
                                    .and_then(|m| m.child("w:left").or_else(|| m.child("w:start")))
                                    .and_then(|e| e.get_attr("w:w"))
                                    .and_then(|v| v.parse::<i32>().ok())
                                    .unwrap_or(tbl_l);
                                let r = tc_mar
                                    .and_then(|m| m.child("w:right").or_else(|| m.child("w:end")))
                                    .and_then(|e| e.get_attr("w:w"))
                                    .and_then(|v| v.parse::<i32>().ok())
                                    .unwrap_or(tbl_r);
                                max_pad = max_pad.max(l + r);
                            }
                            cur_col += span;
                        }
                    }
                }
            }
        }
    }
    max_pad
}

fn zero_table_horiz_cell_margins(tbl: &mut XmlElement) {
    let tbl_pr = tbl.ensure_child_first("w:tblPr");
    let mar = tbl_pr.ensure_child_ordered("w:tblCellMar", TBL_PR_ORDER);
    let left_el = mar.ensure_child_ordered("w:left", TBL_CELL_MAR_ORDER);
    left_el.set_attr("w:w", "0");
    left_el.set_attr("w:type", "dxa");
    if let Some(start_el) = mar.child_mut("w:start") {
        start_el.set_attr("w:w", "0");
        start_el.set_attr("w:type", "dxa");
    }
    let right_el = mar.ensure_child_ordered("w:right", TBL_CELL_MAR_ORDER);
    right_el.set_attr("w:w", "0");
    right_el.set_attr("w:type", "dxa");
    if let Some(end_el) = mar.child_mut("w:end") {
        end_el.set_attr("w:w", "0");
        end_el.set_attr("w:type", "dxa");
    }

    for node in &mut tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                for c_node in &mut tr.children {
                    if let XmlNode::Element(tc) = c_node {
                        if tc.name == "w:tc" {
                            if let Some(tc_mar) =
                                tc.child_mut("w:tcPr").and_then(|tp| tp.child_mut("w:tcMar"))
                            {
                                for tag in ["w:left", "w:start", "w:right", "w:end"] {
                                    if let Some(el) = tc_mar.child_mut(tag) {
                                        el.set_attr("w:w", "0");
                                        el.set_attr("w:type", "dxa");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn clear_center_horiz_indents_in_table(tbl: &mut XmlElement) {
    for node in &mut tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                for c_node in &mut tr.children {
                    if let XmlNode::Element(tc) = c_node {
                        if tc.name == "w:tc" {
                            for p_node in &mut tc.children {
                                if let XmlNode::Element(p) = p_node {
                                    if p.name == "w:p" && paragraph_has_center_or_right_jc(p) {
                                        if let Some(ind) =
                                            p.child_mut("w:pPr").and_then(|pp| pp.child_mut("w:ind"))
                                        {
                                            ind.remove_attr("w:left");
                                            ind.remove_attr("w:right");
                                            ind.remove_attr("w:start");
                                            ind.remove_attr("w:end");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn measure_column_max_line_twips(
    tbl: &XmlElement,
    target_col: usize,
    orig_styles: &OriginalStyleSizes,
) -> i32 {
    let mut max_w = 0i32;
    for node in &tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                let mut cur_col = 0usize;
                for c_node in &tr.children {
                    if let XmlNode::Element(tc) = c_node {
                        if tc.name == "w:tc" {
                            let span = tc
                                .child("w:tcPr")
                                .and_then(|tp| tp.child("w:gridSpan"))
                                .and_then(|gs| gs.get_attr("w:val"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .max(1);
                            if span == 1 && cur_col == target_col {
                                max_w = max_w.max(estimate_cell_max_line_twips(tc, orig_styles));
                            }
                            cur_col += span;
                        }
                    }
                }
            }
        }
    }
    max_w
}

fn estimate_cell_max_line_twips(tc: &XmlElement, orig_styles: &OriginalStyleSizes) -> i32 {
    let mut max_w = 0i32;
    for node in &tc.children {
        if let XmlNode::Element(p) = node {
            if p.name == "w:p" {
                max_w = max_w.max(estimate_paragraph_max_line_twips(p, orig_styles));
            }
        }
    }
    max_w
}

fn estimate_paragraph_max_line_twips(p: &XmlElement, orig_styles: &OriginalStyleSizes) -> i32 {
    let p_style = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"));

    let mut max_line = 0.0f64;
    let mut cur_line = 0.0f64;

    collect_line_widths(p, p_style, orig_styles, &mut cur_line, &mut max_line);
    max_line = max_line.max(cur_line);

    let extra_ind = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:ind"))
        .map(|ind| {
            let l = ind
                .get_attr("w:left")
                .or_else(|| ind.get_attr("w:start"))
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0)
                .max(0);
            let r = ind
                .get_attr("w:right")
                .or_else(|| ind.get_attr("w:end"))
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0)
                .max(0);
            l + r
        })
        .unwrap_or(0);

    (max_line.ceil() as i32) + extra_ind
}

fn collect_line_widths(
    el: &XmlElement,
    p_style: Option<&str>,
    orig_styles: &OriginalStyleSizes,
    cur_line: &mut f64,
    max_line: &mut f64,
) {
    for node in &el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:r" => {
                let r_pr = child.child("w:rPr");
                let r_style = r_pr
                    .and_then(|r| r.child("w:rStyle"))
                    .and_then(|s| s.get_attr("w:val"));
                let sz_pt = r_pr
                    .and_then(read_rpr_sz_pt)
                    .or_else(|| orig_styles.resolve_pt(p_style, r_style))
                    .unwrap_or(12.5);
                let is_bold = r_pr
                    .and_then(|r| read_rpr_flag(r, "w:b"))
                    .unwrap_or_else(|| orig_styles.resolve_bold(p_style, r_style));
                let is_caps = r_pr
                    .and_then(|r| read_rpr_flag(r, "w:caps"))
                    .unwrap_or_else(|| orig_styles.resolve_caps(p_style, r_style));
                let scale_pct = r_pr
                    .and_then(|r| r.child("w:w"))
                    .and_then(|w| w.get_attr("w:val"))
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(100.0)
                    .clamp(50.0, 200.0);

                let em_twips = sz_pt * 20.0 * (scale_pct / 100.0);

                for r_node in &child.children {
                    let XmlNode::Element(r_sub) = r_node else {
                        continue;
                    };
                    match r_sub.name.as_str() {
                        "w:t" => {
                            let txt = r_sub.extract_text();
                            for ch in txt.chars() {
                                if is_caps {
                                    for up in ch.to_uppercase() {
                                        *cur_line += glyph_width_thousandths(up, is_bold) / 1000.0 * em_twips;
                                    }
                                } else {
                                    *cur_line += glyph_width_thousandths(ch, is_bold) / 1000.0 * em_twips;
                                }
                            }
                        }
                        "w:tab" => {
                            *cur_line += 720.0;
                        }
                        "w:br" | "w:cr" => {
                            if *cur_line > *max_line {
                                *max_line = *cur_line;
                            }
                            *cur_line = 0.0;
                        }
                        _ => {}
                    }
                }
            }
            "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                collect_line_widths(child, p_style, orig_styles, cur_line, max_line);
            }
            _ => {}
        }
    }
}

pub(crate) fn estimate_text_line_twips(text: &str, size_pt: f64, is_bold: bool, all_caps: bool) -> i32 {
    let em_twips = size_pt * 20.0;
    let mut cur = 0.0f64;
    let mut max_w = 0.0f64;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            if cur > max_w {
                max_w = cur;
            }
            cur = 0.0;
            continue;
        }
        if all_caps {
            for up in ch.to_uppercase() {
                cur += glyph_width_thousandths(up, is_bold) / 1000.0 * em_twips;
            }
        } else {
            cur += glyph_width_thousandths(ch, is_bold) / 1000.0 * em_twips;
        }
    }
    max_w.max(cur).ceil() as i32
}

fn glyph_width_thousandths(ch: char, is_bold: bool) -> f64 {
    if ch == ' ' || ch == '\u{00A0}' {
        return 250.0;
    }

    let upper_base = match ch {
        'I' | 'Ì' | 'Í' | 'Ỉ' | 'Ĩ' | 'Ị' => Some(389.0),
        'J' => Some(500.0),
        'S' => Some(556.0),
        'F' | 'P' => Some(611.0),
        'B' | 'E' | 'È' | 'É' | 'Ẻ' | 'Ẽ' | 'Ẹ' | 'Ê' | 'Ề' | 'Ế' | 'Ể' | 'Ễ' | 'Ệ'
        | 'L' | 'T' | 'Z' => Some(667.0),
        'A' | 'À' | 'Á' | 'Ả' | 'Ã' | 'Ạ' | 'Ă' | 'Ằ' | 'Ắ' | 'Ẳ' | 'Ẵ' | 'Ặ' | 'Â'
        | 'Ầ' | 'Ấ' | 'Ẩ' | 'Ẫ' | 'Ậ' | 'C' | 'D' | 'Đ' | 'N' | 'R' | 'U' | 'Ù' | 'Ú'
        | 'Ủ' | 'Ũ' | 'Ụ' | 'V' | 'X' | 'Y' | 'Ỳ' | 'Ý' | 'Ỷ' | 'Ỹ' | 'Ỵ' => Some(722.0),
        'G' | 'H' | 'K' | 'O' | 'Ò' | 'Ó' | 'Ỏ' | 'Õ' | 'Ọ' | 'Ô' | 'Ồ' | 'Ố' | 'Ổ'
        | 'Ỗ' | 'Ộ' | 'Q' => Some(778.0),
        'Ư' | 'Ừ' | 'Ứ' | 'Ử' | 'Ữ' | 'Ự' => Some(790.0),
        'Ơ' | 'Ờ' | 'Ớ' | 'Ở' | 'Ỡ' | 'Ợ' => Some(810.0),
        'M' => Some(944.0),
        'W' => Some(1000.0),
        _ => None,
    };

    if let Some(w) = upper_base {
        return if is_bold { w } else { w * 0.95 };
    }

    match ch {
        '0'..='9' => 500.0,
        'i' | 'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' | 'l' | 'j' => 333.0,
        't' | 'f' | 'r' => 389.0,
        'm' | 'w' => 778.0,
        'ơ' | 'ờ' | 'ớ' | 'ở' | 'ỡ' | 'ợ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => {
            if is_bold { 560.0 } else { 540.0 }
        }
        '-' | '–' | '—' | '/' | '.' | ',' | ':' | ';' | '(' | ')' => 333.0,
        _ if ch.is_alphabetic() => {
            if is_bold { 525.0 } else { 500.0 }
        }
        _ => 450.0,
    }
}

fn is_uppercase_header_para(p: &XmlElement, orig_styles: &OriginalStyleSizes) -> bool {
    let text = p.extract_text();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let upper = collapse_spaces_upper(trimmed);
    if upper.starts_with("SỐ:")
        || upper.starts_with("SỐ :")
        || (upper.contains("NGÀY ") && upper.contains("THÁNG "))
        || upper.contains("ĐỘC LẬP - TỰ DO")
        || upper.contains("ĐỘC LẬP – TỰ DO")
        || upper.contains("ĐỘC LẬP-TỰ DO")
    {
        return false;
    }

    let p_style = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"));
    if orig_styles.resolve_caps(p_style, None) {
        return true;
    }

    let mut alpha = 0usize;
    let mut upper_cnt = 0usize;
    for ch in trimmed.chars() {
        if ch.is_alphabetic() {
            alpha += 1;
            if ch.is_uppercase() {
                upper_cnt += 1;
            }
        }
    }
    alpha > 0 && upper_cnt * 10 >= alpha * 8
}

fn cap_paragraph_runs_max_pt(p: &mut XmlElement, max_pt: f64) {
    let max_half = pt_to_half_pts(max_pt);
    cap_runs_recursive(p, max_pt, max_half);
    if let Some(r_pr) = p.child_mut("w:pPr").and_then(|pp| pp.child_mut("w:rPr")) {
        if read_rpr_sz_pt(r_pr).map(|pt| pt > max_pt).unwrap_or(false) {
            let sz = r_pr.ensure_child_ordered("w:sz", R_PR_ORDER);
            sz.set_attr("w:val", &max_half.to_string());
            let sz_cs = r_pr.ensure_child_ordered("w:szCs", R_PR_ORDER);
            sz_cs.set_attr("w:val", &max_half.to_string());
        }
    }
}

fn cap_runs_recursive(el: &mut XmlElement, max_pt: f64, max_half: u32) {
    for node in &mut el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:r" => {
                let is_script = child
                    .child("w:rPr")
                    .and_then(|r| r.child("w:vertAlign"))
                    .is_some();
                if is_script {
                    continue;
                }
                let r_pr = child.ensure_child_first("w:rPr");
                if read_rpr_sz_pt(r_pr).map(|pt| pt > max_pt).unwrap_or(true) {
                    let sz = r_pr.ensure_child_ordered("w:sz", R_PR_ORDER);
                    sz.set_attr("w:val", &max_half.to_string());
                    let sz_cs = r_pr.ensure_child_ordered("w:szCs", R_PR_ORDER);
                    sz_cs.set_attr("w:val", &max_half.to_string());
                }
            }
            "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                cap_runs_recursive(child, max_pt, max_half);
            }
            _ => {}
        }
    }
}

fn set_paragraph_runs_width_scale(p: &mut XmlElement, scale_pct: Option<i32>) {
    set_scale_recursive(p, scale_pct);
}

fn set_scale_recursive(el: &mut XmlElement, scale_pct: Option<i32>) {
    for node in &mut el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:r" => {
                let r_pr = child.ensure_child_first("w:rPr");
                if let Some(pct) = scale_pct {
                    let w_el = r_pr.ensure_child_ordered("w:w", R_PR_ORDER);
                    w_el.set_attr("w:val", &pct.to_string());
                } else {
                    r_pr.remove_children_by_name("w:w");
                }
            }
            "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                set_scale_recursive(child, scale_pct);
            }
            _ => {}
        }
    }
}

fn cap_national_header_sizes(
    tbl: &mut XmlElement,
    orig_styles: &OriginalStyleSizes,
    max_caps_pt: f64,
    max_motto_date_pt: f64,
) {
    for node in &mut tbl.children {
        let XmlNode::Element(tr) = node else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        for c_node in &mut tr.children {
            let XmlNode::Element(tc) = c_node else {
                continue;
            };
            if tc.name != "w:tc" {
                continue;
            }
            for p_node in &mut tc.children {
                let XmlNode::Element(p) = p_node else {
                    continue;
                };
                if p.name != "w:p" {
                    continue;
                }
                let upper = collapse_spaces_upper(&p.extract_text());
                if is_uppercase_header_para(p, orig_styles) {
                    cap_paragraph_runs_max_pt(p, max_caps_pt);
                } else if upper.contains("ĐỘC LẬP - TỰ DO")
                    || upper.contains("ĐỘC LẬP – TỰ DO")
                    || upper.contains("ĐỘC LẬP-TỰ DO")
                    || (upper.contains("NGÀY ") && upper.contains("THÁNG "))
                {
                    cap_paragraph_runs_max_pt(p, max_motto_date_pt);
                }
            }
        }
    }
}

fn cap_date_line_size(tbl: &mut XmlElement, max_pt: f64) {
    for node in &mut tbl.children {
        let XmlNode::Element(tr) = node else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        for c_node in &mut tr.children {
            let XmlNode::Element(tc) = c_node else {
                continue;
            };
            if tc.name != "w:tc" {
                continue;
            }
            for p_node in &mut tc.children {
                let XmlNode::Element(p) = p_node else {
                    continue;
                };
                if p.name == "w:p" {
                    let upper = collapse_spaces_upper(&p.extract_text());
                    if upper.contains("NGÀY ") && upper.contains("THÁNG ") {
                        cap_paragraph_runs_max_pt(p, max_pt);
                    }
                }
            }
        }
    }
}

fn fit_oversized_column_paragraphs(
    tbl: &mut XmlElement,
    target_col: usize,
    budget_twips: i32,
    orig_styles: &OriginalStyleSizes,
) {
    let inner_budget = (budget_twips - 35).max(800);
    for node in &mut tbl.children {
        let XmlNode::Element(tr) = node else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        let mut cur_col = 0usize;
        for c_node in &mut tr.children {
            let XmlNode::Element(tc) = c_node else {
                continue;
            };
            if tc.name != "w:tc" {
                continue;
            }
            let span = tc
                .child("w:tcPr")
                .and_then(|tp| tp.child("w:gridSpan"))
                .and_then(|gs| gs.get_attr("w:val"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            if span == 1 && cur_col == target_col {
                for p_node in &mut tc.children {
                    let XmlNode::Element(p) = p_node else {
                        continue;
                    };
                    if p.name != "w:p" {
                        continue;
                    }
                    let w_cur = estimate_paragraph_max_line_twips(p, orig_styles);
                    if w_cur > inner_budget {
                        let scale = ((inner_budget as f64 / w_cur as f64) * 100.0).floor() as i32;
                        if (93..=99).contains(&scale) {
                            set_paragraph_runs_width_scale(p, Some(scale));
                        } else if scale < 93 {
                            // Ultra-long line: step down to 11.5pt cleanly, re-check scale
                            cap_paragraph_runs_max_pt(p, 11.5);
                            set_paragraph_runs_width_scale(p, None);
                            let w_115 = estimate_paragraph_max_line_twips(p, orig_styles);
                            if w_115 > inner_budget {
                                let scale_115 =
                                    ((inner_budget as f64 / w_115 as f64) * 100.0).floor() as i32;
                                if scale_115 >= 93 {
                                    set_paragraph_runs_width_scale(p, Some(scale_115));
                                } else {
                                    cap_paragraph_runs_max_pt(p, 11.0);
                                    let w_11 = estimate_paragraph_max_line_twips(p, orig_styles);
                                    if w_11 > inner_budget {
                                        let scale_11 = ((inner_budget as f64 / w_11 as f64) * 100.0)
                                            .floor()
                                            .clamp(85.0, 99.0) as i32;
                                        set_paragraph_runs_width_scale(p, Some(scale_11));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            cur_col += span;
        }
    }
}

fn apply_fixed_table_widths(
    tbl: &mut XmlElement,
    col_widths: &[i32],
    metrics: SectionMetrics,
) {
    let total_w: i32 = col_widths.iter().sum();
    let tbl_pr = tbl.ensure_child_first("w:tblPr");

    if let Some(p_pr) = tbl_pr.child_mut("w:tblpPr") {
        if p_pr.get_attr("w:horzAnchor") == Some("page") {
            if let Some(x) = p_pr.get_attr("w:tblpX").and_then(|v| v.parse::<i32>().ok()) {
                if x < metrics.margin_left_twips {
                    p_pr.set_attr("w:tblpX", &metrics.margin_left_twips.to_string());
                }
            }
        }
    }

    let tbl_w = tbl_pr.ensure_child_ordered("w:tblW", TBL_PR_ORDER);
    tbl_w.set_attr("w:w", &total_w.to_string());
    tbl_w.set_attr("w:type", "dxa");

    let tbl_ind = tbl_pr.ensure_child_ordered("w:tblInd", TBL_PR_ORDER);
    tbl_ind.set_attr("w:w", "0");
    tbl_ind.set_attr("w:type", "dxa");

    let tbl_layout = tbl_pr.ensure_child_ordered("w:tblLayout", TBL_PR_ORDER);
    tbl_layout.set_attr("w:type", "fixed");

    update_grid_and_cell_widths(tbl, col_widths, false);
}

fn update_grid_and_cell_widths(
    tbl: &mut XmlElement,
    col_widths: &[i32],
    only_explicit_dxa_cells: bool,
) {
    let grid = tbl.ensure_child_ordered("w:tblGrid", &["w:tblPr", "w:tblGrid", "w:tr"]);
    grid.children.clear();
    for &w in col_widths {
        let mut gc = XmlElement::new("w:gridCol");
        gc.set_attr("w:w", &w.to_string());
        grid.children.push(XmlNode::Element(gc));
    }

    for node in &mut tbl.children {
        let XmlNode::Element(tr) = node else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        let mut cur_col = 0usize;
        for c_node in &mut tr.children {
            let XmlNode::Element(tc) = c_node else {
                continue;
            };
            if tc.name != "w:tc" {
                continue;
            }
            let span = tc
                .child("w:tcPr")
                .and_then(|tp| tp.child("w:gridSpan"))
                .and_then(|gs| gs.get_attr("w:val"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let end_col = (cur_col + span).min(col_widths.len());
            let cell_w: i32 = if cur_col < end_col {
                col_widths[cur_col..end_col].iter().sum()
            } else {
                0
            };
            if cell_w > 0 {
                if only_explicit_dxa_cells {
                    if let Some(tc_w) = tc
                        .child_mut("w:tcPr")
                        .and_then(|tp| tp.child_mut("w:tcW"))
                    {
                        if tc_w.get_attr("w:type").unwrap_or("dxa") == "dxa" {
                            tc_w.set_attr("w:w", &cell_w.to_string());
                        }
                    }
                } else {
                    let tc_pr = tc.ensure_child_first("w:tcPr");
                    let tc_w = tc_pr.ensure_child_ordered("w:tcW", TC_PR_ORDER);
                    tc_w.set_attr("w:w", &cell_w.to_string());
                    tc_w.set_attr("w:type", "dxa");
                }
            }
            cur_col += span;
        }
    }
}

fn proportional_scale_widths(col_widths: &[i32], max_width_twips: i32) -> Vec<i32> {
    let sum: i32 = col_widths.iter().sum();
    if sum <= 0 || col_widths.is_empty() {
        return col_widths.to_vec();
    }
    let mut scaled: Vec<i32> = col_widths
        .iter()
        .map(|&w| ((w as f64 / sum as f64) * max_width_twips as f64).round() as i32)
        .collect();
    let scaled_sum: i32 = scaled.iter().sum();
    if let Some(last) = scaled.last_mut() {
        *last = (*last + (max_width_twips - scaled_sum)).max(100);
    }
    scaled
}

fn scale_table_widths_to_fit(tbl: &mut XmlElement, col_widths: &[i32], max_width_twips: i32) {
    if col_widths.is_empty() || col_widths.iter().sum::<i32>() <= 0 {
        return;
    }
    let scaled = proportional_scale_widths(col_widths, max_width_twips);
    let metrics = SectionMetrics {
        printable_width_twips: max_width_twips,
        margin_left_twips: 0,
    };
    apply_fixed_table_widths(tbl, &scaled, metrics);
}

pub fn is_admin_header_table(tbl: &XmlElement) -> bool {
    classify_admin_table(tbl) == AdminTableKind::NationalHeader
}

pub fn is_admin_layout_table(tbl: &XmlElement) -> bool {
    classify_admin_table(tbl) != AdminTableKind::None
}

pub fn collapse_spaces_upper(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// Returns `true` if the trimmed paragraph text begins with structural section
/// numbering or a Vietnamese heading keyword (e.g. `1.`, `2.1.`, `I.`, `PHẦN I`,
/// `ĐIỀU 1`) so administrative headings such as `2. Chủ tịch Hội đồng quản trị`
/// are never misclassified as signature blocks.
pub fn starts_with_heading_numbering(s: &str) -> bool {
    let t = s.trim_start();
    if t.is_empty() {
        return false;
    }
    let upper = t.to_uppercase();
    for kw in ["PHẦN ", "CHƯƠNG ", "MỤC ", "ĐIỀU "] {
        if upper.starts_with(kw) {
            return true;
        }
    }
    let bytes = t.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 {
        while i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && matches!(bytes[i], b'.' | b':' | b')') {
            let rest = &t[i + 1..];
            if rest.starts_with(char::is_whitespace) && !rest.trim().is_empty() {
                return true;
            }
        }
    }
    let mut r = 0usize;
    while r < bytes.len() && matches!(bytes[r], b'I' | b'V' | b'X' | b'L' | b'C' | b'D' | b'M') {
        r += 1;
    }
    if (1..=6).contains(&r) && r < bytes.len() && bytes[r] == b'.' {
        let rest = &t[r + 1..];
        if rest.starts_with(char::is_whitespace) && !rest.trim().is_empty() {
            return true;
        }
    }
    false
}

fn collect_text_with_tabs(el: &XmlElement, buf: &mut String) {
    for node in &el.children {
        match node {
            XmlNode::Text(t) => buf.push_str(t),
            XmlNode::Element(sub) => match sub.name.as_str() {
                "w:drawing" | "w:pict" | "mc:AlternateContent" | "w:instrText" | "w:delText"
                | "w:footnoteReference" | "w:endnoteReference" => {}
                "w:tab" => buf.push('\t'),
                _ => collect_text_with_tabs(sub, buf),
            },
            _ => {}
        }
    }
}

fn has_tab_node(el: &XmlElement) -> bool {
    el.children.iter().any(|n| match n {
        XmlNode::Element(sub) => sub.name == "w:tab" || has_tab_node(sub),
        _ => false,
    })
}

fn has_caps_formatting(el: &XmlElement) -> bool {
    el.children.iter().any(|n| match n {
        XmlNode::Element(sub) => {
            (sub.name == "w:caps"
                && sub.get_attr("w:val") != Some("0")
                && sub.get_attr("w:val") != Some("false"))
                || has_caps_formatting(sub)
        }
        _ => false,
    })
}

/// Identifies paragraphs that legitimately open or mark an administrative signature
/// or distribution (`Nơi nhận`) zone, ignoring body prose and numbered headings.
pub fn is_signature_trigger_paragraph(p: &XmlElement, trimmed: &str) -> bool {
    if trimmed.is_empty() || starts_with_heading_numbering(trimmed) {
        return false;
    }

    let upper = trimmed.to_uppercase();
    let char_len = trimmed.chars().count();

    if upper.starts_with("NƠI NHẬN") || upper.contains("NƠI NHẬN:") {
        return true;
    }

    let mut tabbed = String::new();
    collect_text_with_tabs(p, &mut tabbed);
    let upper_tabbed = tabbed.trim().to_uppercase();

    if char_len <= 90
        && (upper.starts_with("TM.")
            || upper.starts_with("KT.")
            || upper.starts_with("TL.")
            || upper.starts_with("TUQ.")
            || upper_tabbed.contains("\tTM.")
            || upper_tabbed.contains("\tKT.")
            || upper_tabbed.contains("\tTL.")
            || upper_tabbed.contains("\tTUQ."))
    {
        return true;
    }

    const CAPS_CUES: &[&str] = &[
        "CHỦ TỊCH",
        "TỔNG GIÁM ĐỐC",
        "GIÁM ĐỐC",
        "HIỆU TRƯỞNG",
        "TRƯỞNG PHÒNG",
        "THỦ TRƯỞNG",
        "BỘ TRƯỞNG",
        "THỨ TRƯỞNG",
        "CỤC TRƯỞNG",
        "BÍ THƯ",
    ];

    let jc = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:jc"))
        .and_then(|j| j.get_attr("w:val"))
        .unwrap_or("");
    let has_tabs = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:tabs"))
        .is_some()
        || has_tab_node(p);

    if !trimmed.starts_with('-')
        && !trimmed.starts_with('+')
        && char_len <= 65
        && (jc == "center" || jc == "right" || has_tabs || trimmed == upper)
    {
        let matches_caps_cue = CAPS_CUES.iter().any(|c| trimmed.contains(c))
            || (has_caps_formatting(p) && CAPS_CUES.iter().any(|c| upper.contains(c)));
        if matches_caps_cue {
            return true;
        }
    }

    false
}

pub fn is_national_header_cue(text_upper: &str) -> bool {
    text_upper.contains("CỘNG HÒA XÃ HỘI")
        || text_upper.contains("CỘNG HOÀ XÃ HỘI")
        || text_upper.contains("ĐỘC LẬP - TỰ DO")
        || text_upper.contains("ĐỘC LẬP – TỰ DO")
        || text_upper.contains("ĐỘC LẬP-TỰ DO")
        || text_upper.contains("ĐẢNG CỘNG SẢN VIỆT NAM")
}

pub fn paragraph_has_center_or_right_jc(p: &XmlElement) -> bool {
    p.child("w:pPr")
        .and_then(|pp| pp.child("w:jc"))
        .and_then(|j| j.get_attr("w:val"))
        .map(|v| v == "center" || v == "right")
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn is_protected_admin_paragraph(p: &XmlElement, para_idx: usize) -> bool {
    is_protected_admin_paragraph_ctx(p, para_idx < 12)
}

pub fn is_protected_admin_paragraph_ctx(p: &XmlElement, near_admin_header: bool) -> bool {
    let text = p.extract_text();
    let trimmed = text.trim();
    let upper = collapse_spaces_upper(trimmed);

    const ADMIN_PATTERNS: &[&str] = &[
        "CỘNG HÒA XÃ HỘI",
        "CỘNG HOÀ XÃ HỘI",
        "ĐỘC LẬP - TỰ DO",
        "ĐỘC LẬP – TỰ DO",
        "ĐỘC LẬP-TỰ DO",
        "ĐẢNG CỘNG SẢN VIỆT NAM",
        "NƠI NHẬN:",
        "NƠI NHẬN",
        "KÍNH GỬI:",
    ];

    if ADMIN_PATTERNS.iter().any(|pat| upper.contains(pat))
        || is_signature_trigger_paragraph(p, trimmed)
    {
        return true;
    }

    if near_admin_header {
        if upper.starts_with("SỐ:") || upper.starts_with("SỐ :") {
            return true;
        }
        if paragraph_has_center_or_right_jc(p) {
            return true;
        }
    }

    // Signature block cue near end: right/center-aligned date line
    if trimmed.chars().count() <= 70
        && paragraph_has_center_or_right_jc(p)
        && upper.contains("NGÀY ")
        && upper.contains("THÁNG ")
    {
        return true;
    }

    false
}

pub fn paper_dimensions_twips(paper: PaperSize) -> (i32, i32) {
    match paper {
        PaperSize::A4 => (11906, 16838),
        PaperSize::A3 => (16838, 23811),
        PaperSize::Letter => (12240, 15840),
    }
}

pub fn update_sect_pr_page_and_margins(sect_pr: &mut XmlElement, cfg: &FormatConfig) {
    let is_landscape = sect_pr
        .child("w:pgSz")
        .and_then(|s| s.get_attr("w:orient"))
        .map(|o| o == "landscape")
        .unwrap_or(false);

    let (pw, ph) = paper_dimensions_twips(cfg.paper_size);
    let pg_sz = sect_pr.ensure_child_ordered("w:pgSz", SECT_PR_ORDER);
    if is_landscape {
        pg_sz.set_attr("w:w", &ph.to_string());
        pg_sz.set_attr("w:h", &pw.to_string());
        pg_sz.set_attr("w:orient", "landscape");
    } else {
        pg_sz.set_attr("w:w", &pw.to_string());
        pg_sz.set_attr("w:h", &ph.to_string());
        pg_sz.remove_attr("w:orient");
    }

    let pg_mar = sect_pr.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
    pg_mar.set_attr("w:top", &cm_to_twips(cfg.margin_top_cm).to_string());
    pg_mar.set_attr("w:bottom", &cm_to_twips(cfg.margin_bottom_cm).to_string());
    pg_mar.set_attr("w:left", &cm_to_twips(cfg.margin_left_cm).to_string());
    pg_mar.set_attr("w:right", &cm_to_twips(cfg.margin_right_cm).to_string());
    if pg_mar.get_attr("w:header").is_none() {
        pg_mar.set_attr("w:header", "709");
    }
    if pg_mar.get_attr("w:footer").is_none() {
        pg_mar.set_attr("w:footer", "709");
    }
}

fn sync_styles_defaults(styles_root: &mut XmlElement, cfg: &FormatConfig) {
    if let Some(doc_defaults) = styles_root.child_mut("w:docDefaults") {
        let r_pr_def = doc_defaults.ensure_child_first("w:rPrDefault");
        let r_pr = r_pr_def.ensure_child_first("w:rPr");
        set_run_font_and_size(r_pr, &cfg.font_name, Some(cfg.font_size_pt));
    }

    for node in &mut styles_root.children {
        if let XmlNode::Element(style_el) = node {
            if style_el.name == "w:style" && style_el.get_attr("w:styleId") == Some("Normal") {
                let r_pr = style_el.ensure_child_ordered(
                    "w:rPr",
                    &["w:name", "w:basedOn", "w:next", "w:qFormat", "w:pPr", "w:rPr"],
                );
                set_run_font_and_size(r_pr, &cfg.font_name, Some(cfg.font_size_pt));
                let _ = pt_to_half_pts(cfg.font_size_pt);
            }
        }
    }
}

fn apply_excel_font_formatting(
    pkg: &mut OpenXmlPackage,
    cfg: &FormatConfig,
    logs: &mut Vec<String>,
) -> Result<FormatStats, String> {
    let Some(styles_xml) = pkg.get_xml("xl/styles.xml") else {
        logs.push("Không tìm thấy xl/styles.xml trong bảng tính Excel.".to_string());
        return Ok(FormatStats::default());
    };
    let mut doc = XmlDocument::parse(&styles_xml)?;
    let mut updated_fonts = 0usize;
    if let Some(fonts) = doc.root.child_mut("fonts") {
        for node in &mut fonts.children {
            if let XmlNode::Element(font_el) = node {
                if font_el.name == "font" {
                    let name_el = font_el.ensure_child_first("name");
                    name_el.set_attr("val", &cfg.font_name);
                    if let Some(sz_el) = font_el.child_mut("sz") {
                        sz_el.set_attr("val", &format!("{}", cfg.font_size_pt));
                    }
                    updated_fonts += 1;
                }
            }
        }
    }
    pkg.set_xml("xl/styles.xml", doc.to_xml_string()?);
    logs.push(format!(
        "Đã chuẩn hóa {} định nghĩa Font trong bảng tính Excel ({} {}pt).",
        updated_fonts, cfg.font_name, cfg.font_size_pt
    ));
    Ok(FormatStats {
        runs_updated: updated_fonts,
        ..FormatStats::default()
    })
}
