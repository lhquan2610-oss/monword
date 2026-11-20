use crate::engine::formatter::{classify_admin_table, collapse_spaces_upper, AdminTableKind};
use crate::engine::openxml_arch::SelectivePackageView;
use crate::engine::std_engine::ir::{
    read_rpr_font, read_rpr_sz_pt, DocumentIr, ParagraphInfo, ParagraphRole, SectionInfo,
    StyleCatalog, TableInfo,
};
use crate::engine::xml_helpers::{XmlDocument, XmlElement, XmlNode};
use crate::models::{
    DocumentMapNode, ElementLocator, HeaderFooterMapEntry, IntelligentDocumentMap,
    PageLayoutEntry, ParagraphMapEntry, RecognizedStructureKind, SectionMapEntry, StyleMapEntry,
    TableMapEntry, VisualObjectMapEntry,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

#[inline]
pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[inline]
pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[inline]
pub fn twips_to_mm(twips: i32) -> f64 {
    round1(twips as f64 / 56.692913)
}

#[inline]
pub fn emu_to_mm(emu: i64) -> f64 {
    round1(emu as f64 / 36_000.0)
}

#[inline]
pub fn twips_to_pt(twips: i32) -> f64 {
    round1(twips as f64 / 20.0)
}

#[derive(Debug, Clone)]
struct NumberingLevelMeta {
    num_fmt: String,
    lvl_text: String,
}

#[derive(Debug, Default)]
struct NumberingLookup {
    num_to_abstract: HashMap<String, String>,
    levels: HashMap<(String, String), NumberingLevelMeta>,
}

impl NumberingLookup {
    fn from_xml(xml: Option<&str>) -> Self {
        let Some(raw) = xml else {
            return Self::default();
        };
        let Ok(doc) = XmlDocument::parse(raw) else {
            return Self::default();
        };
        let mut out = Self::default();
        for el in doc.root.children_elements() {
            if el.name == "w:abstractNum" {
                if let Some(abs_id) = el.get_attr("w:abstractNumId") {
                    for lvl in el.children_elements().filter(|c| c.name == "w:lvl") {
                        let ilvl = lvl.get_attr("w:ilvl").unwrap_or("0").to_string();
                        let num_fmt = lvl
                            .child("w:numFmt")
                            .and_then(|n| n.get_attr("w:val"))
                            .unwrap_or("decimal")
                            .to_string();
                        let lvl_text = lvl
                            .child("w:lvlText")
                            .and_then(|n| n.get_attr("w:val"))
                            .unwrap_or("")
                            .to_string();
                        out.levels.insert(
                            (abs_id.to_string(), ilvl),
                            NumberingLevelMeta { num_fmt, lvl_text },
                        );
                    }
                }
            } else if el.name == "w:num" {
                if let (Some(num_id), Some(abs_id)) = (
                    el.get_attr("w:numId"),
                    el.child("w:abstractNumId")
                        .and_then(|a| a.get_attr("w:val")),
                ) {
                    out.num_to_abstract
                        .insert(num_id.to_string(), abs_id.to_string());
                }
            }
        }
        out
    }

    fn resolve(&self, num_id: &str, ilvl: &str) -> Option<&NumberingLevelMeta> {
        let abs = self.num_to_abstract.get(num_id)?;
        self.levels.get(&(abs.clone(), ilvl.to_string()))
    }
}

#[derive(Debug, Clone)]
struct StyleMetaRaw {
    style_id: String,
    style_name: String,
    style_type: String,
    based_on: Option<String>,
    is_default: bool,
}

fn parse_style_metadata(view: &SelectivePackageView) -> (Vec<StyleMetaRaw>, HashMap<String, bool>) {
    let mut list = Vec::new();
    let mut style_widow: HashMap<String, bool> = HashMap::new();
    let Some(raw) = view.get_xml("word/styles.xml") else {
        return (list, style_widow);
    };
    let Ok(doc) = XmlDocument::parse(raw) else {
        return (list, style_widow);
    };
    for st in doc.root.children_elements().filter(|e| e.name == "w:style") {
        let Some(id) = st.get_attr("w:styleId") else {
            continue;
        };
        let style_type = st.get_attr("w:type").unwrap_or("paragraph").to_string();
        let is_default = st.get_attr("w:default") == Some("1");
        let style_name = st
            .child("w:name")
            .and_then(|n| n.get_attr("w:val"))
            .unwrap_or(id)
            .to_string();
        let based_on = st
            .child("w:basedOn")
            .and_then(|b| b.get_attr("w:val"))
            .map(|s| s.to_string());
        if let Some(wc) = st.child("w:pPr").and_then(|pp| pp.child("w:widowControl")).map(|w| {
            w.get_attr("w:val") != Some("0") && w.get_attr("w:val") != Some("false")
        }) {
            style_widow.insert(id.to_string(), wc);
        }
        list.push(StyleMetaRaw {
            style_id: id.to_string(),
            style_name,
            style_type,
            based_on,
            is_default,
        });
    }
    (list, style_widow)
}

#[derive(Debug, Clone, Default)]
struct ParaXmlFlags {
    widow_control: bool,
    keep_next: bool,
    keep_lines: bool,
    page_break_before: bool,
    has_explicit_page_break: bool,
    has_column_break: bool,
    has_last_rendered_break: bool,
    num_id: Option<String>,
    num_ilvl: Option<String>,
    has_tab_stops: bool,
    is_centered_or_right: bool,
}

#[derive(Debug, Clone)]
struct ExtractedVisualRaw {
    is_text_box: bool,
    anchor_mode: String,
    rel_id: Option<String>,
    width_mm: f64,
    height_mm: f64,
    inner_paragraphs: Vec<XmlElement>,
}

fn inspect_para_xml_flags(
    p: &XmlElement,
    styles: &StyleCatalog,
    style_widow: &HashMap<String, bool>,
) -> ParaXmlFlags {
    let p_pr = p.child("w:pPr");
    let style_id = p_pr
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"));

    let direct_widow = p_pr.and_then(|pp| pp.child("w:widowControl")).map(|w| {
        w.get_attr("w:val") != Some("0") && w.get_attr("w:val") != Some("false")
    });
    let widow_control = direct_widow
        .or_else(|| style_id.and_then(|id| style_widow.get(id).copied()))
        .or_else(|| {
            styles
                .default_para_style_id
                .as_ref()
                .and_then(|id| style_widow.get(id).copied())
        })
        .unwrap_or(true);

    let keep_next = p_pr
        .and_then(|pp| pp.child("w:keepNext"))
        .map(|k| k.get_attr("w:val") != Some("0") && k.get_attr("w:val") != Some("false"))
        .unwrap_or(false);
    let keep_lines = p_pr
        .and_then(|pp| pp.child("w:keepLines"))
        .map(|k| k.get_attr("w:val") != Some("0") && k.get_attr("w:val") != Some("false"))
        .unwrap_or(false);
    let page_break_before = p_pr
        .and_then(|pp| pp.child("w:pageBreakBefore"))
        .map(|k| k.get_attr("w:val") != Some("0") && k.get_attr("w:val") != Some("false"))
        .unwrap_or(false);

    let num_pr = p_pr.and_then(|pp| pp.child("w:numPr"));
    let num_id = num_pr
        .and_then(|np| np.child("w:numId"))
        .and_then(|n| n.get_attr("w:val"))
        .map(|s| s.to_string());
    let num_ilvl = num_pr
        .and_then(|np| np.child("w:ilvl"))
        .and_then(|n| n.get_attr("w:val"))
        .map(|s| s.to_string());
    let has_tab_stops = p_pr.and_then(|pp| pp.child("w:tabs")).is_some();

    let jc_val = p_pr
        .and_then(|pp| pp.child("w:jc"))
        .and_then(|j| j.get_attr("w:val"))
        .or_else(|| {
            styles
                .effective_style(style_id)
                .and_then(|s| s.jc.as_deref())
        })
        .unwrap_or("left");

    let mut has_explicit_page_break = false;
    let mut has_column_break = false;
    let mut has_last_rendered_break = false;
    scan_para_breaks(
        p,
        &mut has_explicit_page_break,
        &mut has_column_break,
        &mut has_last_rendered_break,
    );

    ParaXmlFlags {
        widow_control,
        keep_next,
        keep_lines,
        page_break_before,
        has_explicit_page_break,
        has_column_break,
        has_last_rendered_break,
        num_id,
        num_ilvl,
        has_tab_stops,
        is_centered_or_right: matches!(jc_val, "center" | "right"),
    }
}

fn scan_para_breaks(
    el: &XmlElement,
    page_br: &mut bool,
    col_br: &mut bool,
    rendered_br: &mut bool,
) {
    // Do not confuse breaks inside Text Box with body page breaks
    if matches!(
        el.name.as_str(),
        "mc:AlternateContent" | "w:drawing" | "w:pict" | "w:txbxContent"
    ) {
        return;
    }
    for sub in el.children_elements() {
        match sub.name.as_str() {
            "w:br" => {
                let bt = sub.get_attr("w:type").unwrap_or("textWrapping");
                if bt == "page" {
                    *page_br = true;
                } else if bt == "column" {
                    *col_br = true;
                }
            }
            "w:lastRenderedPageBreak" => {
                *rendered_br = true;
            }
            _ => scan_para_breaks(sub, page_br, col_br, rendered_br),
        }
    }
}

/// Extract drawings (images) and Text Boxes (`w:txbxContent`) from an element,
/// strictly deduplicating `mc:AlternateContent` (`mc:Choice` preferred over `mc:Fallback`).
fn extract_visuals_dedup(el: &XmlElement, out: &mut Vec<ExtractedVisualRaw>) {
    match el.name.as_str() {
        "mc:AlternateContent" => {
            if let Some(choice) = el.child("mc:Choice") {
                extract_visuals_dedup(choice, out);
            } else if let Some(fallback) = el.child("mc:Fallback") {
                extract_visuals_dedup(fallback, out);
            }
            return;
        }
        "w:drawing" => {
            let (anchor_mode, w_mm, h_mm) = measure_drawing_extent_mm(el);
            let mut txbx_paras = Vec::new();
            collect_txbx_paragraphs(el, &mut txbx_paras);
            if !txbx_paras.is_empty() {
                out.push(ExtractedVisualRaw {
                    is_text_box: true,
                    anchor_mode: anchor_mode.clone(),
                    rel_id: None,
                    width_mm: if w_mm > 0.0 { w_mm } else { 80.0 },
                    height_mm: if h_mm > 0.0 { h_mm } else { 25.0 },
                    inner_paragraphs: txbx_paras,
                });
            }
            let mut blips = Vec::new();
            collect_blip_rids(el, &mut blips);
            for rid in blips {
                out.push(ExtractedVisualRaw {
                    is_text_box: false,
                    anchor_mode: anchor_mode.clone(),
                    rel_id: Some(rid),
                    width_mm: w_mm,
                    height_mm: h_mm,
                    inner_paragraphs: Vec::new(),
                });
            }
            return;
        }
        "w:pict" => {
            let (w_mm, h_mm) = measure_vml_shape_mm(el);
            let mut txbx_paras = Vec::new();
            collect_txbx_paragraphs(el, &mut txbx_paras);
            if !txbx_paras.is_empty() {
                out.push(ExtractedVisualRaw {
                    is_text_box: true,
                    anchor_mode: "vml".to_string(),
                    rel_id: None,
                    width_mm: if w_mm > 0.0 { w_mm } else { 80.0 },
                    height_mm: if h_mm > 0.0 { h_mm } else { 25.0 },
                    inner_paragraphs: txbx_paras,
                });
            } else {
                let mut vml_rids = Vec::new();
                collect_vml_image_rids(el, &mut vml_rids);
                for rid in vml_rids {
                    out.push(ExtractedVisualRaw {
                        is_text_box: false,
                        anchor_mode: "vml".to_string(),
                        rel_id: Some(rid),
                        width_mm: w_mm,
                        height_mm: h_mm,
                        inner_paragraphs: Vec::new(),
                    });
                }
            }
            return;
        }
        _ => {}
    }

    for sub in el.children_elements() {
        extract_visuals_dedup(sub, out);
    }
}

fn measure_drawing_extent_mm(drawing: &XmlElement) -> (String, f64, f64) {
    let (mode, container) = if let Some(inline) = drawing.child("wp:inline") {
        ("inline".to_string(), Some(inline))
    } else if let Some(anchor) = drawing.child("wp:anchor") {
        ("floating".to_string(), Some(anchor))
    } else {
        ("inline".to_string(), None)
    };
    if let Some(c) = container {
        if let Some(ext) = c.child("wp:extent") {
            let cx: i64 = ext.get_attr("cx").and_then(|v| v.parse().ok()).unwrap_or(0);
            let cy: i64 = ext.get_attr("cy").and_then(|v| v.parse().ok()).unwrap_or(0);
            return (mode, emu_to_mm(cx), emu_to_mm(cy));
        }
    }
    (mode, 0.0, 0.0)
}

fn measure_vml_shape_mm(pict: &XmlElement) -> (f64, f64) {
    for sub in pict.children_elements() {
        if let Some(st) = sub.get_attr("style") {
            let w = parse_css_mm(st, "width");
            let h = parse_css_mm(st, "height");
            if w > 0.0 || h > 0.0 {
                return (w, h);
            }
        }
        let (cw, ch) = measure_vml_shape_mm(sub);
        if cw > 0.0 || ch > 0.0 {
            return (cw, ch);
        }
    }
    (0.0, 0.0)
}

fn parse_css_mm(style: &str, prop: &str) -> f64 {
    for part in style.split(';') {
        if let Some((k, v)) = part.split_once(':') {
            if k.trim().eq_ignore_ascii_case(prop) {
                let val = v.trim().to_ascii_lowercase();
                if let Some(num) = val.strip_suffix("pt") {
                    if let Ok(n) = num.trim().parse::<f64>() {
                        return round1(n * 25.4 / 72.0);
                    }
                } else if let Some(num) = val.strip_suffix("in") {
                    if let Ok(n) = num.trim().parse::<f64>() {
                        return round1(n * 25.4);
                    }
                } else if let Some(num) = val.strip_suffix("cm") {
                    if let Ok(n) = num.trim().parse::<f64>() {
                        return round1(n * 10.0);
                    }
                } else if let Some(num) = val.strip_suffix("mm") {
                    if let Ok(n) = num.trim().parse::<f64>() {
                        return round1(n);
                    }
                } else if let Some(num) = val.strip_suffix("px") {
                    if let Ok(n) = num.trim().parse::<f64>() {
                        return round1(n * 25.4 / 96.0);
                    }
                }
            }
        }
    }
    0.0
}

fn collect_txbx_paragraphs(el: &XmlElement, out: &mut Vec<XmlElement>) {
    if el.name == "w:txbxContent" {
        for sub in el.children_elements() {
            if sub.name == "w:p" {
                out.push(sub.clone());
            } else if sub.name == "w:tbl" {
                for tr in sub.children_elements().filter(|e| e.name == "w:tr") {
                    for tc in tr.children_elements().filter(|e| e.name == "w:tc") {
                        for p in tc.children_elements().filter(|e| e.name == "w:p") {
                            out.push(p.clone());
                        }
                    }
                }
            }
        }
        return;
    }
    for sub in el.children_elements() {
        collect_txbx_paragraphs(sub, out);
    }
}

fn collect_blip_rids(el: &XmlElement, out: &mut Vec<String>) {
    if el.name == "a:blip" {
        if let Some(rid) = el.get_attr("r:embed").or_else(|| el.get_attr("r:link")) {
            if !rid.is_empty() {
                out.push(rid.to_string());
            }
        }
    }
    for sub in el.children_elements() {
        collect_blip_rids(sub, out);
    }
}

fn collect_vml_image_rids(el: &XmlElement, out: &mut Vec<String>) {
    if el.name == "v:imagedata" {
        if let Some(rid) = el.get_attr("r:id").or_else(|| el.get_attr("o:relid")) {
            if !rid.is_empty() {
                out.push(rid.to_string());
            }
        }
    }
    for sub in el.children_elements() {
        collect_vml_image_rids(sub, out);
    }
}

fn re_caption_line() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(Bảng|Hình|Biểu\s*đồ|Sơ\s*đồ|Ảnh)\s+\d+([.:–-])?(\s|$)").expect("valid")
    })
}

fn re_chapter_h1() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(PHẦN\s+[IVXLCDM\d]+|CHƯƠNG\s+[IVXLCDM\d]+|[IVXLCDM]+\.\s+\S+)")
            .expect("valid")
    })
}

fn re_decimal_h() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d+(?:\.\d+){0,3})\.?\s+\S+").expect("valid"))
}

fn re_dieu_h() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^Điều\s+\d+[\.:]").expect("valid"))
}

fn re_appendix_head() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^PHỤ\s+LỤC(?:\s+(?:SỐ\s+)?[IVXLCDM\dA-Z]+)?(?:\s*[:.\-–]|\s*$)").expect("valid")
    })
}

fn re_letter_sub() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zđ]\)\s+\S+").expect("valid"))
}

#[allow(clippy::too_many_arguments)]
fn classify_para_with_context(
    ir_p: &ParagraphInfo,
    flags: &ParaXmlFlags,
    numbering: &NumberingLookup,
    body_median_size: f64,
    prev_section_index: usize,
    seen_header_or_title: bool,
    seen_first_heading: bool,
    seen_signature_zone: bool,
    after_doc_type_title: bool,
    body_non_empty_idx: usize,
) -> (RecognizedStructureKind, f64, Vec<String>) {
    let trimmed = ir_p.trimmed.as_str();
    let upper = collapse_spaces_upper(trimmed);
    let mut signals = Vec::new();

    if ir_p.in_table_idx.is_some() {
        return match ir_p.role {
            ParagraphRole::AdminHeaderCell => (
                RecognizedStructureKind::DocumentHeader,
                0.98,
                vec!["openxml:header_table".to_string()],
            ),
            ParagraphRole::RecipientNoiNhan => (
                RecognizedStructureKind::RecipientBlock,
                0.96,
                vec!["openxml:sig_table_left".to_string()],
            ),
            ParagraphRole::SignatureBlock => (
                RecognizedStructureKind::SignatureBlock,
                0.96,
                vec!["openxml:sig_table_right".to_string()],
            ),
            _ => (
                RecognizedStructureKind::MainBody,
                0.9,
                vec!["openxml:data_table_cell".to_string()],
            ),
        };
    }

    if trimmed.is_empty() {
        return (
            RecognizedStructureKind::MainBody,
            0.7,
            vec!["empty_paragraph".to_string()],
        );
    }

    let char_count = trimmed.chars().count();
    let size_pt = ir_p.primary_size_pt.unwrap_or(body_median_size);
    let starts_new_section = ir_p.section_index > prev_section_index;
    let has_hard_break = flags.page_break_before || flags.has_explicit_page_break || starts_new_section;

    // 1. Header zone signals (non-table header paragraphs at top of document)
    if matches!(ir_p.role, ParagraphRole::AdminHeaderCell) {
        signals.push("zone:top_of_document".to_string());
        signals.push("context:admin_header_cue".to_string());
        return (RecognizedStructureKind::DocumentHeader, 0.95, signals);
    }

    // 2. Document type title ("QUYẾT ĐỊNH", "CÔNG VĂN", "TỜ TRÌNH", "BÁO CÁO", ...)
    if matches!(ir_p.role, ParagraphRole::DocTypeHeading) {
        signals.push("zone:pre_body".to_string());
        signals.push(format!("align:{}", ir_p.effective_jc));
        if ir_p.is_bold_majority {
            signals.push("style:bold_uppercase".to_string());
        }
        return (RecognizedStructureKind::DocTypeTitle, 0.97, signals);
    }

    // 3. Subject line ("V/v ..." or centered bold line right after DocTypeTitle, before any body heading/caption)
    if !seen_first_heading
        && !re_caption_line().is_match(trimmed)
        && !re_chapter_h1().is_match(trimmed)
        && !re_decimal_h().is_match(trimmed)
        && !re_dieu_h().is_match(trimmed)
        && (matches!(ir_p.role, ParagraphRole::DocSubject)
            || (after_doc_type_title
                && body_non_empty_idx <= 12
                && (upper.starts_with("V/V") || upper.starts_with("VỀ VIỆC"))))
    {
        signals.push("context:follows_doc_type".to_string());
        if upper.starts_with("V/V") || upper.starts_with("VỀ VIỆC") {
            signals.push("cue:trich_yeu_prefix".to_string());
        }
        return (RecognizedStructureKind::DocSubject, 0.94, signals);
    }

    // 4. Non-administrative Main Title (e.g., Enterprise SOP/Report or Academic Thesis without DocTypeHeading)
    let style_low = ir_p
        .style_id
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_title_style = style_low == "title" || style_low.contains("doctitle");
    if !seen_first_heading
        && !seen_header_or_title
        && !after_doc_type_title
        && body_non_empty_idx <= 5
        && char_count <= 200
        && !trimmed.ends_with('.')
        && !re_chapter_h1().is_match(trimmed)
        && !re_decimal_h().is_match(trimmed)
        && !re_dieu_h().is_match(trimmed)
        && (is_title_style
            || (ir_p.effective_jc == "center"
                && (ir_p.is_bold_majority || size_pt >= body_median_size + 1.5)))
    {
        signals.push("zone:opening_block".to_string());
        if is_title_style {
            signals.push("openxml:style_title".to_string());
        }
        if ir_p.effective_jc == "center" {
            signals.push("align:center".to_string());
        }
        if size_pt >= body_median_size + 1.5 {
            signals.push(format!("size:+{:.1}pt", size_pt - body_median_size));
        }
        return (RecognizedStructureKind::MainTitle, 0.91, signals);
    }

    // 5. Signature & Recipient blocks in trailing/closing zone
    if matches!(ir_p.role, ParagraphRole::RecipientNoiNhan) {
        signals.push("context:closing_zone".to_string());
        signals.push("cue:noi_nhan".to_string());
        return (RecognizedStructureKind::RecipientBlock, 0.95, signals);
    }
    if matches!(ir_p.role, ParagraphRole::SignatureBlock) {
        signals.push("context:closing_zone".to_string());
        if ir_p.indent_left > 2500 || flags.has_tab_stops || flags.is_centered_or_right {
            signals.push("layout:signature_offset".to_string());
        }
        return (RecognizedStructureKind::SignatureBlock, 0.94, signals);
    }

    // 6. Appendix block — MUST NOT misclassify inline body citations like "chi tiết tại Phụ lục 1"
    let looks_like_appendix_head = re_appendix_head().is_match(trimmed)
        || (upper.starts_with("PHỤ LỤC") && char_count <= 100);
    if looks_like_appendix_head
        && char_count <= 120
        && !trimmed.ends_with('.')
        && (seen_signature_zone
            || has_hard_break
            || ir_p.outline_lvl.is_some()
            || ((ir_p.effective_jc == "center" || ir_p.is_bold_majority) && char_count <= 80))
    {
        if seen_signature_zone {
            signals.push("context:after_signature_zone".to_string());
        }
        if has_hard_break {
            signals.push("openxml:new_page_or_section".to_string());
        }
        if ir_p.effective_jc == "center" || ir_p.is_bold_majority {
            signals.push("format:centered_or_bold_title".to_string());
        }
        return (RecognizedStructureKind::AppendixBlock, 0.95, signals);
    }

    // 7. Disqualify table/image captions from becoming headings
    if re_caption_line().is_match(trimmed) {
        signals.push("context:figure_or_table_caption".to_string());
        return (RecognizedStructureKind::MainBody, 0.9, signals);
    }

    // 8. Multi-signal Heading recognition (OpenXML style + outlineLvl + numbering.xml + hierarchy + typography)
    let mut heading_lvl: Option<u8> = None;
    let mut score: f64 = 0.0;

    if let Some(ol) = ir_p.outline_lvl {
        let lvl = (ol + 1).clamp(1, 4);
        heading_lvl = Some(lvl);
        score += 0.45;
        signals.push(format!("openxml:outlineLvl={}", ol));
    }

    if style_low.starts_with("heading") {
        let parsed = style_low
            .trim_start_matches("heading")
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u8>()
            .ok()
            .map(|n| n.clamp(1, 4));
        if let Some(sl) = parsed {
            if heading_lvl.is_none() {
                heading_lvl = Some(sl);
            }
            score += 0.45;
            signals.push(format!("openxml:pStyle={}", ir_p.style_id.as_deref().unwrap_or("")));
        }
    }

    if let (Some(nid), Some(ilvl)) = (&flags.num_id, &flags.num_ilvl) {
        if let Some(meta) = numbering.resolve(nid, ilvl) {
            let num_lvl = ilvl.parse::<u8>().unwrap_or(0).saturating_add(1).clamp(1, 4);
            let is_bullet = meta.num_fmt.eq_ignore_ascii_case("bullet");
            if !is_bullet && char_count <= 130 && !trimmed.ends_with('.') && !trimmed.ends_with(';') {
                if heading_lvl.is_none() && (ir_p.is_bold_majority || size_pt > body_median_size + 0.5) {
                    heading_lvl = Some(num_lvl);
                }
                if heading_lvl.is_some() || ir_p.is_bold_majority {
                    score += 0.35;
                    signals.push(format!(
                        "openxml:numbering(fmt={},ilvl={},pat={})",
                        meta.num_fmt, ilvl, meta.lvl_text
                    ));
                }
            }
        }
    }

    if char_count <= 140 && !trimmed.ends_with('.') && !trimmed.ends_with(';') && !trimmed.ends_with(',') {
        if re_chapter_h1().is_match(trimmed) && (ir_p.is_bold_majority || ir_p.effective_jc == "center" || heading_lvl.is_some()) {
            heading_lvl = Some(heading_lvl.unwrap_or(1));
            score += 0.35;
            signals.push("structure:chapter_or_roman".to_string());
        } else if re_dieu_h().is_match(trimmed) {
            heading_lvl = Some(heading_lvl.unwrap_or(2));
            score += 0.35;
            signals.push("structure:dieu_clause".to_string());
        } else if let Some(caps) = re_decimal_h().captures(trimmed) {
            let code = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let depth = (code.split('.').count() as u8).clamp(1, 4);
            if depth >= 2 || ir_p.is_bold_majority || heading_lvl.is_some() {
                heading_lvl = Some(heading_lvl.unwrap_or(depth));
                score += 0.30;
                signals.push(format!("structure:decimal_depth_{}", depth));
            }
        } else if re_letter_sub().is_match(trimmed) && ir_p.is_bold_majority && heading_lvl.is_some() {
            heading_lvl = Some(4);
            score += 0.20;
            signals.push("structure:sub_letter".to_string());
        }
    }

    if let Some(lvl) = heading_lvl {
        if ir_p.is_bold_majority {
            score += 0.15;
            signals.push("format:bold".to_string());
        }
        if size_pt > body_median_size + 0.5 {
            score += 0.10;
            signals.push(format!("size:{:.1}pt", size_pt));
        }
        if ir_p.keep_next_signal_hint(flags) {
            score += 0.08;
            signals.push("openxml:keepNext".to_string());
        }
        if score >= 0.40 {
            let kind = match lvl {
                1 => RecognizedStructureKind::HeadingLevel1,
                2 => RecognizedStructureKind::HeadingLevel2,
                3 => RecognizedStructureKind::HeadingLevel3,
                _ => RecognizedStructureKind::HeadingLevel4,
            };
            return (kind, score.min(0.99), signals);
        }
    }

    signals.push("context:prose_flow".to_string());
    (RecognizedStructureKind::MainBody, 0.90, signals)
}

trait KeepNextHint {
    fn keep_next_signal_hint(&self, flags: &ParaXmlFlags) -> bool;
}

impl KeepNextHint for ParagraphInfo {
    fn keep_next_signal_hint(&self, flags: &ParaXmlFlags) -> bool {
        flags.keep_next
    }
}

pub fn build_intelligent_map(
    view: &SelectivePackageView,
    ir: &DocumentIr,
) -> Result<IntelligentDocumentMap, String> {
    let doc_xml = view
        .get_xml("word/document.xml")
        .ok_or_else(|| "Tài liệu thiếu word/document.xml.".to_string())?;
    let doc = XmlDocument::parse(doc_xml)?;
    let body = doc
        .root
        .child("w:body")
        .ok_or_else(|| "word/document.xml thiếu thẻ w:body.".to_string())?;

    let numbering = NumberingLookup::from_xml(view.get_xml("word/numbering.xml"));
    let (style_meta_raw, style_widow) = parse_style_metadata(view);

    // Estimate median body font size so headings / titles contrast accurately without hardcoding
    let mut size_samples: Vec<f64> = ir
        .paragraphs
        .iter()
        .filter(|p| p.in_table_idx.is_none() && p.trimmed.chars().count() > 60)
        .filter_map(|p| p.primary_size_pt)
        .collect();
    size_samples.sort_by(|a, b| a.total_cmp(b));
    let body_median_size = if size_samples.is_empty() {
        13.0
    } else {
        size_samples[size_samples.len() / 2]
    };

    // Index IR paragraphs by element_path & IR tables by body_child_idx
    let para_by_path: HashMap<&str, &ParagraphInfo> = ir
        .paragraphs
        .iter()
        .map(|p| (p.element_path.as_str(), p))
        .collect();
    let table_by_bidx: HashMap<usize, &TableInfo> =
        ir.tables.iter().map(|t| (t.body_child_idx, t)).collect();
    let section_by_idx: HashMap<usize, &SectionInfo> = ir
        .sections
        .iter()
        .map(|s| (s.summary.section_index, s))
        .collect();

    // Index Style usage counts
    let mut style_usage: HashMap<String, usize> = HashMap::new();
    let mut style_conflicts: HashMap<String, usize> = HashMap::new();
    for p in &ir.paragraphs {
        let sid = p.style_id.as_deref().unwrap_or("Normal").to_string();
        *style_usage.entry(sid.clone()).or_insert(0) += 1;
        if p.has_direct_formatting_conflict {
            *style_conflicts.entry(sid).or_insert(0) += 1;
        }
    }

    // Index Header/Footer bindings from SectionInfo
    let mut part_bindings: HashMap<String, (Vec<usize>, Vec<String>)> = HashMap::new();
    let doc_sect_order: Vec<&XmlElement> = {
        let mut list = Vec::new();
        for node in &body.children {
            if let XmlNode::Element(el) = node {
                if el.name == "w:p" {
                    if let Some(sect) = el.child("w:pPr").and_then(|pp| pp.child("w:sectPr")) {
                        list.push(sect);
                    }
                }
            }
        }
        if let Some(sect) = body.child("w:sectPr") {
            list.push(sect);
        }
        list
    };
    for (idx, sect_el) in doc_sect_order.iter().enumerate() {
        let sec_num = idx + 1;
        for child in sect_el.children_elements() {
            if child.name == "w:headerReference" || child.name == "w:footerReference" {
                if let Some(rid) = child.get_attr("r:id") {
                    if let Some((_, target)) = ir.rels_map.get(rid) {
                        let norm_part = normalize_rel_word_part(target);
                        let bind_type = child.get_attr("w:type").unwrap_or("default").to_string();
                        let entry = part_bindings.entry(norm_part).or_default();
                        if !entry.0.contains(&sec_num) {
                            entry.0.push(sec_num);
                        }
                        if !entry.1.contains(&bind_type) {
                            entry.1.push(bind_type);
                        }
                    }
                }
            }
        }
    }

    // Build HeaderFooterMapEntry
    let mut header_footer_map: Vec<HeaderFooterMapEntry> = Vec::new();
    let mut sec_headers: HashMap<usize, Vec<String>> = HashMap::new();
    let mut sec_footers: HashMap<usize, Vec<String>> = HashMap::new();
    let mut doc_nodes: Vec<DocumentMapNode> = Vec::new();
    let mut doc_node_seq = 0usize;

    for (hf_idx, hf) in ir.headers_footers.iter().enumerate() {
        let hf_id = format!("hf-{}", hf_idx + 1);
        let (linked_sections, bind_types) = part_bindings
            .get(&hf.part_name.to_ascii_lowercase())
            .or_else(|| {
                part_bindings
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(&hf.part_name))
                    .map(|(_, v)| v)
            })
            .cloned()
            .unwrap_or_else(|| (Vec::new(), vec!["unlinked".to_string()]));

        let (para_cnt, tbl_cnt, has_img) = count_hf_contents(view.get_xml(&hf.part_name));
        for &sec_n in &linked_sections {
            if hf.kind == "header" {
                sec_headers.entry(sec_n).or_default().push(hf_id.clone());
            } else {
                sec_footers.entry(sec_n).or_default().push(hf_id.clone());
            }
        }

        let locator = ElementLocator {
            part_name: hf.part_name.clone(),
            element_path: format!("{}[{}]", hf.kind, hf_idx),
            body_child_index: None,
            section_index: *linked_sections.first().unwrap_or(&1),
            estimated_page_start: 1,
            estimated_page_end: 1,
            column_index: 0,
            x_left_mm: 0.0,
            y_top_mm: if hf.kind == "header" { 10.0 } else { 282.0 },
            width_mm: 165.0,
            height_mm: 12.0,
        };

        header_footer_map.push(HeaderFooterMapEntry {
            hf_id: hf_id.clone(),
            part_name: hf.part_name.clone(),
            hf_kind: hf.kind.clone(),
            bind_types: bind_types.clone(),
            linked_sections: linked_sections.clone(),
            paragraph_count: para_cnt,
            table_count: tbl_cnt,
            has_page_field: hf.has_page_field,
            has_numpages_field: hf.has_numpages_field,
            has_images: has_img,
            text_preview: hf.text_preview.clone(),
            locator: locator.clone(),
        });

        doc_node_seq += 1;
        doc_nodes.push(DocumentMapNode {
            node_id: hf_id,
            parent_id: None,
            children_ids: Vec::new(),
            kind: RecognizedStructureKind::HeaderFooter,
            label: format!(
                "{} ({})",
                if hf.kind == "header" { "Header" } else { "Footer" },
                hf.part_name.strip_prefix("word/").unwrap_or(&hf.part_name)
            ),
            excerpt: if hf.text_preview.is_empty() {
                format!(
                    "Liên kết Section {:?} · Số trang: {}",
                    linked_sections,
                    if hf.has_page_field { "Có" } else { "Không" }
                )
            } else {
                hf.text_preview.clone()
            },
            level: 0,
            confidence: 0.99,
            signals: vec![
                format!("part:{}", hf.part_name),
                format!("binds:{}", bind_types.join(",")),
            ],
            locator,
        });
    }

    // Build StyleMap
    let mut style_map: Vec<StyleMapEntry> = Vec::with_capacity(style_meta_raw.len());
    for raw in &style_meta_raw {
        let resolved = ir.styles.by_id.get(&raw.style_id);
        style_map.push(StyleMapEntry {
            style_id: raw.style_id.clone(),
            style_name: raw.style_name.clone(),
            style_type: raw.style_type.clone(),
            based_on: raw.based_on.clone(),
            is_default: raw.is_default,
            font_name: resolved.and_then(|r| r.font_name.clone()),
            font_size_pt: resolved.and_then(|r| r.size_pt),
            alignment: resolved.and_then(|r| r.jc.clone()),
            outline_level: resolved.and_then(|r| r.outline_lvl),
            usage_count: style_usage.get(&raw.style_id).copied().unwrap_or(0),
            direct_conflict_count: style_conflicts.get(&raw.style_id).copied().unwrap_or(0),
        });
    }

    // Walk body elements and build ParagraphMap, TableMap, VisualObjects, DocumentMap & simulate PageLayoutMap
    let mut paragraph_map: Vec<ParagraphMapEntry> = Vec::new();
    let mut table_map: Vec<TableMapEntry> = Vec::new();
    let mut visual_objects: Vec<VisualObjectMapEntry> = Vec::new();

    // Context machine across body flow
    let mut seen_header_or_title = false;
    let mut seen_doc_type_title = false;
    let mut seen_first_heading = false;
    let mut seen_signature_zone = false;
    let mut prev_section_index = 1usize;
    let mut body_non_empty_idx = 0usize;
    let mut prev_para_space_after_pt = 0.0f64;

    // Heading / Appendix hierarchy stack: (level, node_id)
    let mut hierarchy_stack: Vec<(u8, String)> = Vec::new();
    let mut current_appendix_node: Option<String> = None;

    // 2D Page Layout Simulation state
    let mut current_page = 1usize;
    let mut current_sec_idx = 1usize;
    let mut cur_col = 0usize;
    let mut cur_y_mm = 0.0f64;

    let mut page_buckets: HashMap<usize, PageLayoutEntry> = HashMap::new();
    let ensure_page_bucket =
        |page_no: usize, sec_idx: usize, buckets: &mut HashMap<usize, PageLayoutEntry>| {
            if !buckets.contains_key(&page_no) {
                let sec = section_by_idx.get(&sec_idx);
                let w_mm = sec.map(|s| s.summary.paper_width_mm).unwrap_or(210.0);
                let h_mm = sec.map(|s| s.summary.paper_height_mm).unwrap_or(297.0);
                let pw_mm = sec.map(|s| twips_to_mm(s.printable_width_twips)).unwrap_or(165.0);
                let ph_mm = sec
                    .map(|s| twips_to_mm((s.pg_h_twips - s.top_twips - s.bottom_twips).max(3000)))
                    .unwrap_or(257.0);
                buckets.insert(
                    page_no,
                    PageLayoutEntry {
                        page_id: format!("page-{}", page_no),
                        page_number: page_no,
                        section_index: sec_idx,
                        orientation: sec
                            .map(|s| s.summary.orientation.clone())
                            .unwrap_or_else(|| "portrait".to_string()),
                        width_mm: w_mm,
                        height_mm: h_mm,
                        printable_width_mm: pw_mm,
                        printable_height_mm: ph_mm,
                        used_height_mm: 0.0,
                        fill_ratio_pct: 0.0,
                        column_count: sec.map(|s| s.column_count).unwrap_or(1),
                        element_ids: Vec::new(),
                        has_overflow: false,
                        has_orphan_or_stranded_heading: false,
                        has_abnormal_whitespace: false,
                    },
                );
            }
        };
    ensure_page_bucket(1, 1, &mut page_buckets);

    let mut sec_page_ranges: HashMap<usize, (usize, usize)> = HashMap::new();
    sec_page_ranges.insert(1, (1, 1));

    let mut visual_seq = 0usize;
    let mut txbx_seq = 0usize;
    let mut body_node_inserted = false;

    for (b_idx, node) in body.children.iter().enumerate() {
        let XmlNode::Element(el) = node else {
            continue;
        };

        match el.name.as_str() {
            "w:p" => {
                let ep = format!("body/p[{}]", b_idx);
                let Some(&ir_p) = para_by_path.get(ep.as_str()) else {
                    continue;
                };
                let sec = section_by_idx.get(&ir_p.section_index);

                // Handle transition into a new section (from previous paragraph's inline sectPr)
                if ir_p.section_index > current_sec_idx {
                    current_sec_idx = ir_p.section_index;
                    let break_type = sec
                        .map(|s| s.summary.break_type.as_str())
                        .unwrap_or("nextPage");
                    if break_type != "continuous" {
                        current_page += 1;
                        if break_type == "evenPage" && current_page % 2 == 1 {
                            current_page += 1;
                        } else if break_type == "oddPage" && current_page % 2 == 0 {
                            current_page += 1;
                        }
                        cur_col = 0;
                        cur_y_mm = 0.0;
                    }
                    ensure_page_bucket(current_page, current_sec_idx, &mut page_buckets);
                    sec_page_ranges
                        .entry(current_sec_idx)
                        .or_insert((current_page, current_page));
                }

                let flags = inspect_para_xml_flags(el, &ir.styles, &style_widow);
                if (flags.page_break_before || (flags.has_last_rendered_break && cur_y_mm > 60.0))
                    && cur_y_mm > 2.0
                {
                    current_page += 1;
                    cur_col = 0;
                    cur_y_mm = 0.0;
                    ensure_page_bucket(current_page, current_sec_idx, &mut page_buckets);
                }

                if !ir_p.trimmed.is_empty() {
                    body_non_empty_idx += 1;
                }

                let (kind, confidence, signals) = classify_para_with_context(
                    ir_p,
                    &flags,
                    &numbering,
                    body_median_size,
                    prev_section_index,
                    seen_header_or_title,
                    seen_first_heading,
                    seen_signature_zone,
                    seen_doc_type_title,
                    body_non_empty_idx,
                );
                prev_section_index = ir_p.section_index;

                match kind {
                    RecognizedStructureKind::DocumentHeader => {
                        seen_header_or_title = true;
                    }
                    RecognizedStructureKind::DocTypeTitle => {
                        seen_header_or_title = true;
                        seen_doc_type_title = true;
                    }
                    RecognizedStructureKind::MainTitle => {
                        seen_header_or_title = true;
                    }
                    RecognizedStructureKind::HeadingLevel1
                    | RecognizedStructureKind::HeadingLevel2
                    | RecognizedStructureKind::HeadingLevel3
                    | RecognizedStructureKind::HeadingLevel4 => {
                        seen_first_heading = true;
                    }
                    RecognizedStructureKind::SignatureBlock
                    | RecognizedStructureKind::RecipientBlock => {
                        seen_signature_zone = true;
                    }
                    RecognizedStructureKind::AppendixBlock => {
                        seen_first_heading = true;
                    }
                    _ => {}
                }

                // Visuals (images and Text Boxes) anchored in this paragraph
                let mut extracted = Vec::new();
                extract_visuals_dedup(el, &mut extracted);

                // Estimate geometry inside current section & column
                let printable_w_mm = sec
                    .map(|s| twips_to_mm(s.printable_width_twips))
                    .unwrap_or(165.0);
                let printable_h_mm = sec
                    .map(|s| twips_to_mm((s.pg_h_twips - s.top_twips - s.bottom_twips).max(3000)))
                    .unwrap_or(257.0);
                let col_count = sec.map(|s| s.column_count).unwrap_or(1).max(1);
                let col_w_mm = sec
                    .and_then(|s| s.col_widths_twips.get(cur_col).copied())
                    .map(twips_to_mm)
                    .unwrap_or(printable_w_mm);
                let margin_left_mm = sec.map(|s| s.summary.margin_left_mm).unwrap_or(30.0);
                let margin_top_mm = sec.map(|s| s.summary.margin_top_mm).unwrap_or(20.0);
                let col_space_mm = sec
                    .map(|s| twips_to_mm(s.column_space_twips))
                    .unwrap_or(12.7);

                let ind_l_mm = twips_to_mm(ir_p.indent_left);
                let ind_r_mm = twips_to_mm(ir_p.indent_right);
                let ind_fl_mm = twips_to_mm(ir_p.indent_first_line);
                let usable_line_mm = (col_w_mm - ind_l_mm - ind_r_mm).max(20.0);
                let font_pt = ir_p.primary_size_pt.unwrap_or(body_median_size);
                let avg_char_mm = (font_pt * 0.175).max(1.8);
                let chars_per_line = (usable_line_mm / avg_char_mm).max(12.0);
                let soft_line_breaks = ir_p.text.matches('\n').count() + 1;
                let char_len = ir_p.trimmed.chars().count();
                let est_lines = if char_len == 0 {
                    1usize
                } else {
                    (((char_len as f64) / chars_per_line).ceil() as usize)
                        .max(soft_line_breaks)
                        .max(1)
                };

                let sp_before_pt = twips_to_pt(ir_p.spacing_before);
                let sp_after_pt = twips_to_pt(ir_p.spacing_after);
                let line_mult = match ir_p.line_rule.as_str() {
                    "exact" | "atLeast" => round2((ir_p.line_spacing as f64) / (font_pt * 20.0).max(120.0)),
                    _ => round2(ir_p.line_spacing as f64 / 240.0),
                }
                .max(0.8);
                let inline_visual_h_mm = extracted
                    .iter()
                    .filter(|v| !v.is_text_box && v.anchor_mode == "inline")
                    .map(|v| v.height_mm)
                    .fold(0.0f64, f64::max);
                let para_h_mm = round1(
                    (sp_before_pt + sp_after_pt) * 0.3528
                        + (est_lines as f64) * (font_pt * 0.3528 * line_mult.max(1.05))
                        + inline_visual_h_mm,
                )
                .max(4.5);

                if cur_y_mm > 0.0 && cur_y_mm + para_h_mm > printable_h_mm + 1.0 {
                    if cur_col + 1 < col_count {
                        cur_col += 1;
                        cur_y_mm = 0.0;
                    } else {
                        current_page += 1;
                        cur_col = 0;
                        cur_y_mm = 0.0;
                        ensure_page_bucket(current_page, current_sec_idx, &mut page_buckets);
                    }
                }

                let start_page = current_page;
                let x_left = round1(
                    margin_left_mm + (cur_col as f64) * (col_w_mm + col_space_mm) + ind_l_mm,
                );
                let y_top = round1(margin_top_mm + cur_y_mm);
                let p_locator = ElementLocator {
                    part_name: "word/document.xml".to_string(),
                    element_path: ir_p.element_path.clone(),
                    body_child_index: Some(b_idx),
                    section_index: ir_p.section_index,
                    estimated_page_start: start_page,
                    estimated_page_end: start_page,
                    column_index: cur_col,
                    x_left_mm: x_left,
                    y_top_mm: y_top,
                    width_mm: round1(usable_line_mm),
                    height_mm: para_h_mm,
                };

                let para_id = format!("p-{}", ir_p.global_index);
                let gap_from_prev_pt = round1(prev_para_space_after_pt + sp_before_pt);
                prev_para_space_after_pt = sp_after_pt;

                paragraph_map.push(ParagraphMapEntry {
                    para_id: para_id.clone(),
                    global_index: ir_p.global_index,
                    kind,
                    role_label: label_for_kind(kind).to_string(),
                    excerpt: excerpt_str(&ir_p.trimmed, 120),
                    style_id: ir_p.style_id.clone(),
                    font_name: ir_p.primary_font.clone(),
                    font_size_pt: ir_p.primary_size_pt,
                    alignment: ir_p.effective_jc.clone(),
                    indent_left_mm: ind_l_mm,
                    indent_right_mm: ind_r_mm,
                    indent_first_line_mm: ind_fl_mm,
                    space_before_pt: sp_before_pt,
                    space_after_pt: sp_after_pt,
                    line_spacing_mult: line_mult,
                    widow_control: flags.widow_control,
                    keep_next: flags.keep_next,
                    keep_lines: flags.keep_lines,
                    page_break_before: flags.page_break_before,
                    in_table_id: None,
                    in_text_box_id: None,
                    estimated_lines: est_lines,
                    gap_from_prev_pt,
                    locator: p_locator.clone(),
                });

                if let Some(bucket) = page_buckets.get_mut(&start_page) {
                    bucket.element_ids.push(para_id.clone());
                    bucket.used_height_mm = round1((cur_y_mm + para_h_mm).min(printable_h_mm * 1.2));
                }
                if let Some(sr) = sec_page_ranges.get_mut(&ir_p.section_index) {
                    sr.1 = sr.1.max(start_page);
                }

                // Add DocumentMap nodes for structural paragraphs (and first MainBody anchor if none yet)
                let should_add_doc_node = match kind {
                    RecognizedStructureKind::MainBody => {
                        if !body_node_inserted && !ir_p.trimmed.is_empty() && !ir_p.trimmed.starts_with('-') {
                            body_node_inserted = true;
                            true
                        } else {
                            false
                        }
                    }
                    _ => !ir_p.trimmed.is_empty(),
                };
                if should_add_doc_node {
                    doc_node_seq += 1;
                    let node_id = format!("node-{}", doc_node_seq);
                    let lvl = level_for_kind(kind);
                    let parent_id = if kind == RecognizedStructureKind::AppendixBlock {
                        hierarchy_stack.clear();
                        current_appendix_node = Some(node_id.clone());
                        None
                    } else if matches!(
                        kind,
                        RecognizedStructureKind::DocumentHeader
                            | RecognizedStructureKind::DocTypeTitle
                            | RecognizedStructureKind::DocSubject
                            | RecognizedStructureKind::MainTitle
                    ) {
                        None
                    } else if matches!(
                        kind,
                        RecognizedStructureKind::HeadingLevel1
                            | RecognizedStructureKind::HeadingLevel2
                            | RecognizedStructureKind::HeadingLevel3
                            | RecognizedStructureKind::HeadingLevel4
                    ) {
                        while let Some((top_lvl, _)) = hierarchy_stack.last() {
                            if *top_lvl >= lvl {
                                hierarchy_stack.pop();
                            } else {
                                break;
                            }
                        }
                        let pid = hierarchy_stack
                            .last()
                            .map(|(_, id)| id.clone())
                            .or_else(|| current_appendix_node.clone());
                        hierarchy_stack.push((lvl, node_id.clone()));
                        pid
                    } else if matches!(
                        kind,
                        RecognizedStructureKind::SignatureBlock | RecognizedStructureKind::RecipientBlock
                    ) {
                        None
                    } else {
                        hierarchy_stack
                            .last()
                            .map(|(_, id)| id.clone())
                            .or_else(|| current_appendix_node.clone())
                    };

                    doc_nodes.push(DocumentMapNode {
                        node_id,
                        parent_id,
                        children_ids: Vec::new(),
                        kind,
                        label: label_for_kind(kind).to_string(),
                        excerpt: excerpt_str(&ir_p.trimmed, 96),
                        level: lvl,
                        confidence: round2(confidence),
                        signals,
                        locator: p_locator.clone(),
                    });
                }

                // Process extracted visual objects (Images & Text Boxes) inside this paragraph
                let parent_for_visuals = hierarchy_stack
                    .last()
                    .map(|(_, id)| id.clone())
                    .or_else(|| current_appendix_node.clone());
                register_visuals_for_element(
                    extracted,
                    ir_p.section_index,
                    b_idx,
                    start_page,
                    cur_col,
                    x_left,
                    y_top,
                    printable_w_mm,
                    printable_h_mm,
                    parent_for_visuals,
                    ir,
                    &style_widow,
                    &mut visual_seq,
                    &mut txbx_seq,
                    &mut doc_node_seq,
                    &mut visual_objects,
                    &mut doc_nodes,
                    &mut paragraph_map,
                    &mut page_buckets,
                );

                // Advance vertical cursor after paragraph
                cur_y_mm += para_h_mm;
                if flags.has_explicit_page_break {
                    current_page += 1;
                    cur_col = 0;
                    cur_y_mm = 0.0;
                    ensure_page_bucket(current_page, current_sec_idx, &mut page_buckets);
                } else if flags.has_column_break && col_count > 1 {
                    if cur_col + 1 < col_count {
                        cur_col += 1;
                        cur_y_mm = 0.0;
                    } else {
                        current_page += 1;
                        cur_col = 0;
                        cur_y_mm = 0.0;
                        ensure_page_bucket(current_page, current_sec_idx, &mut page_buckets);
                    }
                }
            }
            "w:tbl" => {
                let Some(&ir_tbl) = table_by_bidx.get(&b_idx) else {
                    continue;
                };
                let sec = section_by_idx.get(&ir_tbl.section_index);
                if ir_tbl.section_index > current_sec_idx {
                    current_sec_idx = ir_tbl.section_index;
                    let break_type = sec
                        .map(|s| s.summary.break_type.as_str())
                        .unwrap_or("nextPage");
                    if break_type != "continuous" {
                        current_page += 1;
                        cur_col = 0;
                        cur_y_mm = 0.0;
                    }
                    ensure_page_bucket(current_page, current_sec_idx, &mut page_buckets);
                    sec_page_ranges
                        .entry(current_sec_idx)
                        .or_insert((current_page, current_page));
                }

                let tbl_id = format!("tbl-{}", ir_tbl.table_index);
                let col_widths_mm = parse_table_col_widths_mm(
                    el,
                    ir_tbl.summary.col_count.max(1),
                    twips_to_mm(ir_tbl.summary.total_width_twips),
                );
                let tbl_pr = el.child("w:tblPr");
                let tbl_ind_tw: i32 = tbl_pr
                    .and_then(|tp| tp.child("w:tblInd"))
                    .and_then(|ti| ti.get_attr("w:w"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let tbl_jc = tbl_pr
                    .and_then(|tp| tp.child("w:jc"))
                    .and_then(|j| j.get_attr("w:val"))
                    .unwrap_or("left")
                    .to_string();
                let has_merged_cells = table_has_merged_cells(el);

                let total_w_mm = twips_to_mm(ir_tbl.summary.total_width_twips);
                let print_w_mm = twips_to_mm(ir_tbl.summary.printable_width_twips);
                let printable_h_mm = sec
                    .map(|s| twips_to_mm((s.pg_h_twips - s.top_twips - s.bottom_twips).max(3000)))
                    .unwrap_or(257.0);
                let margin_left_mm = sec.map(|s| s.summary.margin_left_mm).unwrap_or(30.0);
                let margin_top_mm = sec.map(|s| s.summary.margin_top_mm).unwrap_or(20.0);

                // Rough vertical estimate per row considering cell text length vs column width
                let tbl_h_mm = estimate_table_height_mm(el, &col_widths_mm);
                if cur_y_mm > 0.0 && cur_y_mm + (tbl_h_mm.min(45.0)) > printable_h_mm {
                    current_page += 1;
                    cur_col = 0;
                    cur_y_mm = 0.0;
                    ensure_page_bucket(current_page, current_sec_idx, &mut page_buckets);
                }
                let start_page = current_page;
                let end_page = if cur_y_mm + tbl_h_mm > printable_h_mm {
                    let extra = ((cur_y_mm + tbl_h_mm - printable_h_mm) / printable_h_mm.max(80.0)).ceil() as usize;
                    for p_no in (start_page + 1)..=(start_page + extra) {
                        ensure_page_bucket(p_no, current_sec_idx, &mut page_buckets);
                        if let Some(bk) = page_buckets.get_mut(&p_no) {
                            bk.used_height_mm = round1(printable_h_mm * 0.85);
                            bk.element_ids.push(tbl_id.clone());
                        }
                    }
                    current_page = start_page + extra;
                    cur_y_mm = (cur_y_mm + tbl_h_mm - extra as f64 * printable_h_mm).clamp(15.0, printable_h_mm * 0.85);
                    current_page
                } else {
                    cur_y_mm += tbl_h_mm;
                    start_page
                };

                let overflow_mm = round1((total_w_mm - print_w_mm).max(0.0));
                let overflows_width = overflow_mm > 2.1;
                let rec_orient = if ir_tbl.admin_kind == AdminTableKind::None
                    && (overflows_width && (ir_tbl.summary.col_count >= 6 || total_w_mm > print_w_mm * 1.15))
                {
                    "landscape".to_string()
                } else {
                    sec.map(|s| s.summary.orientation.clone())
                        .unwrap_or_else(|| "portrait".to_string())
                };

                let t_locator = ElementLocator {
                    part_name: "word/document.xml".to_string(),
                    element_path: ir_tbl.summary.element_path.clone(),
                    body_child_index: Some(b_idx),
                    section_index: ir_tbl.section_index,
                    estimated_page_start: start_page,
                    estimated_page_end: end_page,
                    column_index: cur_col,
                    x_left_mm: round1(margin_left_mm + twips_to_mm(tbl_ind_tw)),
                    y_top_mm: round1(margin_top_mm + cur_y_mm.min(printable_h_mm)),
                    width_mm: total_w_mm,
                    height_mm: tbl_h_mm,
                };

                let role_str = match ir_tbl.admin_kind {
                    AdminTableKind::NationalHeader => "adminHeader",
                    AdminTableKind::SignatureOrRecipient => "signatureLayout",
                    AdminTableKind::None => "dataTable",
                };

                table_map.push(TableMapEntry {
                    table_id: tbl_id.clone(),
                    table_index: ir_tbl.table_index,
                    table_role: role_str.to_string(),
                    row_count: ir_tbl.summary.row_count,
                    col_count: ir_tbl.summary.col_count,
                    col_widths_mm: col_widths_mm.clone(),
                    total_width_mm: total_w_mm,
                    printable_width_mm: print_w_mm,
                    indent_left_mm: twips_to_mm(tbl_ind_tw),
                    alignment: tbl_jc,
                    has_repeat_header: ir_tbl.summary.has_repeat_header,
                    has_merged_cells,
                    locked_long_row_count: ir_tbl.locked_long_row_indices.len(),
                    overflows_width,
                    overflow_mm,
                    recommended_orientation: rec_orient,
                    locator: t_locator.clone(),
                });

                if let Some(bk) = page_buckets.get_mut(&start_page) {
                    if !bk.element_ids.contains(&tbl_id) {
                        bk.element_ids.push(tbl_id.clone());
                    }
                    if start_page == end_page {
                        bk.used_height_mm = round1(cur_y_mm.min(printable_h_mm));
                    } else {
                        bk.used_height_mm = round1(printable_h_mm * 0.95);
                    }
                    if overflows_width {
                        bk.has_overflow = true;
                    }
                }
                if let Some(sr) = sec_page_ranges.get_mut(&ir_tbl.section_index) {
                    sr.1 = sr.1.max(end_page);
                }

                // Record table node(s) in DocumentMap + extract cell paragraphs & embedded visuals
                let table_text_preview = excerpt_str(&el.extract_text(), 90);
                match ir_tbl.admin_kind {
                    AdminTableKind::NationalHeader => {
                        seen_header_or_title = true;
                        doc_node_seq += 1;
                        doc_nodes.push(DocumentMapNode {
                            node_id: format!("node-{}", doc_node_seq),
                            parent_id: None,
                            children_ids: Vec::new(),
                            kind: RecognizedStructureKind::DocumentHeader,
                            label: "Phần đầu văn bản (Bảng 2 cột)".to_string(),
                            excerpt: table_text_preview,
                            level: 0,
                            confidence: 0.99,
                            signals: vec![
                                "openxml:tbl_2col".to_string(),
                                "zone:top_of_document".to_string(),
                            ],
                            locator: t_locator.clone(),
                        });
                    }
                    AdminTableKind::SignatureOrRecipient => {
                        seen_signature_zone = true;
                        let cols = extract_signature_table_previews(el);
                        if !cols.0.is_empty() {
                            doc_node_seq += 1;
                            doc_nodes.push(DocumentMapNode {
                                node_id: format!("node-{}", doc_node_seq),
                                parent_id: None,
                                children_ids: Vec::new(),
                                kind: RecognizedStructureKind::RecipientBlock,
                                label: "Khối Nơi nhận".to_string(),
                                excerpt: cols.0,
                                level: 0,
                                confidence: 0.97,
                                signals: vec!["openxml:sig_table_col0".to_string()],
                                locator: t_locator.clone(),
                            });
                        }
                        if !cols.1.is_empty() {
                            doc_node_seq += 1;
                            doc_nodes.push(DocumentMapNode {
                                node_id: format!("node-{}", doc_node_seq),
                                parent_id: None,
                                children_ids: Vec::new(),
                                kind: RecognizedStructureKind::SignatureBlock,
                                label: "Khối Chữ ký & Thẩm quyền".to_string(),
                                excerpt: cols.1,
                                level: 0,
                                confidence: 0.98,
                                signals: vec!["openxml:sig_table_right".to_string()],
                                locator: t_locator.clone(),
                            });
                        }
                    }
                    AdminTableKind::None => {
                        doc_node_seq += 1;
                        let parent_id = hierarchy_stack
                            .last()
                            .map(|(_, id)| id.clone())
                            .or_else(|| current_appendix_node.clone());
                        doc_nodes.push(DocumentMapNode {
                            node_id: format!("node-{}", doc_node_seq),
                            parent_id,
                            children_ids: Vec::new(),
                            kind: RecognizedStructureKind::DataTable,
                            label: format!(
                                "Bảng {} ({}×{}, {:.1}mm)",
                                ir_tbl.table_index + 1,
                                ir_tbl.summary.row_count,
                                ir_tbl.summary.col_count,
                                total_w_mm
                            ),
                            excerpt: table_text_preview,
                            level: 0,
                            confidence: 0.98,
                            signals: vec![
                                format!("openxml:tbl[{}]", ir_tbl.table_index),
                                if overflows_width {
                                    "layout:overflows_width".to_string()
                                } else {
                                    "layout:fits_printable_width".to_string()
                                },
                            ],
                            locator: t_locator.clone(),
                        });
                    }
                }

                // Record table cell paragraphs in ParagraphMap + any visuals inside cells
                for (r_idx, tr) in el.children_elements().filter(|c| c.name == "w:tr").enumerate() {
                    for (c_idx, tc) in tr.children_elements().filter(|c| c.name == "w:tc").enumerate() {
                        for (cp_idx, cp) in tc.children_elements().filter(|c| c.name == "w:p").enumerate() {
                            let cpath = format!("body/tbl[{}]/tr[{}]/tc[{}]/p[{}]", b_idx, r_idx, c_idx, cp_idx);
                            if let Some(&ir_cp) = para_by_path.get(cpath.as_str()) {
                                let cflags = inspect_para_xml_flags(cp, &ir.styles, &style_widow);
                                let ckind = match classify_admin_table(el) {
                                    AdminTableKind::NationalHeader => RecognizedStructureKind::DocumentHeader,
                                    AdminTableKind::SignatureOrRecipient => {
                                        if c_idx == 0 {
                                            RecognizedStructureKind::RecipientBlock
                                        } else {
                                            RecognizedStructureKind::SignatureBlock
                                        }
                                    }
                                    AdminTableKind::None => RecognizedStructureKind::MainBody,
                                };
                                let cw_mm = col_widths_mm.get(c_idx).copied().unwrap_or(28.0);
                                let c_font_pt = ir_cp.primary_size_pt.unwrap_or(body_median_size);
                                let c_lines = if ir_cp.trimmed.is_empty() {
                                    1
                                } else {
                                    ((ir_cp.trimmed.chars().count() as f64)
                                        / (cw_mm / (c_font_pt * 0.18).max(1.8)).max(5.0))
                                    .ceil() as usize
                                }
                                .max(1);
                                paragraph_map.push(ParagraphMapEntry {
                                    para_id: format!("p-{}", ir_cp.global_index),
                                    global_index: ir_cp.global_index,
                                    kind: ckind,
                                    role_label: ir_cp.role.label_vi().to_string(),
                                    excerpt: excerpt_str(&ir_cp.trimmed, 100),
                                    style_id: ir_cp.style_id.clone(),
                                    font_name: ir_cp.primary_font.clone(),
                                    font_size_pt: ir_cp.primary_size_pt,
                                    alignment: ir_cp.effective_jc.clone(),
                                    indent_left_mm: twips_to_mm(ir_cp.indent_left),
                                    indent_right_mm: twips_to_mm(ir_cp.indent_right),
                                    indent_first_line_mm: twips_to_mm(ir_cp.indent_first_line),
                                    space_before_pt: twips_to_pt(ir_cp.spacing_before),
                                    space_after_pt: twips_to_pt(ir_cp.spacing_after),
                                    line_spacing_mult: round2(ir_cp.line_spacing as f64 / 240.0),
                                    widow_control: cflags.widow_control,
                                    keep_next: cflags.keep_next,
                                    keep_lines: cflags.keep_lines,
                                    page_break_before: cflags.page_break_before,
                                    in_table_id: Some(tbl_id.clone()),
                                    in_text_box_id: None,
                                    estimated_lines: c_lines,
                                    gap_from_prev_pt: 0.0,
                                    locator: ElementLocator {
                                        part_name: "word/document.xml".to_string(),
                                        element_path: cpath,
                                        body_child_index: Some(b_idx),
                                        section_index: ir_cp.section_index,
                                        estimated_page_start: start_page,
                                        estimated_page_end: end_page,
                                        column_index: c_idx,
                                        x_left_mm: t_locator.x_left_mm,
                                        y_top_mm: t_locator.y_top_mm,
                                        width_mm: cw_mm,
                                        height_mm: round1(c_lines as f64 * 4.5),
                                    },
                                });

                                let mut cell_vis = Vec::new();
                                extract_visuals_dedup(cp, &mut cell_vis);
                                register_visuals_for_element(
                                    cell_vis,
                                    ir_cp.section_index,
                                    b_idx,
                                    start_page,
                                    c_idx,
                                    t_locator.x_left_mm,
                                    t_locator.y_top_mm,
                                    cw_mm,
                                    printable_h_mm,
                                    None,
                                    ir,
                                    &style_widow,
                                    &mut visual_seq,
                                    &mut txbx_seq,
                                    &mut doc_node_seq,
                                    &mut visual_objects,
                                    &mut doc_nodes,
                                    &mut paragraph_map,
                                    &mut page_buckets,
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Wire parent -> children_ids in DocumentMap
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    for dn in &doc_nodes {
        if let Some(ref pid) = dn.parent_id {
            children_map.entry(pid.clone()).or_default().push(dn.node_id.clone());
        }
    }
    for dn in &mut doc_nodes {
        if let Some(kids) = children_map.remove(&dn.node_id) {
            dn.children_ids = kids;
        }
    }

    // Build SectionMap
    let mut section_map: Vec<SectionMapEntry> = Vec::with_capacity(ir.sections.len());
    for s in &ir.sections {
        let idx = s.summary.section_index;
        let (ps, pe) = sec_page_ranges.get(&idx).copied().unwrap_or((1, current_page.max(1)));
        section_map.push(SectionMapEntry {
            section_id: format!("sec-{}", idx),
            section_index: idx,
            break_type: s.summary.break_type.clone(),
            orientation: s.summary.orientation.clone(),
            paper_width_mm: s.summary.paper_width_mm,
            paper_height_mm: s.summary.paper_height_mm,
            margin_top_mm: s.summary.margin_top_mm,
            margin_bottom_mm: s.summary.margin_bottom_mm,
            margin_left_mm: s.summary.margin_left_mm,
            margin_right_mm: s.summary.margin_right_mm,
            printable_width_mm: twips_to_mm(s.printable_width_twips),
            printable_height_mm: twips_to_mm((s.pg_h_twips - s.top_twips - s.bottom_twips).max(3000)),
            column_count: s.column_count,
            column_space_mm: twips_to_mm(s.column_space_twips),
            equal_column_width: s.equal_column_width,
            col_widths_mm: s.col_widths_twips.iter().map(|&w| twips_to_mm(w)).collect(),
            header_ids: sec_headers.get(&idx).cloned().unwrap_or_default(),
            footer_ids: sec_footers.get(&idx).cloned().unwrap_or_default(),
            page_start: ps,
            page_end: pe.max(ps),
        });
    }

    // Finalize PageLayoutMap sorted by page_number
    let mut page_layout_map: Vec<PageLayoutEntry> = page_buckets.into_values().collect();
    page_layout_map.sort_by_key(|p| p.page_number);
    for page in &mut page_layout_map {
        page.fill_ratio_pct = round1((page.used_height_mm / page.printable_height_mm.max(50.0)) * 100.0)
            .clamp(0.0, 120.0);
    }

    Ok(IntelligentDocumentMap {
        document_map: doc_nodes,
        paragraph_map,
        table_map,
        section_map,
        style_map,
        header_footer_map,
        page_layout_map,
        visual_objects,
    })
}

#[allow(clippy::too_many_arguments)]
fn register_visuals_for_element(
    extracted: Vec<ExtractedVisualRaw>,
    section_index: usize,
    b_idx: usize,
    start_page: usize,
    cur_col: usize,
    x_left: f64,
    y_top: f64,
    printable_w_mm: f64,
    printable_h_mm: f64,
    parent_node_id: Option<String>,
    ir: &DocumentIr,
    style_widow: &HashMap<String, bool>,
    visual_seq: &mut usize,
    txbx_seq: &mut usize,
    doc_node_seq: &mut usize,
    visual_objects: &mut Vec<VisualObjectMapEntry>,
    doc_nodes: &mut Vec<DocumentMapNode>,
    paragraph_map: &mut Vec<ParagraphMapEntry>,
    page_buckets: &mut HashMap<usize, PageLayoutEntry>,
) {
    for vis in extracted {
        if vis.is_text_box {
            *txbx_seq += 1;
            let txbx_id = format!("txbx-{}", *txbx_seq);
            let mut inner_texts = Vec::new();
            let mut inner_para_count = 0usize;

            for (ip_idx, ip) in vis.inner_paragraphs.iter().enumerate() {
                let raw_t = ip.extract_text();
                let trim_t = raw_t.trim().to_string();
                if !trim_t.is_empty() {
                    inner_texts.push(trim_t.clone());
                }
                inner_para_count += 1;
                let ip_flags = inspect_para_xml_flags(ip, &ir.styles, style_widow);
                let p_pr = ip.child("w:pPr");
                let ip_style_id = p_pr
                    .and_then(|pp| pp.child("w:pStyle"))
                    .and_then(|s| s.get_attr("w:val"))
                    .map(|s| s.to_string());
                let (ip_font, ip_sz) = extract_first_run_font_sz(ip, &ir.styles, ip_style_id.as_deref());
                let g_idx = paragraph_map.len();
                let ip_loc = ElementLocator {
                    part_name: "word/document.xml".to_string(),
                    element_path: format!("body/p[{}]/txbx[{}]/p[{}]", b_idx, *txbx_seq - 1, ip_idx),
                    body_child_index: Some(b_idx),
                    section_index,
                    estimated_page_start: start_page,
                    estimated_page_end: start_page,
                    column_index: cur_col,
                    x_left_mm: x_left,
                    y_top_mm: y_top,
                    width_mm: vis.width_mm,
                    height_mm: 5.5,
                };
                paragraph_map.push(ParagraphMapEntry {
                    para_id: format!("txbx-{}-p-{}", *txbx_seq, ip_idx + 1),
                    global_index: g_idx,
                    kind: RecognizedStructureKind::TextBox,
                    role_label: "Đoạn trong Text Box".to_string(),
                    excerpt: excerpt_str(&trim_t, 120),
                    style_id: ip_style_id,
                    font_name: ip_font,
                    font_size_pt: ip_sz,
                    alignment: p_pr
                        .and_then(|pp| pp.child("w:jc"))
                        .and_then(|j| j.get_attr("w:val"))
                        .unwrap_or("left")
                        .to_string(),
                    indent_left_mm: 0.0,
                    indent_right_mm: 0.0,
                    indent_first_line_mm: 0.0,
                    space_before_pt: 0.0,
                    space_after_pt: 0.0,
                    line_spacing_mult: 1.15,
                    widow_control: ip_flags.widow_control,
                    keep_next: ip_flags.keep_next,
                    keep_lines: ip_flags.keep_lines,
                    page_break_before: false,
                    in_table_id: None,
                    in_text_box_id: Some(txbx_id.clone()),
                    estimated_lines: 1,
                    gap_from_prev_pt: 0.0,
                    locator: ip_loc,
                });
            }

            let combined = excerpt_str(&inner_texts.join(" | "), 120);
            let overflows = vis.width_mm > printable_w_mm + 1.5 || vis.height_mm > printable_h_mm + 1.5;
            let loc = ElementLocator {
                part_name: "word/document.xml".to_string(),
                element_path: format!("body/p[{}]/txbx[{}]", b_idx, *txbx_seq - 1),
                body_child_index: Some(b_idx),
                section_index,
                estimated_page_start: start_page,
                estimated_page_end: start_page,
                column_index: cur_col,
                x_left_mm: x_left,
                y_top_mm: y_top,
                width_mm: vis.width_mm,
                height_mm: vis.height_mm,
            };

            visual_objects.push(VisualObjectMapEntry {
                object_id: txbx_id.clone(),
                object_kind: "textBox".to_string(),
                anchor_mode: vis.anchor_mode.clone(),
                relationship_id: None,
                media_target: None,
                width_mm: vis.width_mm,
                height_mm: vis.height_mm,
                overflows_printable_area: overflows,
                inner_paragraph_count: inner_para_count,
                text_excerpt: combined.clone(),
                locator: loc.clone(),
            });

            *doc_node_seq += 1;
            doc_nodes.push(DocumentMapNode {
                node_id: format!("node-{}", *doc_node_seq),
                parent_id: parent_node_id.clone(),
                children_ids: Vec::new(),
                kind: RecognizedStructureKind::TextBox,
                label: format!("Text Box #{} ({} đoạn)", *txbx_seq, inner_para_count),
                excerpt: if combined.is_empty() {
                    "(Text Box trống)".to_string()
                } else {
                    combined
                },
                level: 0,
                confidence: 0.98,
                signals: vec![
                    "openxml:txbxContent".to_string(),
                    format!("mode:{}", vis.anchor_mode),
                ],
                locator: loc,
            });
            if let Some(bk) = page_buckets.get_mut(&start_page) {
                bk.element_ids.push(txbx_id);
                if overflows {
                    bk.has_overflow = true;
                }
            }
        } else {
            *visual_seq += 1;
            let img_id = format!("img-{}", *visual_seq);
            let media_target = vis
                .rel_id
                .as_ref()
                .and_then(|rid| ir.rels_map.get(rid))
                .map(|(_, target)| target.clone());
            let overflows =
                vis.width_mm > printable_w_mm + 1.5 || vis.height_mm > printable_h_mm + 1.5;
            let loc = ElementLocator {
                part_name: "word/document.xml".to_string(),
                element_path: format!("body/p[{}]/drawing[{}]", b_idx, *visual_seq - 1),
                body_child_index: Some(b_idx),
                section_index,
                estimated_page_start: start_page,
                estimated_page_end: start_page,
                column_index: cur_col,
                x_left_mm: x_left,
                y_top_mm: y_top,
                width_mm: vis.width_mm,
                height_mm: vis.height_mm,
            };

            visual_objects.push(VisualObjectMapEntry {
                object_id: img_id.clone(),
                object_kind: "image".to_string(),
                anchor_mode: vis.anchor_mode.clone(),
                relationship_id: vis.rel_id.clone(),
                media_target: media_target.clone(),
                width_mm: vis.width_mm,
                height_mm: vis.height_mm,
                overflows_printable_area: overflows,
                inner_paragraph_count: 0,
                text_excerpt: media_target.clone().unwrap_or_default(),
                locator: loc.clone(),
            });

            *doc_node_seq += 1;
            doc_nodes.push(DocumentMapNode {
                node_id: format!("node-{}", *doc_node_seq),
                parent_id: parent_node_id.clone(),
                children_ids: Vec::new(),
                kind: RecognizedStructureKind::ImageAsset,
                label: format!(
                    "Hình ảnh #{} ({:.1}×{:.1} mm)",
                    *visual_seq, vis.width_mm, vis.height_mm
                ),
                excerpt: media_target.unwrap_or_else(|| "Ảnh nhúng OpenXML".to_string()),
                level: 0,
                confidence: 0.99,
                signals: vec![
                    format!("openxml:{}", vis.anchor_mode),
                    if overflows {
                        "layout:overflows_printable".to_string()
                    } else {
                        "layout:within_margins".to_string()
                    },
                ],
                locator: loc,
            });
            if let Some(bk) = page_buckets.get_mut(&start_page) {
                bk.element_ids.push(img_id);
                if overflows {
                    bk.has_overflow = true;
                }
            }
        }
    }
}

fn extract_first_run_font_sz(
    p: &XmlElement,
    styles: &StyleCatalog,
    p_style: Option<&str>,
) -> (String, Option<f64>) {
    for r in p.children_elements().filter(|e| e.name == "w:r") {
        if let Some(r_pr) = r.child("w:rPr") {
            let f = read_rpr_font(r_pr)
                .or_else(|| styles.effective_font(p_style, None))
                .unwrap_or_default();
            let sz = read_rpr_sz_pt(r_pr).or_else(|| styles.effective_size_pt(p_style, None));
            return (f, sz);
        }
    }
    (
        styles.effective_font(p_style, None).unwrap_or_default(),
        styles.effective_size_pt(p_style, None),
    )
}

fn parse_table_col_widths_mm(tbl: &XmlElement, col_count: usize, fallback_total_mm: f64) -> Vec<f64> {
    if let Some(grid) = tbl.child("w:tblGrid") {
        let cols: Vec<f64> = grid
            .children_elements()
            .filter(|e| e.name == "w:gridCol")
            .filter_map(|e| e.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()))
            .map(twips_to_mm)
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    if let Some(first_tr) = tbl.children_elements().find(|e| e.name == "w:tr") {
        let cols: Vec<f64> = first_tr
            .children_elements()
            .filter(|e| e.name == "w:tc")
            .filter_map(|tc| {
                tc.child("w:tcPr")
                    .and_then(|tp| tp.child("w:tcW"))
                    .and_then(|w| w.get_attr("w:w"))
                    .and_then(|v| v.parse::<i32>().ok())
            })
            .map(twips_to_mm)
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    vec![round1(fallback_total_mm / col_count.max(1) as f64); col_count.max(1)]
}

fn table_has_merged_cells(tbl: &XmlElement) -> bool {
    for tr in tbl.children_elements().filter(|c| c.name == "w:tr") {
        for tc in tr.children_elements().filter(|c| c.name == "w:tc") {
            if let Some(tp) = tc.child("w:tcPr") {
                if tp.child("w:vMerge").is_some()
                    || tp.child("w:hMerge").is_some()
                    || tp
                        .child("w:gridSpan")
                        .and_then(|g| g.get_attr("w:val"))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        > 1
                {
                    return true;
                }
            }
        }
    }
    false
}

fn estimate_table_height_mm(tbl: &XmlElement, col_widths_mm: &[f64]) -> f64 {
    let mut total = 0.0f64;
    for tr in tbl.children_elements().filter(|c| c.name == "w:tr") {
        let mut max_row_mm = 7.0f64;
        for (c_idx, tc) in tr.children_elements().filter(|c| c.name == "w:tc").enumerate() {
            let cw = col_widths_mm.get(c_idx).copied().unwrap_or(30.0).max(12.0);
            let chars_per_line = (cw / 2.3).max(4.0);
            let mut cell_h = 3.0f64;
            for p in tc.children_elements().filter(|c| c.name == "w:p") {
                let len = p.extract_text().trim().chars().count().max(1);
                let lines = ((len as f64) / chars_per_line).ceil().max(1.0);
                cell_h += lines * 5.2;
            }
            max_row_mm = max_row_mm.max(cell_h);
        }
        total += max_row_mm;
    }
    round1(total.max(10.0))
}

fn extract_signature_table_previews(tbl: &XmlElement) -> (String, String) {
    let mut left_parts = Vec::new();
    let mut right_parts = Vec::new();
    for tr in tbl.children_elements().filter(|c| c.name == "w:tr") {
        for (c_idx, tc) in tr.children_elements().filter(|c| c.name == "w:tc").enumerate() {
            let txt = tc.extract_text();
            let trim = txt.trim();
            if !trim.is_empty() {
                if c_idx == 0 {
                    left_parts.push(trim.to_string());
                } else {
                    right_parts.push(trim.to_string());
                }
            }
        }
    }
    (
        excerpt_str(&left_parts.join(" · "), 90),
        excerpt_str(&right_parts.join(" · "), 90),
    )
}

fn count_hf_contents(xml: Option<&str>) -> (usize, usize, bool) {
    let Some(raw) = xml else {
        return (0, 0, false);
    };
    let Ok(doc) = XmlDocument::parse(raw) else {
        return (0, 0, false);
    };
    let paras = doc.root.children_elements().filter(|e| e.name == "w:p").count();
    let tbls = doc.root.children_elements().filter(|e| e.name == "w:tbl").count();
    let mut vis = Vec::new();
    extract_visuals_dedup(&doc.root, &mut vis);
    (paras, tbls, !vis.is_empty())
}

fn normalize_rel_word_part(target: &str) -> String {
    let clean = target.trim_start_matches('/').replace('\\', "/");
    if clean.to_ascii_lowercase().starts_with("word/") {
        clean
    } else {
        format!("word/{}", clean)
    }
}

fn label_for_kind(kind: RecognizedStructureKind) -> &'static str {
    match kind {
        RecognizedStructureKind::DocumentHeader => "Phần đầu văn bản",
        RecognizedStructureKind::DocTypeTitle => "Tên loại văn bản",
        RecognizedStructureKind::DocSubject => "Trích yếu",
        RecognizedStructureKind::MainTitle => "Tiêu đề chính",
        RecognizedStructureKind::MainBody => "Nội dung chính",
        RecognizedStructureKind::HeadingLevel1 => "Tiêu đề cấp 1",
        RecognizedStructureKind::HeadingLevel2 => "Tiêu đề cấp 2",
        RecognizedStructureKind::HeadingLevel3 => "Tiêu đề cấp 3",
        RecognizedStructureKind::HeadingLevel4 => "Tiêu đề cấp 4",
        RecognizedStructureKind::DataTable => "Bảng biểu",
        RecognizedStructureKind::ImageAsset => "Hình ảnh",
        RecognizedStructureKind::SignatureBlock => "Chữ ký",
        RecognizedStructureKind::RecipientBlock => "Nơi nhận",
        RecognizedStructureKind::AppendixBlock => "Phụ lục",
        RecognizedStructureKind::HeaderFooter => "Header / Footer",
        RecognizedStructureKind::TextBox => "Text Box",
    }
}

fn level_for_kind(kind: RecognizedStructureKind) -> u8 {
    match kind {
        RecognizedStructureKind::HeadingLevel1 => 1,
        RecognizedStructureKind::HeadingLevel2 => 2,
        RecognizedStructureKind::HeadingLevel3 => 3,
        RecognizedStructureKind::HeadingLevel4 => 4,
        _ => 0,
    }
}

fn excerpt_str(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", head)
    }
}
