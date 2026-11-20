use super::cancel::CancelToken;
use crate::engine::formatter::{classify_admin_table, collapse_spaces_upper, AdminTableKind};
use crate::engine::layout_intelligence::mapper::{emu_to_mm, round1, round2, twips_to_mm, twips_to_pt};
use crate::engine::openxml_arch::SelectivePackageView;
use crate::engine::std_engine::ir::{read_rpr_flag, read_rpr_font, read_rpr_sz_pt, StyleCatalog};
use crate::engine::xml_helpers::{XmlDocument, XmlElement, XmlNode};
use crate::models::{
    PreviewBlock, PreviewBorderEdge, PreviewImageRef, PreviewParagraphBlock, PreviewSectionSpec,
    PreviewSpan, PreviewTableBlock, PreviewTableBorders, PreviewTableCell, PreviewTableRow,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ExtractedPreviewDoc {
    pub sections: Vec<PreviewSectionSpec>,
    pub blocks: Vec<PreviewBlock>,
    pub paragraph_count: usize,
    pub table_count: usize,
    pub image_count: usize,
}

#[derive(Debug, Default)]
struct SimpleNumbering {
    num_to_abs: HashMap<String, String>,
    abs_levels: HashMap<(String, String), (String, String)>, // (numFmt, lvlText)
    counters: HashMap<(String, String), usize>,
}

impl SimpleNumbering {
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
                        let fmt = lvl
                            .child("w:numFmt")
                            .and_then(|n| n.get_attr("w:val"))
                            .unwrap_or("decimal")
                            .to_string();
                        let txt = lvl
                            .child("w:lvlText")
                            .and_then(|n| n.get_attr("w:val"))
                            .unwrap_or("")
                            .to_string();
                        out.abs_levels
                            .insert((abs_id.to_string(), ilvl), (fmt, txt));
                    }
                }
            } else if el.name == "w:num" {
                if let (Some(nid), Some(abs)) = (
                    el.get_attr("w:numId"),
                    el.child("w:abstractNumId").and_then(|a| a.get_attr("w:val")),
                ) {
                    out.num_to_abs.insert(nid.to_string(), abs.to_string());
                }
            }
        }
        out
    }

    fn next_prefix(&mut self, num_id: &str, ilvl: &str) -> String {
        let Some(abs_id) = self.num_to_abs.get(num_id).cloned() else {
            return String::new();
        };
        let fmt = self
            .abs_levels
            .get(&(abs_id, ilvl.to_string()))
            .map(|(f, t)| (f.clone(), t.clone()))
            .unwrap_or_else(|| ("decimal".to_string(), "".to_string()));

        if fmt.0 == "bullet" {
            let sym = if fmt.1.contains('\u{F0B7}') || fmt.1.is_empty() {
                "•"
            } else if fmt.1 == "-" {
                "-"
            } else {
                "•"
            };
            return format!("{} ", sym);
        }

        let key = (num_id.to_string(), ilvl.to_string());
        let next_n = self.counters.get(&key).copied().unwrap_or(0) + 1;
        self.counters.insert(key, next_n);

        match fmt.0.as_str() {
            "lowerLetter" => {
                let ch = ((b'a' + ((next_n - 1) % 26) as u8) as char).to_string();
                format!("{}) ", ch)
            }
            "upperLetter" => {
                let ch = ((b'A' + ((next_n - 1) % 26) as u8) as char).to_string();
                format!("{}. ", ch)
            }
            "lowerRoman" => format!("{}. ", to_roman(next_n).to_lowercase()),
            "upperRoman" => format!("{}. ", to_roman(next_n)),
            _ => format!("{}. ", next_n),
        }
    }
}

fn to_roman(mut n: usize) -> String {
    let vals = [
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (v, s) in vals {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

pub fn extract_from_selective_view(
    view: &SelectivePackageView,
    cancel: Option<&CancelToken>,
) -> Result<ExtractedPreviewDoc, String> {
    let doc_xml = view
        .get_xml("word/document.xml")
        .ok_or_else(|| "Không tìm thấy word/document.xml để dựng xem trước.".to_string())?;
    let doc = XmlDocument::parse(doc_xml)?;
    let body = doc
        .root
        .child("w:body")
        .ok_or_else(|| "Tài liệu không có phần tử <w:body>.".to_string())?;

    let styles = StyleCatalog::from_xml(view.get_xml("word/styles.xml"));
    let mut numbering = SimpleNumbering::from_xml(view.get_xml("word/numbering.xml"));
    let rels = parse_rels_targets(view.get_xml("word/_rels/document.xml.rels"));

    // 1. Collect all sections in order
    let mut raw_sections: Vec<(PreviewSectionSpec, Option<usize>)> = Vec::new();
    for (b_idx, node) in body.children.iter().enumerate() {
        if let Some(tok) = cancel {
            if tok.is_cancelled() {
                return Err("CANCELLED".to_string());
            }
        }
        if let XmlNode::Element(el) = node {
            if el.name == "w:p" {
                if let Some(sect_el) = el.child("w:pPr").and_then(|pp| pp.child("w:sectPr")) {
                    let s_idx = raw_sections.len() + 1;
                    let spec = parse_section_spec(sect_el, s_idx, view, &rels, &styles);
                    raw_sections.push((spec, Some(b_idx)));
                }
            }
        }
    }
    if let Some(sect_el) = body.child("w:sectPr") {
        let s_idx = raw_sections.len() + 1;
        let spec = parse_section_spec(sect_el, s_idx, view, &rels, &styles);
        raw_sections.push((spec, None));
    }
    if raw_sections.is_empty() {
        let fallback = default_a4_section(1);
        raw_sections.push((fallback, None));
    }

    let sections: Vec<PreviewSectionSpec> = raw_sections.iter().map(|(s, _)| s.clone()).collect();
    let inline_sect_para_indices: HashMap<usize, usize> = raw_sections
        .iter()
        .enumerate()
        .filter_map(|(i, (_, b_opt))| b_opt.map(|b| (b, i)))
        .collect();

    // 2. Walk body children sequentially
    let mut blocks: Vec<PreviewBlock> = Vec::new();
    let mut current_sect_idx: usize = 1;
    let mut global_para_idx: usize = 0;
    let mut table_seq: usize = 0;
    let mut image_count: usize = 0;

    let mut near_admin_header = false;
    let mut past_doc_type = false;

    for (b_idx, node) in body.children.iter().enumerate() {
        if let Some(tok) = cancel {
            if tok.is_cancelled() {
                return Err("CANCELLED".to_string());
            }
        }
        let XmlNode::Element(el) = node else {
            continue;
        };

        let sect_spec = sections
            .get(current_sect_idx.saturating_sub(1))
            .unwrap_or(&sections[0]);
        let printable_w_mm =
            (sect_spec.width_mm - sect_spec.margin_left_mm - sect_spec.margin_right_mm).max(40.0);

        match el.name.as_str() {
            "w:p" => {
                let g = global_para_idx;
                global_para_idx += 1;
                let mut p_block = extract_paragraph_block(
                    el,
                    &format!("body/p[{}]", b_idx),
                    g,
                    current_sect_idx,
                    printable_w_mm,
                    &styles,
                    &mut numbering,
                    &rels,
                    None,
                );
                let upper = collapse_spaces_upper(&p_block.plain_text);
                p_block.role = infer_visual_role(
                    el,
                    &p_block.plain_text,
                    &upper,
                    &p_block.alignment,
                    &p_block.spans,
                    g,
                    near_admin_header,
                    past_doc_type,
                );
                if p_block.role == "docTypeHeading" {
                    past_doc_type = true;
                    near_admin_header = false;
                } else if p_block.role == "adminHeader" {
                    near_admin_header = true;
                } else if !p_block.plain_text.trim().is_empty() && p_block.role != "docSubject" {
                    near_admin_header = false;
                }
                image_count += p_block.images.len();
                blocks.push(PreviewBlock::Paragraph(p_block));

                if inline_sect_para_indices.contains_key(&b_idx) {
                    current_sect_idx = (current_sect_idx + 1).min(sections.len());
                }
            }
            "w:tbl" => {
                let t_idx = table_seq;
                table_seq += 1;
                let tbl_block = extract_table_block(
                    el,
                    b_idx,
                    t_idx,
                    current_sect_idx,
                    printable_w_mm,
                    &styles,
                    &mut numbering,
                    &rels,
                    &mut global_para_idx,
                    &mut image_count,
                );
                if tbl_block.table_role == "adminHeader" {
                    near_admin_header = true;
                }
                blocks.push(PreviewBlock::Table(tbl_block));
            }
            _ => {}
        }
    }

    Ok(ExtractedPreviewDoc {
        sections,
        blocks,
        paragraph_count: global_para_idx,
        table_count: table_seq,
        image_count,
    })
}

fn default_a4_section(idx: usize) -> PreviewSectionSpec {
    PreviewSectionSpec {
        section_index: idx,
        paper_label: "A4".to_string(),
        orientation: "portrait".to_string(),
        width_mm: 210.0,
        height_mm: 297.0,
        margin_top_mm: 20.0,
        margin_bottom_mm: 20.0,
        margin_left_mm: 30.0,
        margin_right_mm: 15.0,
        header_distance_mm: 12.5,
        footer_distance_mm: 12.5,
        different_first_page: false,
        page_num_start: None,
        has_dynamic_page_field: false,
        default_header_blocks: Vec::new(),
        first_header_blocks: Vec::new(),
        default_footer_blocks: Vec::new(),
        first_footer_blocks: Vec::new(),
    }
}

pub fn classify_paper_label(w_mm: f64, h_mm: f64) -> String {
    let min_s = w_mm.min(h_mm);
    let max_s = w_mm.max(h_mm);
    if (min_s - 210.0).abs() <= 4.0 && (max_s - 297.0).abs() <= 4.0 {
        "A4".to_string()
    } else if (min_s - 297.0).abs() <= 5.0 && (max_s - 420.0).abs() <= 5.0 {
        "A3".to_string()
    } else if (min_s - 215.9).abs() <= 4.0 && (max_s - 279.4).abs() <= 4.0 {
        "Letter".to_string()
    } else {
        format!("Custom ({}×{}mm)", round1(w_mm), round1(h_mm))
    }
}

fn parse_section_spec(
    sect: &XmlElement,
    sect_idx: usize,
    view: &SelectivePackageView,
    doc_rels: &HashMap<String, String>,
    styles: &StyleCatalog,
) -> PreviewSectionSpec {
    let pg_sz = sect.child("w:pgSz");
    let pg_w: i32 = pg_sz
        .and_then(|s| s.get_attr("w:w"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(11906);
    let pg_h: i32 = pg_sz
        .and_then(|s| s.get_attr("w:h"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(16838);
    let orient = pg_sz
        .and_then(|s| s.get_attr("w:orient"))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| {
            if pg_w > pg_h {
                "landscape".to_string()
            } else {
                "portrait".to_string()
            }
        });

    let mar = sect.child("w:pgMar");
    let top_t: i32 = mar
        .and_then(|m| m.get_attr("w:top"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1134);
    let bot_t: i32 = mar
        .and_then(|m| m.get_attr("w:bottom"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1134);
    let left_t: i32 = mar
        .and_then(|m| m.get_attr("w:left"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1701);
    let right_t: i32 = mar
        .and_then(|m| m.get_attr("w:right"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(850);
    let hdr_t: i32 = mar
        .and_then(|m| m.get_attr("w:header"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(708);
    let ftr_t: i32 = mar
        .and_then(|m| m.get_attr("w:footer"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(708);

    let width_mm = twips_to_mm(pg_w);
    let height_mm = twips_to_mm(pg_h);
    let margin_left_mm = twips_to_mm(left_t);
    let margin_right_mm = twips_to_mm(right_t);
    let printable_w_mm = (width_mm - margin_left_mm - margin_right_mm).max(40.0);

    let different_first_page = sect.child("w:titlePg").is_some();
    let page_num_start = sect
        .child("w:pgNumType")
        .and_then(|n| n.get_attr("w:start"))
        .and_then(|v| v.parse::<u32>().ok());

    let mut default_header_blocks = Vec::new();
    let mut first_header_blocks = Vec::new();
    let mut default_footer_blocks = Vec::new();
    let mut first_footer_blocks = Vec::new();
    let mut has_dynamic_page_field = false;

    for child in sect.children_elements() {
        if child.name == "w:headerReference" || child.name == "w:footerReference" {
            let ref_type = child.get_attr("w:type").unwrap_or("default");
            let Some(rid) = child.get_attr("r:id") else {
                continue;
            };
            let Some(part_path) = doc_rels.get(rid) else {
                continue;
            };
            let blocks = extract_hf_blocks(view, part_path, sect_idx, printable_w_mm, styles);
            if blocks
                .iter()
                .any(|b| b.spans.iter().any(|s| s.is_dynamic_page_field.is_some()))
            {
                has_dynamic_page_field = true;
            }
            if child.name == "w:headerReference" {
                if ref_type == "first" {
                    first_header_blocks = blocks;
                } else if ref_type == "default" || default_header_blocks.is_empty() {
                    default_header_blocks = blocks;
                }
            } else if ref_type == "first" {
                first_footer_blocks = blocks;
            } else if ref_type == "default" || default_footer_blocks.is_empty() {
                default_footer_blocks = blocks;
            }
        }
    }

    PreviewSectionSpec {
        section_index: sect_idx,
        paper_label: classify_paper_label(width_mm, height_mm),
        orientation: orient,
        width_mm,
        height_mm,
        margin_top_mm: twips_to_mm(top_t),
        margin_bottom_mm: twips_to_mm(bot_t),
        margin_left_mm,
        margin_right_mm,
        header_distance_mm: twips_to_mm(hdr_t),
        footer_distance_mm: twips_to_mm(ftr_t),
        different_first_page,
        page_num_start,
        has_dynamic_page_field,
        default_header_blocks,
        first_header_blocks,
        default_footer_blocks,
        first_footer_blocks,
    }
}

fn extract_hf_blocks(
    view: &SelectivePackageView,
    part_path: &str,
    sect_idx: usize,
    printable_w_mm: f64,
    styles: &StyleCatalog,
) -> Vec<PreviewParagraphBlock> {
    let Some(xml) = view.get_xml(part_path) else {
        return Vec::new();
    };
    let Ok(doc) = XmlDocument::parse(xml) else {
        return Vec::new();
    };
    let rels_path = {
        let fname = part_path.rsplit('/').next().unwrap_or(part_path);
        format!("word/_rels/{}.rels", fname)
    };
    let rels = parse_rels_targets(view.get_xml(&rels_path));
    let mut dummy_num = SimpleNumbering::default();
    let mut out = Vec::new();
    for (idx, p) in doc
        .root
        .children_elements()
        .filter(|e| e.name == "w:p")
        .enumerate()
    {
        let block = extract_paragraph_block(
            p,
            &format!("{}/p[{}]", part_path, idx),
            idx,
            sect_idx,
            printable_w_mm,
            styles,
            &mut dummy_num,
            &rels,
            Some("headerFooter"),
        );
        if !block.plain_text.trim().is_empty()
            || !block.images.is_empty()
            || block
                .spans
                .iter()
                .any(|s| s.is_dynamic_page_field.is_some())
        {
            out.push(block);
        }
    }
    out
}

fn parse_rels_targets(xml: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(raw) = xml else {
        return map;
    };
    let Ok(doc) = XmlDocument::parse(raw) else {
        return map;
    };
    for rel in doc
        .root
        .children_elements()
        .filter(|e| e.name == "Relationship")
    {
        if let (Some(id), Some(target)) = (rel.get_attr("Id"), rel.get_attr("Target")) {
            let norm_t = target.trim_start_matches('/').replace('\\', "/");
            let full = if norm_t.starts_with("word/") || norm_t.starts_with("http") {
                norm_t
            } else {
                format!("word/{}", norm_t.trim_start_matches("../"))
            };
            map.insert(id.to_string(), full);
        }
    }
    map
}

#[allow(clippy::too_many_arguments)]
fn extract_paragraph_block(
    p: &XmlElement,
    element_path: &str,
    global_index: usize,
    section_index: usize,
    col_width_mm: f64,
    styles: &StyleCatalog,
    numbering: &mut SimpleNumbering,
    rels: &HashMap<String, String>,
    forced_role: Option<&str>,
) -> PreviewParagraphBlock {
    let p_pr = p.child("w:pPr");
    let style_id = p_pr
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"));
    let st_info = styles.effective_style(style_id);

    let alignment = p_pr
        .and_then(|pp| pp.child("w:jc"))
        .and_then(|j| j.get_attr("w:val"))
        .map(|s| s.to_string())
        .or_else(|| st_info.and_then(|s| s.jc.clone()))
        .unwrap_or_else(|| "left".to_string());

    let ind = p_pr.and_then(|pp| pp.child("w:ind"));
    let first_line_t = ind
        .and_then(|i| i.get_attr("w:firstLine"))
        .and_then(|v| v.parse::<i32>().ok())
        .or_else(|| st_info.and_then(|s| s.indent_first_line))
        .unwrap_or(0);
    let hanging_t = ind
        .and_then(|i| i.get_attr("w:hanging"))
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let left_t = ind
        .and_then(|i| i.get_attr("w:left"))
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let right_t = ind
        .and_then(|i| i.get_attr("w:right"))
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);

    let sp = p_pr.and_then(|pp| pp.child("w:spacing"));
    let sb_t = sp
        .and_then(|s| s.get_attr("w:before"))
        .and_then(|v| v.parse::<i32>().ok())
        .or_else(|| st_info.and_then(|s| s.spacing_before))
        .unwrap_or(0);
    let sa_t = sp
        .and_then(|s| s.get_attr("w:after"))
        .and_then(|v| v.parse::<i32>().ok())
        .or_else(|| st_info.and_then(|s| s.spacing_after))
        .unwrap_or(120);
    let line_raw = sp
        .and_then(|s| s.get_attr("w:line"))
        .and_then(|v| v.parse::<i32>().ok())
        .or_else(|| st_info.and_then(|s| s.line_spacing))
        .unwrap_or(276);
    let line_rule = sp
        .and_then(|s| s.get_attr("w:lineRule"))
        .unwrap_or("auto");

    let (line_spacing_mult, exact_line_pt) = if line_rule == "exact" || line_rule == "atLeast" {
        let pt = (line_raw as f64 / 20.0).max(8.0);
        (round2((pt / 14.0).max(1.0)), Some(round1(pt)))
    } else {
        (round2((line_raw as f64 / 240.0).clamp(0.85, 3.0)), None)
    };

    let keep_next = p_pr.and_then(|pp| pp.child("w:keepNext")).is_some();
    let mut page_break_before = p_pr.and_then(|pp| pp.child("w:pageBreakBefore")).is_some();

    let list_prefix = if let Some(np) = p_pr.and_then(|pp| pp.child("w:numPr")) {
        if let Some(num_id) = np.child("w:numId").and_then(|n| n.get_attr("w:val")) {
            if num_id != "0" {
                let ilvl = np
                    .child("w:ilvl")
                    .and_then(|l| l.get_attr("w:val"))
                    .unwrap_or("0");
                numbering.next_prefix(num_id, ilvl)
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let mut spans: Vec<PreviewSpan> = Vec::new();
    let mut images: Vec<PreviewImageRef> = Vec::new();
    let mut has_leading_page_break = false;
    let mut has_explicit_page_break = false;
    let mut has_last_rendered_break = false;

    let def_font = styles
        .effective_font(style_id, None)
        .unwrap_or_else(|| "Times New Roman".to_string());
    let def_sz = styles.effective_size_pt(style_id, None).unwrap_or(14.0);
    let def_bold = st_info.and_then(|s| s.bold).unwrap_or(false);
    let def_italic = st_info.and_then(|s| s.italic).unwrap_or(false);

    let mut active_field_instr: Option<String> = None;
    let mut inside_fld_result = false;

    for child in &p.children {
        let XmlNode::Element(el) = child else {
            continue;
        };
        if el.name == "w:r" {
            extract_run_contents(
                el,
                styles,
                style_id,
                &def_font,
                def_sz,
                def_bold,
                def_italic,
                col_width_mm,
                rels,
                &mut spans,
                &mut images,
                &mut has_leading_page_break,
                &mut has_explicit_page_break,
                &mut has_last_rendered_break,
                &mut active_field_instr,
                &mut inside_fld_result,
            );
        } else if el.name == "w:hyperlink" {
            for r in el.children_elements().filter(|c| c.name == "w:r") {
                extract_run_contents(
                    r,
                    styles,
                    style_id,
                    &def_font,
                    def_sz,
                    def_bold,
                    def_italic,
                    col_width_mm,
                    rels,
                    &mut spans,
                    &mut images,
                    &mut has_leading_page_break,
                    &mut has_explicit_page_break,
                    &mut has_last_rendered_break,
                    &mut active_field_instr,
                    &mut inside_fld_result,
                );
            }
        } else if el.name == "w:fldSimple" {
            let instr = el.get_attr("w:instr").unwrap_or("").to_ascii_uppercase();
            let field_kind = if instr.contains("NUMPAGES") {
                Some("NUMPAGES".to_string())
            } else if instr.contains("PAGE") {
                Some("PAGE".to_string())
            } else {
                None
            };
            if let Some(fk) = field_kind {
                let txt = el.extract_text();
                let show_txt = if txt.trim().is_empty() {
                    if fk == "PAGE" { "1".to_string() } else { "1".to_string() }
                } else {
                    txt
                };
                spans.push(PreviewSpan {
                    text: show_txt,
                    font_family: def_font.clone(),
                    font_size_pt: def_sz,
                    bold: def_bold,
                    italic: def_italic,
                    underline: false,
                    strike: false,
                    color_hex: "000000".to_string(),
                    highlight_hex: None,
                    vert_align: "baseline".to_string(),
                    is_dynamic_page_field: Some(fk),
                });
            }
        }
    }

    if has_leading_page_break {
        if !spans.is_empty() || !images.is_empty() {
            page_break_before = true;
        } else {
            has_explicit_page_break = true;
        }
    }

    let plain_text = if list_prefix.is_empty() {
        spans.iter().map(|s| s.text.as_str()).collect::<String>()
    } else {
        format!(
            "{}{}",
            list_prefix,
            spans.iter().map(|s| s.text.as_str()).collect::<String>()
        )
    };

    let mut block = PreviewParagraphBlock {
        element_path: element_path.to_string(),
        global_index,
        section_index,
        role: forced_role.unwrap_or("body").to_string(),
        list_prefix,
        alignment,
        indent_left_mm: twips_to_mm(left_t),
        indent_right_mm: twips_to_mm(right_t),
        first_line_indent_mm: twips_to_mm(first_line_t),
        hanging_indent_mm: twips_to_mm(hanging_t),
        space_before_pt: twips_to_pt(sb_t),
        space_after_pt: twips_to_pt(sa_t),
        line_spacing_mult,
        exact_line_pt,
        keep_next,
        page_break_before,
        has_explicit_page_break,
        has_last_rendered_break,
        plain_text,
        spans,
        images,
        estimated_height_mm: 0.0,
    };
    block.estimated_height_mm = estimate_paragraph_height_mm(&block, col_width_mm);
    block
}

#[allow(clippy::too_many_arguments)]
fn extract_run_contents(
    r: &XmlElement,
    styles: &StyleCatalog,
    p_style: Option<&str>,
    def_font: &str,
    def_sz: f64,
    def_bold: bool,
    def_italic: bool,
    col_width_mm: f64,
    rels: &HashMap<String, String>,
    spans: &mut Vec<PreviewSpan>,
    images: &mut Vec<PreviewImageRef>,
    has_leading_page_break: &mut bool,
    has_explicit_page_break: &mut bool,
    has_last_rendered_break: &mut bool,
    active_field_instr: &mut Option<String>,
    inside_fld_result: &mut bool,
) {
    let r_pr = r.child("w:rPr");
    let r_style = r_pr
        .and_then(|rp| rp.child("w:rStyle"))
        .and_then(|s| s.get_attr("w:val"));

    let font_family = r_pr
        .and_then(read_rpr_font)
        .or_else(|| styles.effective_font(p_style, r_style))
        .unwrap_or_else(|| def_font.to_string());
    let font_size_pt = r_pr
        .and_then(read_rpr_sz_pt)
        .or_else(|| styles.effective_size_pt(p_style, r_style))
        .unwrap_or(def_sz);
    let bold = r_pr.and_then(|rp| read_rpr_flag(rp, "w:b")).unwrap_or(def_bold);
    let italic = r_pr.and_then(|rp| read_rpr_flag(rp, "w:i")).unwrap_or(def_italic);
    let underline = r_pr
        .and_then(|rp| rp.child("w:u"))
        .and_then(|u| u.get_attr("w:val"))
        .map(|v| v != "none")
        .unwrap_or(false);
    let strike = r_pr
        .and_then(|rp| read_rpr_flag(rp, "w:strike"))
        .unwrap_or(false);
    let color_hex = r_pr
        .and_then(|rp| rp.child("w:color"))
        .and_then(|c| c.get_attr("w:val"))
        .filter(|v| *v != "auto" && v.len() == 6)
        .unwrap_or("000000")
        .to_string();
    let highlight_hex = r_pr
        .and_then(|rp| rp.child("w:highlight"))
        .and_then(|h| h.get_attr("w:val"))
        .map(|s| s.to_string());
    let vert_align = r_pr
        .and_then(|rp| rp.child("w:vertAlign"))
        .and_then(|v| v.get_attr("w:val"))
        .unwrap_or("baseline")
        .to_string();

    for c in &r.children {
        let XmlNode::Element(sub) = c else {
            continue;
        };
        match sub.name.as_str() {
            "w:fldChar" => {
                match sub.get_attr("w:fldCharType").unwrap_or("") {
                    "begin" => {
                        *active_field_instr = Some(String::new());
                        *inside_fld_result = false;
                    }
                    "separate" => {
                        *inside_fld_result = true;
                    }
                    "end" => {
                        *active_field_instr = None;
                        *inside_fld_result = false;
                    }
                    _ => {}
                }
            }
            "w:instrText" => {
                if let Some(ref mut instr) = active_field_instr {
                    instr.push_str(&sub.extract_text());
                }
            }
            "w:br" => {
                if sub.get_attr("w:type") == Some("page") {
                    if spans.is_empty() && images.is_empty() {
                        *has_leading_page_break = true;
                    } else {
                        *has_explicit_page_break = true;
                    }
                } else {
                    spans.push(PreviewSpan {
                        text: "\n".to_string(),
                        font_family: font_family.clone(),
                        font_size_pt,
                        bold,
                        italic,
                        underline,
                        strike,
                        color_hex: color_hex.clone(),
                        highlight_hex: highlight_hex.clone(),
                        vert_align: vert_align.clone(),
                        is_dynamic_page_field: None,
                    });
                }
            }
            "w:lastRenderedPageBreak" => {
                *has_last_rendered_break = true;
            }
            "w:tab" => {
                spans.push(PreviewSpan {
                    text: "    ".to_string(),
                    font_family: font_family.clone(),
                    font_size_pt,
                    bold,
                    italic,
                    underline,
                    strike,
                    color_hex: color_hex.clone(),
                    highlight_hex: highlight_hex.clone(),
                    vert_align: vert_align.clone(),
                    is_dynamic_page_field: None,
                });
            }
            "w:t" => {
                let dynamic_kind = active_field_instr.as_deref().and_then(|instr| {
                    if !*inside_fld_result {
                        return None;
                    }
                    let u = instr.to_ascii_uppercase();
                    if u.contains("NUMPAGES") {
                        Some("NUMPAGES".to_string())
                    } else if u.contains("PAGE") {
                        Some("PAGE".to_string())
                    } else {
                        None
                    }
                });
                let txt = sub.extract_text();
                if !txt.is_empty() || dynamic_kind.is_some() {
                    spans.push(PreviewSpan {
                        text: txt,
                        font_family: font_family.clone(),
                        font_size_pt,
                        bold,
                        italic,
                        underline,
                        strike,
                        color_hex: color_hex.clone(),
                        highlight_hex: highlight_hex.clone(),
                        vert_align: vert_align.clone(),
                        is_dynamic_page_field: dynamic_kind,
                    });
                }
            }
            "w:drawing" | "w:pict" | "mc:AlternateContent" => {
                collect_images_recursive(sub, rels, col_width_mm, images);
            }
            _ => {}
        }
    }
}

fn collect_images_recursive(
    node: &XmlElement,
    rels: &HashMap<String, String>,
    col_width_mm: f64,
    out: &mut Vec<PreviewImageRef>,
) {
    // WordprocessingML wp:inline or wp:anchor (skip TextBoxes with txbxContent!)
    if node.name == "wp:inline" || node.name == "wp:anchor" {
        if find_descendant(node, "w:txbxContent").is_some() {
            return;
        }
        let ext = find_descendant(node, "wp:extent");
        let cx = ext
            .and_then(|e| e.get_attr("cx"))
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1_800_000);
        let cy = ext
            .and_then(|e| e.get_attr("cy"))
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1_200_000);
        let width_mm = emu_to_mm(cx).max(4.0);
        let height_mm = emu_to_mm(cy).max(4.0);

        if let Some(blip) = find_descendant(node, "a:blip") {
            if let Some(rid) = blip.get_attr("r:embed").or_else(|| blip.get_attr("r:link")) {
                if let Some(target) = rels.get(rid) {
                    let alt = find_descendant(node, "wp:docPr")
                        .and_then(|d| d.get_attr("descr").or_else(|| d.get_attr("name")))
                        .unwrap_or("Hình ảnh")
                        .to_string();
                    out.push(PreviewImageRef {
                        rel_id: rid.to_string(),
                        media_entry: target.clone(),
                        mime_type: mime_from_entry(target),
                        width_mm,
                        height_mm,
                        wrap_mode: if node.name == "wp:anchor" {
                            "floating".to_string()
                        } else {
                            "inline".to_string()
                        },
                        alt_text: alt,
                        overflows_printable_area: width_mm > col_width_mm + 1.5,
                    });
                }
            }
        }
        return;
    }

    for c in node.children_elements() {
        collect_images_recursive(c, rels, col_width_mm, out);
    }
}

fn find_descendant<'a>(root: &'a XmlElement, target_name: &str) -> Option<&'a XmlElement> {
    if root.name == target_name {
        return Some(root);
    }
    for c in root.children_elements() {
        if let Some(found) = find_descendant(c, target_name) {
            return Some(found);
        }
    }
    None
}

fn mime_from_entry(entry: &str) -> String {
    let low = entry.to_ascii_lowercase();
    if low.ends_with(".png") {
        "image/png".to_string()
    } else if low.ends_with(".jpg") || low.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if low.ends_with(".gif") {
        "image/gif".to_string()
    } else if low.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else if low.ends_with(".webp") {
        "image/webp".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

fn parse_border_edge(el: Option<&XmlElement>, default_visible: bool) -> PreviewBorderEdge {
    let Some(b) = el else {
        return PreviewBorderEdge {
            visible: default_visible,
            style: if default_visible { "single" } else { "none" }.to_string(),
            width_pt: if default_visible { 0.5 } else { 0.0 },
            color_hex: "334155".to_string(),
        };
    };
    let style = b.get_attr("w:val").unwrap_or("single").to_string();
    let visible = !matches!(style.as_str(), "none" | "nil");
    let sz_eighths = b
        .get_attr("w:sz")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(if visible { 4.0 } else { 0.0 });
    let color = b
        .get_attr("w:color")
        .filter(|v| *v != "auto" && v.len() == 6)
        .unwrap_or("334155")
        .to_string();
    PreviewBorderEdge {
        visible,
        style,
        width_pt: round2((sz_eighths / 8.0).clamp(0.25, 4.0)),
        color_hex: color,
    }
}

fn parse_borders(el: Option<&XmlElement>, fallback: Option<&PreviewTableBorders>, default_vis: bool) -> PreviewTableBorders {
    let fb_top = fallback.map(|b| b.top.visible).unwrap_or(default_vis);
    let fb_bot = fallback.map(|b| b.bottom.visible).unwrap_or(default_vis);
    let fb_left = fallback.map(|b| b.left.visible).unwrap_or(default_vis);
    let fb_right = fallback.map(|b| b.right.visible).unwrap_or(default_vis);

    PreviewTableBorders {
        top: parse_border_edge(el.and_then(|e| e.child("w:top")), fb_top),
        bottom: parse_border_edge(el.and_then(|e| e.child("w:bottom")), fb_bot),
        left: parse_border_edge(el.and_then(|e| e.child("w:left")), fb_left),
        right: parse_border_edge(el.and_then(|e| e.child("w:right")), fb_right),
    }
}

#[allow(clippy::too_many_arguments)]
fn extract_table_block(
    tbl: &XmlElement,
    b_idx: usize,
    t_idx: usize,
    section_index: usize,
    printable_w_mm: f64,
    styles: &StyleCatalog,
    numbering: &mut SimpleNumbering,
    rels: &HashMap<String, String>,
    global_para_idx: &mut usize,
    image_count: &mut usize,
) -> PreviewTableBlock {
    let admin_kind = classify_admin_table(tbl);
    let table_role = match admin_kind {
        AdminTableKind::NationalHeader => "adminHeader",
        AdminTableKind::SignatureOrRecipient => "adminSignature",
        AdminTableKind::None => "dataTable",
    }
    .to_string();

    let tbl_pr = tbl.child("w:tblPr");
    let alignment = tbl_pr
        .and_then(|tp| tp.child("w:jc"))
        .and_then(|j| j.get_attr("w:val"))
        .unwrap_or("left")
        .to_string();
    let indent_left_mm = tbl_pr
        .and_then(|tp| tp.child("w:tblInd"))
        .and_then(|i| i.get_attr("w:w"))
        .and_then(|v| v.parse::<i32>().ok())
        .map(twips_to_mm)
        .unwrap_or(0.0);

    let tbl_borders_el = tbl_pr.and_then(|tp| tp.child("w:tblBorders"));
    let default_vis = table_role == "dataTable";
    let tbl_borders = parse_borders(tbl_borders_el, None, default_vis);
    let cell_default_borders = PreviewTableBorders {
        top: parse_border_edge(tbl_borders_el.and_then(|e| e.child("w:insideH")), tbl_borders.top.visible),
        bottom: parse_border_edge(tbl_borders_el.and_then(|e| e.child("w:insideH")), tbl_borders.bottom.visible),
        left: parse_border_edge(tbl_borders_el.and_then(|e| e.child("w:insideV")), tbl_borders.left.visible),
        right: parse_border_edge(tbl_borders_el.and_then(|e| e.child("w:insideV")), tbl_borders.right.visible),
    };

    let mut col_widths_mm: Vec<f64> = tbl
        .child("w:tblGrid")
        .map(|g| {
            g.children_elements()
                .filter(|c| c.name == "w:gridCol")
                .filter_map(|c| c.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()))
                .map(twips_to_mm)
                .collect()
        })
        .unwrap_or_default();

    let mut rows: Vec<PreviewTableRow> = Vec::new();
    for (r_idx, tr) in tbl.children_elements().filter(|c| c.name == "w:tr").enumerate() {
        let tr_pr = tr.child("w:trPr");
        let is_header_row = tr_pr.and_then(|tp| tp.child("w:tblHeader")).is_some();
        let cant_split = tr_pr.and_then(|tp| tp.child("w:cantSplit")).is_some();
        let min_height_mm = tr_pr
            .and_then(|tp| tp.child("w:trHeight"))
            .and_then(|h| h.get_attr("w:val"))
            .and_then(|v| v.parse::<i32>().ok())
            .map(twips_to_mm)
            .unwrap_or(0.0);

        let mut cells: Vec<PreviewTableCell> = Vec::new();
        let mut grid_col_cursor = 0usize;

        for (c_idx, tc) in tr.children_elements().filter(|c| c.name == "w:tc").enumerate() {
            let tc_pr = tc.child("w:tcPr");
            let col_span = tc_pr
                .and_then(|cp| cp.child("w:gridSpan"))
                .and_then(|g| g.get_attr("w:val"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let v_merge_attr = tc_pr
                .and_then(|cp| cp.child("w:vMerge"))
                .map(|vm| vm.get_attr("w:val").unwrap_or("continue"));
            let merged_continuation = matches!(v_merge_attr, Some(v) if v != "restart");

            let cell_w_mm = tc_pr
                .and_then(|cp| cp.child("w:tcW"))
                .and_then(|w| {
                    if w.get_attr("w:type").unwrap_or("dxa") == "dxa" {
                        w.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()).map(twips_to_mm)
                    } else {
                        None
                    }
                })
                .filter(|&w| w > 1.0)
                .unwrap_or_else(|| {
                    if !col_widths_mm.is_empty() {
                        (grid_col_cursor..(grid_col_cursor + col_span).min(col_widths_mm.len()))
                            .map(|k| col_widths_mm[k])
                            .sum::<f64>()
                            .max(12.0)
                    } else {
                        (printable_w_mm / 2.0).max(20.0)
                    }
                });

            let v_align = tc_pr
                .and_then(|cp| cp.child("w:vAlign"))
                .and_then(|v| v.get_attr("w:val"))
                .unwrap_or("top")
                .to_string();
            let shading_hex = tc_pr
                .and_then(|cp| cp.child("w:shd"))
                .and_then(|s| s.get_attr("w:fill"))
                .filter(|f| *f != "auto" && f.len() == 6)
                .map(|s| s.to_string());
            let borders = parse_borders(
                tc_pr.and_then(|cp| cp.child("w:tcBorders")),
                Some(&cell_default_borders),
                default_vis,
            );

            let mut paras = Vec::new();
            for (cp_idx, p) in tc.children_elements().filter(|c| c.name == "w:p").enumerate() {
                let g = *global_para_idx;
                *global_para_idx += 1;
                let mut p_block = extract_paragraph_block(
                    p,
                    &format!("body/tbl[{}]/tr[{}]/tc[{}]/p[{}]", b_idx, r_idx, c_idx, cp_idx),
                    g,
                    section_index,
                    (cell_w_mm - 3.5).max(12.0),
                    styles,
                    numbering,
                    rels,
                    Some(match table_role.as_str() {
                        "adminHeader" => "adminHeader",
                        "adminSignature" => if c_idx == 0 { "recipient" } else { "signature" },
                        _ => "tableCell",
                    }),
                );
                if table_role != "dataTable" && p_block.space_after_pt == 6.0 {
                    p_block.space_after_pt = 2.0;
                    p_block.estimated_height_mm =
                        estimate_paragraph_height_mm(&p_block, (cell_w_mm - 3.5).max(12.0));
                }
                *image_count += p_block.images.len();
                paras.push(p_block);
            }

            cells.push(PreviewTableCell {
                col_index: grid_col_cursor,
                col_span,
                row_span: 1,
                merged_continuation,
                width_mm: round1(cell_w_mm),
                v_align,
                shading_hex,
                borders,
                paragraphs: paras,
            });
            grid_col_cursor += col_span;
        }

        if col_widths_mm.is_empty() && !cells.is_empty() {
            col_widths_mm = cells.iter().map(|c| c.width_mm).collect();
        }

        rows.push(PreviewTableRow {
            row_index: r_idx,
            is_header_row,
            cant_split,
            min_height_mm,
            cells,
        });
    }

    // Resolve vertical row_span counts across rows
    for r_i in 0..rows.len() {
        for c_pos in 0..rows[r_i].cells.len() {
            if rows[r_i].cells[c_pos].merged_continuation {
                continue;
            }
            let target_col = rows[r_i].cells[c_pos].col_index;
            let mut span = 1usize;
            for next_r in (r_i + 1)..rows.len() {
                if let Some(below) = rows[next_r]
                    .cells
                    .iter()
                    .find(|c| c.col_index == target_col)
                {
                    if below.merged_continuation {
                        span += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            rows[r_i].cells[c_pos].row_span = span;
        }
    }

    let tbl_w_attr_mm = tbl_pr
        .and_then(|tp| tp.child("w:tblW"))
        .and_then(|w| {
            let t = w.get_attr("w:type").unwrap_or("dxa");
            if t == "dxa" {
                w.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()).map(twips_to_mm)
            } else if t == "pct" {
                w.get_attr("w:w")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|pct| round1(printable_w_mm * pct / 5000.0))
            } else {
                None
            }
        })
        .filter(|&w| w > 1.0);

    let grid_sum_mm: f64 = col_widths_mm.iter().sum();
    let total_width_mm = round1(if grid_sum_mm > 1.0 {
        tbl_w_attr_mm.map(|w| w.max(grid_sum_mm)).unwrap_or(grid_sum_mm)
    } else {
        tbl_w_attr_mm.unwrap_or(printable_w_mm)
    });

    let row_count = rows.len();
    PreviewTableBlock {
        element_path: format!("body/tbl[{}]", b_idx),
        table_index: t_idx,
        section_index,
        table_role,
        alignment,
        indent_left_mm,
        total_width_mm,
        col_widths_mm,
        borders: tbl_borders,
        overflows_printable_width: (total_width_mm + indent_left_mm) > (printable_w_mm + 1.5),
        rows,
        row_slice_range: (0, row_count),
    }
}

pub fn estimate_paragraph_height_mm(p: &PreviewParagraphBlock, col_width_mm: f64) -> f64 {
    let pt_to_mm = 0.352778;
    let max_sz = p
        .spans
        .iter()
        .map(|s| s.font_size_pt)
        .fold(13.5f64, f64::max);

    let line_h_mm = if let Some(exact_pt) = p.exact_line_pt {
        exact_pt * pt_to_mm
    } else {
        max_sz * pt_to_mm * 1.2 * p.line_spacing_mult.max(0.85)
    };

    let text_w_mm = (col_width_mm - p.indent_left_mm - p.indent_right_mm).max(20.0);
    // Average Vietnamese Times New Roman character advance ~ 0.48 * font_size_pt
    let avg_char_mm = (max_sz * pt_to_mm * 0.48).max(1.2);
    let chars_per_line = (text_w_mm / avg_char_mm).floor().max(12.0) as usize;

    let mut visual_lines = 0usize;
    let first_line_char_delta = (p.first_line_indent_mm / avg_char_mm).max(0.0) as usize;
    for raw_line in p.plain_text.split('\n') {
        let char_count = raw_line.chars().count();
        if char_count == 0 {
            visual_lines += 1;
        } else {
            let eff = char_count + first_line_char_delta;
            visual_lines += (eff + chars_per_line - 1) / chars_per_line;
        }
    }
    if visual_lines == 0 && p.images.is_empty() {
        visual_lines = 1;
    }

    let imgs_h_mm: f64 = p.images.iter().map(|i| i.height_mm + 2.0).sum();
    let spacing_mm = (p.space_before_pt + p.space_after_pt) * pt_to_mm;
    let content_h = (visual_lines as f64) * line_h_mm + imgs_h_mm + spacing_mm;
    round2(content_h.max(4.0))
}

fn infer_visual_role(
    p: &XmlElement,
    trimmed: &str,
    upper: &str,
    jc: &str,
    spans: &[PreviewSpan],
    global_idx: usize,
    near_admin_header: bool,
    past_doc_type: bool,
) -> String {
    let t = trimmed.trim();
    if t.is_empty() {
        return "empty".to_string();
    }
    let style_id = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"))
        .unwrap_or("")
        .to_ascii_lowercase();
    if style_id == "heading1" || style_id == "heading 1" {
        return "heading1".to_string();
    }
    if style_id == "heading2" || style_id == "heading 2" {
        return "heading2".to_string();
    }
    if style_id.starts_with("heading") {
        return "heading3".to_string();
    }
    let is_bold = spans.iter().any(|s| s.bold);
    if (global_idx <= 12 || near_admin_header)
        && (upper.contains("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM")
            || upper.contains("ĐỘC LẬP - TỰ DO - HẠNH PHÚC")
            || upper.starts_with("SỞ:")
            || upper.starts_with("SỐ:"))
    {
        return "adminHeader".to_string();
    }
    if !past_doc_type
        && global_idx <= 16
        && (jc == "center" || is_bold)
        && matches!(
            upper,
            "QUYẾT ĐỊNH" | "TỜ TRÌNH" | "BÁO CÁO" | "THÔNG BÁO" | "KẾ HOẠCH" | "BIÊN BẢN" | "NGHỊ QUYẾT" | "CHỈ THỊ" | "CÔNG VĂN" | "HƯỚNG DẪN" | "QUY CHẾ"
        )
    {
        return "docTypeHeading".to_string();
    }
    if (near_admin_header || (past_doc_type && global_idx <= 18))
        && (upper.starts_with("V/V") || upper.starts_with("VỀ VIỆC") || (jc == "center" && is_bold && t.chars().count() <= 180))
    {
        return "docSubject".to_string();
    }
    if upper.starts_with("NƠI NHẬN") {
        return "recipient".to_string();
    }
    if upper.starts_with("TM.") || upper.starts_with("KT.") || upper.starts_with("CHỦ TỊCH") || upper.starts_with("GIÁM ĐỐC") || upper.starts_with("BỘ TRƯỞNG") {
        return "signature".to_string();
    }
    "body".to_string()
}
