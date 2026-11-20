use crate::engine::formatter::{
    classify_admin_table, collapse_spaces_upper, is_national_header_cue,
    is_protected_admin_paragraph_ctx, is_signature_trigger_paragraph, AdminTableKind,
};
use crate::engine::openxml_arch::SelectivePackageView;
use crate::engine::vn_cleaner::{contains_soft_break, extract_run_font, is_removable_empty_paragraph};
use crate::engine::xml_helpers::{XmlDocument, XmlElement, XmlNode};
use crate::models::{
    AssetInventory, HeaderFooterSummary, OrgSector, OrganizationProfile,
    SectionSummary, StdDocScope, TableSummary,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

const NON_REPLACEABLE_SYMBOL_FONTS: &[&str] = &[
    "symbol",
    "wingdings",
    "wingdings 2",
    "wingdings 3",
    "webdings",
    "marlett",
    "cambria math",
    "segoe ui symbol",
    "mt extra",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphRole {
    AdminHeaderCell,
    DocTypeHeading,
    DocSubject,
    LegalBasisCanCu,
    RecipientKinhGui,
    StructuralHeading(u8),
    AppendixHeading,
    BodyProse,
    RecipientNoiNhan,
    SignatureBlock,
    DataTableCell,
    EmptyParagraph,
}

impl ParagraphRole {
    pub fn label_vi(self) -> &'static str {
        match self {
            Self::AdminHeaderCell => "Khối đầu trang",
            Self::DocTypeHeading => "Tên loại VB",
            Self::DocSubject => "Trích yếu",
            Self::LegalBasisCanCu => "Căn cứ pháp lý",
            Self::RecipientKinhGui => "Kính gửi",
            Self::StructuralHeading(1) => "Đề mục cấp 1",
            Self::StructuralHeading(2) => "Đề mục cấp 2",
            Self::StructuralHeading(3) => "Đề mục cấp 3",
            Self::StructuralHeading(_) => "Đề mục cấp 4",
            Self::AppendixHeading => "Phụ lục",
            Self::BodyProse => "Lời văn chính",
            Self::RecipientNoiNhan => "Nơi nhận",
            Self::SignatureBlock => "Chữ ký",
            Self::DataTableCell => "Ô bảng dữ liệu",
            Self::EmptyParagraph => "Đoạn trống",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedStyleInfo {
    pub font_name: Option<String>,
    pub size_pt: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<String>,
    pub jc: Option<String>,
    pub indent_first_line: Option<i32>,
    pub spacing_before: Option<i32>,
    pub spacing_after: Option<i32>,
    pub line_spacing: Option<i32>,
    pub outline_lvl: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct StyleCatalog {
    pub doc_default_font: Option<String>,
    pub doc_default_size_pt: Option<f64>,
    pub default_para_style_id: Option<String>,
    pub by_id: HashMap<String, ResolvedStyleInfo>,
}

impl StyleCatalog {
    pub fn from_xml(xml: Option<&str>) -> Self {
        let Some(raw) = xml else {
            return Self::default();
        };
        let Ok(doc) = XmlDocument::parse(raw) else {
            return Self::default();
        };

        let mut catalog = Self::default();

        if let Some(r_pr) = doc
            .root
            .child("w:docDefaults")
            .and_then(|d| d.child("w:rPrDefault"))
            .and_then(|d| d.child("w:rPr"))
        {
            catalog.doc_default_font = read_rpr_font(r_pr);
            catalog.doc_default_size_pt = read_rpr_sz_pt(r_pr);
        }

        #[derive(Clone, Default)]
        struct RawSt {
            based_on: Option<String>,
            info: ResolvedStyleInfo,
        }

        let mut raw_map: HashMap<String, RawSt> = HashMap::new();
        for el in doc.root.children_elements().filter(|e| e.name == "w:style") {
            let Some(id) = el.get_attr("w:styleId") else {
                continue;
            };
            if el.get_attr("w:type") == Some("paragraph") && el.get_attr("w:default") == Some("1") {
                catalog.default_para_style_id = Some(id.to_string());
            }
            let based_on = el
                .child("w:basedOn")
                .and_then(|b| b.get_attr("w:val"))
                .map(|s| s.to_string());
            let r_pr = el.child("w:rPr");
            let p_pr = el.child("w:pPr");

            let info = ResolvedStyleInfo {
                font_name: r_pr.and_then(read_rpr_font),
                size_pt: r_pr.and_then(read_rpr_sz_pt),
                bold: r_pr.and_then(|r| read_rpr_flag(r, "w:b")),
                italic: r_pr.and_then(|r| read_rpr_flag(r, "w:i")),
                color: r_pr
                    .and_then(|r| r.child("w:color"))
                    .and_then(|c| c.get_attr("w:val"))
                    .map(|s| s.to_string()),
                jc: p_pr
                    .and_then(|p| p.child("w:jc"))
                    .and_then(|j| j.get_attr("w:val"))
                    .map(|s| s.to_string()),
                indent_first_line: p_pr
                    .and_then(|p| p.child("w:ind"))
                    .and_then(|i| i.get_attr("w:firstLine"))
                    .and_then(|v| v.parse().ok()),
                spacing_before: p_pr
                    .and_then(|p| p.child("w:spacing"))
                    .and_then(|s| s.get_attr("w:before"))
                    .and_then(|v| v.parse().ok()),
                spacing_after: p_pr
                    .and_then(|p| p.child("w:spacing"))
                    .and_then(|s| s.get_attr("w:after"))
                    .and_then(|v| v.parse().ok()),
                line_spacing: p_pr
                    .and_then(|p| p.child("w:spacing"))
                    .and_then(|s| s.get_attr("w:line"))
                    .and_then(|v| v.parse().ok()),
                outline_lvl: p_pr
                    .and_then(|p| p.child("w:outlineLvl"))
                    .and_then(|o| o.get_attr("w:val"))
                    .and_then(|v| v.parse().ok()),
            };

            raw_map.insert(id.to_string(), RawSt { based_on, info });
        }

        for id in raw_map.keys() {
            let mut merged = ResolvedStyleInfo::default();
            let mut cur = Some(id.as_str());
            for _ in 0..16 {
                let Some(cid) = cur else {
                    break;
                };
                let Some(st) = raw_map.get(cid) else {
                    break;
                };
                if merged.font_name.is_none() {
                    merged.font_name = st.info.font_name.clone();
                }
                if merged.size_pt.is_none() {
                    merged.size_pt = st.info.size_pt;
                }
                if merged.bold.is_none() {
                    merged.bold = st.info.bold;
                }
                if merged.italic.is_none() {
                    merged.italic = st.info.italic;
                }
                if merged.color.is_none() {
                    merged.color = st.info.color.clone();
                }
                if merged.jc.is_none() {
                    merged.jc = st.info.jc.clone();
                }
                if merged.indent_first_line.is_none() {
                    merged.indent_first_line = st.info.indent_first_line;
                }
                if merged.spacing_before.is_none() {
                    merged.spacing_before = st.info.spacing_before;
                }
                if merged.spacing_after.is_none() {
                    merged.spacing_after = st.info.spacing_after;
                }
                if merged.line_spacing.is_none() {
                    merged.line_spacing = st.info.line_spacing;
                }
                if merged.outline_lvl.is_none() {
                    merged.outline_lvl = st.info.outline_lvl;
                }
                cur = st.based_on.as_deref();
            }
            catalog.by_id.insert(id.clone(), merged);
        }

        catalog
    }

    pub fn effective_font(&self, p_style: Option<&str>, r_style: Option<&str>) -> Option<String> {
        if let Some(rs) = r_style {
            if let Some(f) = self.by_id.get(rs).and_then(|s| s.font_name.as_ref()) {
                return Some(f.clone());
            }
        }
        if let Some(ps) = p_style {
            if let Some(f) = self.by_id.get(ps).and_then(|s| s.font_name.as_ref()) {
                return Some(f.clone());
            }
        } else if let Some(ref def_ps) = self.default_para_style_id {
            if let Some(f) = self.by_id.get(def_ps).and_then(|s| s.font_name.as_ref()) {
                return Some(f.clone());
            }
        }
        self.doc_default_font.clone()
    }

    pub fn effective_size_pt(&self, p_style: Option<&str>, r_style: Option<&str>) -> Option<f64> {
        if let Some(rs) = r_style {
            if let Some(sz) = self.by_id.get(rs).and_then(|s| s.size_pt) {
                return Some(sz);
            }
        }
        if let Some(ps) = p_style {
            if let Some(sz) = self.by_id.get(ps).and_then(|s| s.size_pt) {
                return Some(sz);
            }
        } else if let Some(ref def_ps) = self.default_para_style_id {
            if let Some(sz) = self.by_id.get(def_ps).and_then(|s| s.size_pt) {
                return Some(sz);
            }
        }
        self.doc_default_size_pt
    }

    pub fn effective_style(&self, p_style: Option<&str>) -> Option<&ResolvedStyleInfo> {
        p_style
            .and_then(|id| self.by_id.get(id))
            .or_else(|| self.default_para_style_id.as_ref().and_then(|id| self.by_id.get(id)))
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SectionInfo {
    pub summary: SectionSummary,
    pub pg_w_twips: i32,
    pub pg_h_twips: i32,
    pub top_twips: i32,
    pub bottom_twips: i32,
    pub left_twips: i32,
    pub right_twips: i32,
    pub printable_width_twips: i32,
    pub column_count: usize,
    pub column_space_twips: i32,
    pub equal_column_width: bool,
    pub col_widths_twips: Vec<i32>,
    pub is_body_trailing_sect: bool,
    pub header_rids: Vec<String>,
    pub footer_rids: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ParagraphInfo {
    pub body_child_idx: usize,
    pub in_table_idx: Option<usize>,
    pub global_index: usize,
    pub element_path: String,
    pub section_index: usize,
    pub role: ParagraphRole,
    pub text: String,
    pub trimmed: String,
    pub style_id: Option<String>,
    pub direct_jc: Option<String>,
    pub effective_jc: String,
    pub indent_first_line: i32,
    pub indent_left: i32,
    pub indent_right: i32,
    pub spacing_before: i32,
    pub spacing_after: i32,
    pub line_spacing: i32,
    pub line_rule: String,
    pub has_direct_formatting_conflict: bool,
    pub has_soft_break: bool,
    pub is_empty_removable: bool,
    pub is_bold_majority: bool,
    pub is_italic_majority: bool,
    pub primary_font: String,
    pub font_names: Vec<String>,
    pub primary_size_pt: Option<f64>,
    pub sizes_pt: Vec<f64>,
    pub non_black_colors: Vec<String>,
    pub has_legacy_encoding: bool,
    pub has_mixed_fonts: bool,
    pub has_missing_font_decl: bool,
    pub has_multi_space: bool,
    pub has_punct_space_error: bool,
    pub has_control_chars: bool,
    pub has_drawing: bool,
    pub has_field: bool,
    pub has_hyperlink: bool,
    pub has_bookmark: bool,
    pub has_footnote_ref: bool,
    pub has_endnote_ref: bool,
    pub has_comment_ref: bool,
    pub outline_lvl: Option<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub table_index: usize,
    pub body_child_idx: usize,
    pub summary: TableSummary,
    pub admin_kind: AdminTableKind,
    pub section_index: usize,
    pub exceeds_by_twips: i32,
    pub row_fonts: Vec<String>,
    pub row_sizes_pt: Vec<f64>,
    pub row_has_bad_indent: bool,
    pub locked_long_row_indices: Vec<usize>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DocumentIr {
    pub file_path: String,
    pub file_name: String,
    pub file_ext: String,
    pub requested_scope: StdDocScope,
    pub effective_scope: StdDocScope,
    pub requires_scope_selection: bool,
    pub scope_reason: String,
    pub styles: StyleCatalog,
    pub even_and_odd_headers_enabled: bool,
    pub sections: Vec<SectionInfo>,
    pub paragraphs: Vec<ParagraphInfo>,
    pub tables: Vec<TableInfo>,
    pub headers_footers: Vec<HeaderFooterSummary>,
    pub rels_map: HashMap<String, (String, String)>,
    pub broken_rids: Vec<String>,
    pub assets: AssetInventory,
}

impl DocumentIr {
    pub fn build(
        path: &Path,
        view: &SelectivePackageView,
        requested_scope: StdDocScope,
        org_profile: Option<&OrganizationProfile>,
    ) -> Result<Self, String> {
        let doc_xml = view
            .get_xml("word/document.xml")
            .ok_or_else(|| "Không tìm thấy word/document.xml.".to_string())?;
        let doc = XmlDocument::parse(doc_xml)?;
        let body = doc
            .root
            .child("w:body")
            .ok_or_else(|| "Tài liệu không chứa phần tử <w:body>.".to_string())?;

        let styles = StyleCatalog::from_xml(view.get_xml("word/styles.xml"));
        let even_and_odd_headers_enabled = view
            .get_xml("word/settings.xml")
            .and_then(|s| XmlDocument::parse(s).ok())
            .map(|sd| sd.root.child("w:evenAndHeaders").is_some())
            .unwrap_or(false);

        let (rels_map, target_to_rid) = parse_rels(view.get_xml("word/_rels/document.xml.rels"));

        // Collect all section boundaries (inline w:p/w:pPr/w:sectPr + final w:body/w:sectPr)
        let mut sections: Vec<SectionInfo> = Vec::new();
        let mut inline_sect_para_indices: HashMap<usize, usize> = HashMap::new();
        for (idx, node) in body.children.iter().enumerate() {
            if let XmlNode::Element(el) = node {
                if el.name == "w:p" {
                    if let Some(sect) = el.child("w:pPr").and_then(|pp| pp.child("w:sectPr")) {
                        let s_idx = sections.len() + 1;
                        inline_sect_para_indices.insert(idx, s_idx);
                        sections.push(parse_section(sect, s_idx, false, even_and_odd_headers_enabled));
                    }
                }
            }
        }
        if let Some(sect) = body.child("w:sectPr") {
            let s_idx = sections.len() + 1;
            sections.push(parse_section(sect, s_idx, true, even_and_odd_headers_enabled));
        }
        if sections.is_empty() {
            sections.push(SectionInfo {
                summary: SectionSummary {
                    section_index: 1,
                    paper_width_mm: 210.0,
                    paper_height_mm: 297.0,
                    orientation: "portrait".to_string(),
                    margin_top_mm: 20.0,
                    margin_bottom_mm: 20.0,
                    margin_left_mm: 30.0,
                    margin_right_mm: 15.0,
                    different_first_page: false,
                    different_odd_even: even_and_odd_headers_enabled,
                    break_type: "nextPage".to_string(),
                    has_header_ref: false,
                    has_footer_ref: false,
                },
                pg_w_twips: 11906,
                pg_h_twips: 16838,
                top_twips: 1134,
                bottom_twips: 1134,
                left_twips: 1701,
                right_twips: 850,
                printable_width_twips: 9355,
                column_count: 1,
                column_space_twips: 720,
                equal_column_width: true,
                col_widths_twips: vec![9355],
                is_body_trailing_sect: true,
                header_rids: Vec::new(),
                footer_rids: Vec::new(),
            });
        }

        // Analyze Header/Footer parts
        let headers_footers = inspect_headers_footers(view);

        // Analyze broken rel references across word/document.xml
        let mut referenced_rids: Vec<String> = Vec::new();
        collect_all_rids(&doc.root, &mut referenced_rids);
        let mut broken_rids: Vec<String> = Vec::new();
        let mut seen_broken = HashSet::new();
        for rid in referenced_rids {
            if !rels_map.contains_key(&rid) && seen_broken.insert(rid.clone()) {
                broken_rids.push(rid);
            }
        }

        // Walk body top-level elements (sections advance AFTER encountering a paragraph containing inline sectPr)
        let mut paragraphs: Vec<ParagraphInfo> = Vec::new();
        let mut tables: Vec<TableInfo> = Vec::new();
        let mut current_sect_idx: usize = 1;
        let mut near_admin_header = false;
        let mut in_sig_or_recipient = false;
        let mut past_doc_type_heading = false;

        for (b_idx, node) in body.children.iter().enumerate() {
            let XmlNode::Element(el) = node else {
                continue;
            };
            match el.name.as_str() {
                "w:p" => {
                    let g_idx = paragraphs.len();
                    let p_info = analyze_paragraph(
                        el,
                        b_idx,
                        None,
                        g_idx,
                        format!("body/p[{}]", b_idx),
                        current_sect_idx,
                        &styles,
                        near_admin_header,
                        in_sig_or_recipient,
                        past_doc_type_heading,
                        None,
                    );
                    if matches!(p_info.role, ParagraphRole::AdminHeaderCell) {
                        near_admin_header = true;
                    } else if matches!(p_info.role, ParagraphRole::DocTypeHeading) {
                        near_admin_header = false;
                        past_doc_type_heading = true;
                    } else if !p_info.trimmed.is_empty()
                        && !matches!(p_info.role, ParagraphRole::DocSubject)
                    {
                        near_admin_header = false;
                    }
                    if matches!(
                        p_info.role,
                        ParagraphRole::RecipientNoiNhan | ParagraphRole::SignatureBlock
                    ) {
                        in_sig_or_recipient = true;
                    } else if matches!(p_info.role, ParagraphRole::StructuralHeading(_) | ParagraphRole::AppendixHeading)
                    {
                        in_sig_or_recipient = false;
                    }

                    paragraphs.push(p_info);

                    if inline_sect_para_indices.contains_key(&b_idx) {
                        current_sect_idx = (current_sect_idx + 1).min(sections.len());
                    }
                }
                "w:tbl" => {
                    let t_idx = tables.len();
                    let sect_printable = sections
                        .get(current_sect_idx.saturating_sub(1))
                        .map(|s| s.printable_width_twips)
                        .unwrap_or(9355);
                    let admin_kind = classify_admin_table(el);
                    if admin_kind == AdminTableKind::NationalHeader {
                        near_admin_header = true;
                    }

                    let mut t_fonts_set = HashSet::new();
                    let mut t_sizes_set: Vec<f64> = Vec::new();
                    let mut row_has_bad_indent = false;
                    let mut locked_long_row_indices = Vec::new();
                    let mut first_row_has_header = false;
                    let mut row_count = 0usize;
                    let mut max_cols = 0usize;

                    for (r_idx, tr) in el.children_elements().filter(|c| c.name == "w:tr").enumerate() {
                        row_count += 1;
                        if r_idx == 0
                            && tr
                                .child("w:trPr")
                                .and_then(|tp| tp.child("w:tblHeader"))
                                .is_some()
                        {
                            first_row_has_header = true;
                        }
                        let mut row_len = 0usize;
                        let mut max_c_len = 0usize;
                        let mut max_c_paras = 0usize;
                        let mut col_cnt = 0usize;

                        for (c_idx, tc) in tr.children_elements().filter(|c| c.name == "w:tc").enumerate() {
                            col_cnt += 1;
                            let cell_txt_len = tc.extract_text().chars().count();
                            row_len += cell_txt_len;
                            max_c_len = max_c_len.max(cell_txt_len);
                            let mut c_paras = 0usize;

                            for (cp_idx, p) in tc.children_elements().filter(|c| c.name == "w:p").enumerate() {
                                c_paras += 1;
                                let cell_role = match admin_kind {
                                    AdminTableKind::NationalHeader => Some(ParagraphRole::AdminHeaderCell),
                                    AdminTableKind::SignatureOrRecipient => {
                                        if c_idx == 0 {
                                            Some(ParagraphRole::RecipientNoiNhan)
                                        } else {
                                            Some(ParagraphRole::SignatureBlock)
                                        }
                                    }
                                    AdminTableKind::None => Some(ParagraphRole::DataTableCell),
                                };
                                let g_idx = paragraphs.len();
                                let pi = analyze_paragraph(
                                    p,
                                    b_idx,
                                    Some(t_idx),
                                    g_idx,
                                    format!("body/tbl[{}]/tr[{}]/tc[{}]/p[{}]", b_idx, r_idx, c_idx, cp_idx),
                                    current_sect_idx,
                                    &styles,
                                    false,
                                    false,
                                    false,
                                    cell_role,
                                );
                                for f in &pi.font_names {
                                    t_fonts_set.insert(f.clone());
                                }
                                for &sz in &pi.sizes_pt {
                                    if !t_sizes_set.iter().any(|&v| (v - sz).abs() < 0.1) {
                                        t_sizes_set.push(sz);
                                    }
                                }
                                if admin_kind == AdminTableKind::None
                                    && (pi.indent_first_line.abs() > 80 || pi.indent_left > 240)
                                {
                                    row_has_bad_indent = true;
                                }
                                paragraphs.push(pi);
                            }
                            max_c_paras = max_c_paras.max(c_paras);
                        }
                        max_cols = max_cols.max(col_cnt);
                        let is_long_row = max_c_paras > 1 || max_c_len > 65 || row_len > 130;
                        let has_cant_split = tr
                            .child("w:trPr")
                            .and_then(|tp| tp.child("w:cantSplit"))
                            .is_some();
                        if admin_kind == AdminTableKind::None && is_long_row && has_cant_split {
                            locked_long_row_indices.push(r_idx);
                        }
                    }

                    let total_w = estimate_table_width_twips(el, sect_printable);
                    let exceeds = total_w - sect_printable;
                    let summary = TableSummary {
                        table_index: t_idx,
                        element_path: format!("body/tbl[{}]", b_idx),
                        kind: match admin_kind {
                            AdminTableKind::NationalHeader => "Khung Quốc hiệu / Cơ quan".to_string(),
                            AdminTableKind::SignatureOrRecipient => "Khung Nơi nhận / Chữ ký".to_string(),
                            AdminTableKind::None => "Bảng dữ liệu".to_string(),
                        },
                        row_count,
                        col_count: max_cols,
                        total_width_twips: total_w,
                        printable_width_twips: sect_printable,
                        exceeds_printable_width: exceeds > 120,
                        has_repeat_header: first_row_has_header,
                        has_locked_long_rows: !locked_long_row_indices.is_empty(),
                    };

                    tables.push(TableInfo {
                        table_index: t_idx,
                        body_child_idx: b_idx,
                        summary,
                        admin_kind,
                        section_index: current_sect_idx,
                        exceeds_by_twips: exceeds,
                        row_fonts: t_fonts_set.into_iter().collect(),
                        row_sizes_pt: t_sizes_set,
                        row_has_bad_indent,
                        locked_long_row_indices,
                    });
                }
                _ => {}
            }
        }

        // Inventory assets across selective package parts
        let assets = build_asset_inventory(view, &doc.root, &paragraphs, &target_to_rid);

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document.docx")
            .to_string();
        let file_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("docx")
            .to_ascii_lowercase();

        let (effective_scope, scope_reason, requires_scope_selection) =
            resolve_scope(requested_scope, &paragraphs, &tables, org_profile);

        Ok(Self {
            file_path: path.to_string_lossy().to_string(),
            file_name,
            file_ext,
            requested_scope,
            effective_scope,
            requires_scope_selection,
            scope_reason,
            styles,
            even_and_odd_headers_enabled,
            sections,
            paragraphs,
            tables,
            headers_footers,
            rels_map,
            broken_rids,
            assets,
        })
    }
}

fn parse_rels(xml: Option<&str>) -> (HashMap<String, (String, String)>, HashMap<String, String>) {
    let mut map = HashMap::new();
    let mut rev = HashMap::new();
    let Some(raw) = xml else {
        return (map, rev);
    };
    let Ok(doc) = XmlDocument::parse(raw) else {
        return (map, rev);
    };
    for rel in doc.root.children_elements().filter(|e| e.name == "Relationship") {
        let Some(id) = rel.get_attr("Id") else {
            continue;
        };
        let rtype = rel.get_attr("Type").unwrap_or("").to_string();
        let target = rel.get_attr("Target").unwrap_or("").to_string();
        rev.insert(target.clone(), id.to_string());
        map.insert(id.to_string(), (rtype, target));
    }
    (map, rev)
}

fn parse_section(
    sect: &XmlElement,
    section_index: usize,
    is_body_trailing_sect: bool,
    even_odd: bool,
) -> SectionInfo {
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
        .unwrap_or_else(|| if pg_w > pg_h { "landscape" } else { "portrait" })
        .to_string();

    let mar = sect.child("w:pgMar");
    let top: i32 = mar
        .and_then(|m| m.get_attr("w:top"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1134);
    let bottom: i32 = mar
        .and_then(|m| m.get_attr("w:bottom"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1134);
    let left: i32 = mar
        .and_then(|m| m.get_attr("w:left"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1701);
    let right: i32 = mar
        .and_then(|m| m.get_attr("w:right"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(850);

    let break_type = sect
        .child("w:type")
        .and_then(|t| t.get_attr("w:val"))
        .unwrap_or("nextPage")
        .to_string();
    let different_first_page = sect.child("w:titlePg").is_some();

    let mut header_rids = Vec::new();
    let mut footer_rids = Vec::new();
    for c in sect.children_elements() {
        if c.name == "w:headerReference" {
            if let Some(rid) = c.get_attr("r:id") {
                header_rids.push(rid.to_string());
            }
        } else if c.name == "w:footerReference" {
            if let Some(rid) = c.get_attr("r:id") {
                footer_rids.push(rid.to_string());
            }
        }
    }

    let printable = (pg_w - left - right).max(2000);
    let cols_el = sect.child("w:cols");
    let column_count: usize = cols_el
        .and_then(|c| c.get_attr("w:num"))
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.max(1))
        .unwrap_or(1);
    let column_space_twips: i32 = cols_el
        .and_then(|c| c.get_attr("w:space"))
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(720);
    let equal_column_width = cols_el
        .and_then(|c| c.get_attr("w:equalWidth"))
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true);
    let mut col_widths_twips: Vec<i32> = cols_el
        .map(|c| {
            c.children_elements()
                .filter(|sub| sub.name == "w:col")
                .filter_map(|sub| sub.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()))
                .collect()
        })
        .unwrap_or_default();
    if col_widths_twips.is_empty() {
        if column_count > 1 {
            let gaps = (column_count as i32 - 1) * column_space_twips;
            let each = ((printable - gaps) / column_count as i32).max(720);
            col_widths_twips = vec![each; column_count];
        } else {
            col_widths_twips = vec![printable];
        }
    }

    let summary = SectionSummary {
        section_index,
        paper_width_mm: round1(pg_w as f64 / 56.6929),
        paper_height_mm: round1(pg_h as f64 / 56.6929),
        orientation: orient,
        margin_top_mm: round1(top as f64 / 56.6929),
        margin_bottom_mm: round1(bottom as f64 / 56.6929),
        margin_left_mm: round1(left as f64 / 56.6929),
        margin_right_mm: round1(right as f64 / 56.6929),
        different_first_page,
        different_odd_even: even_odd,
        break_type,
        has_header_ref: !header_rids.is_empty(),
        has_footer_ref: !footer_rids.is_empty(),
    };

    SectionInfo {
        summary,
        pg_w_twips: pg_w,
        pg_h_twips: pg_h,
        top_twips: top,
        bottom_twips: bottom,
        left_twips: left,
        right_twips: right,
        printable_width_twips: printable,
        column_count,
        column_space_twips,
        equal_column_width,
        col_widths_twips,
        is_body_trailing_sect,
        header_rids,
        footer_rids,
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn inspect_headers_footers(view: &SelectivePackageView) -> Vec<HeaderFooterSummary> {
    let mut parts: Vec<(&String, &String)> = view
        .xml_parts
        .iter()
        .filter(|(k, _)| {
            let kl = k.to_ascii_lowercase();
            kl.starts_with("word/header") || kl.starts_with("word/footer")
        })
        .collect();
    parts.sort_by(|a, b| a.0.cmp(b.0));

    let mut out = Vec::new();
    for (name, xml) in parts {
        let lower = name.to_ascii_lowercase();
        let kind = if lower.contains("header") {
            "header"
        } else {
            "footer"
        };
        let Ok(doc) = XmlDocument::parse(xml) else {
            continue;
        };
        let full_xml_upper = xml.to_ascii_uppercase();
        let has_page_field = full_xml_upper.contains("PAGE") && !full_xml_upper.contains("NUMPAGES")
            || (xml.contains("instrText") && contains_instr_token(&doc.root, "PAGE"))
            || contains_fld_simple_token(&doc.root, "PAGE");
        let has_numpages_field = (xml.contains("instrText") && contains_instr_token(&doc.root, "NUMPAGES"))
            || contains_fld_simple_token(&doc.root, "NUMPAGES");

        let mut fonts = HashSet::new();
        let mut sizes: Vec<f64> = Vec::new();
        let mut text_parts = Vec::new();
        for p in doc.root.children_elements().filter(|e| e.name == "w:p") {
            let txt = p.extract_text();
            if !txt.trim().is_empty() {
                text_parts.push(txt.trim().to_string());
            }
            for r in p.children_elements().filter(|e| e.name == "w:r") {
                if let Some(f) = extract_run_font(r) {
                    fonts.insert(f);
                }
                if let Some(sz) = r.child("w:rPr").and_then(read_rpr_sz_pt) {
                    if !sizes.iter().any(|&v| (v - sz).abs() < 0.1) {
                        sizes.push(sz);
                    }
                }
            }
        }
        let preview = text_parts.join(" | ");
        let trimmed_p = preview.trim();
        let static_page_digit_suspect = !has_page_field
            && !trimmed_p.is_empty()
            && (trimmed_p.chars().all(|c| c.is_ascii_digit() || c.is_whitespace() || c == '-')
                || trimmed_p.to_lowercase().starts_with("trang "));

        out.push(HeaderFooterSummary {
            part_name: name.clone(),
            kind: kind.to_string(),
            has_page_field,
            has_numpages_field,
            static_page_digit_suspect,
            font_names: fonts.into_iter().collect(),
            font_sizes_pt: sizes,
            text_preview: preview.chars().take(80).collect(),
        });
    }
    out
}

fn contains_instr_token(el: &XmlElement, token: &str) -> bool {
    if el.name == "w:instrText" {
        let up = el.extract_text().to_ascii_uppercase();
        if up.split_whitespace().any(|w| w == token) {
            return true;
        }
    }
    for sub in el.children_elements() {
        if contains_instr_token(sub, token) {
            return true;
        }
    }
    false
}

fn contains_fld_simple_token(el: &XmlElement, token: &str) -> bool {
    if el.name == "w:fldSimple" {
        if let Some(instr) = el.get_attr("w:instr") {
            if instr
                .to_ascii_uppercase()
                .split_whitespace()
                .any(|w| w == token)
            {
                return true;
            }
        }
    }
    for sub in el.children_elements() {
        if contains_fld_simple_token(sub, token) {
            return true;
        }
    }
    false
}

fn collect_all_rids(el: &XmlElement, out: &mut Vec<String>) {
    for (k, v) in &el.attrs {
        if (k == "r:id" || k == "r:embed" || k == "r:link") && !v.is_empty() {
            out.push(v.clone());
        }
    }
    for sub in el.children_elements() {
        collect_all_rids(sub, out);
    }
}

#[allow(clippy::too_many_arguments)]
fn analyze_paragraph(
    p: &XmlElement,
    body_child_idx: usize,
    in_table_idx: Option<usize>,
    global_index: usize,
    element_path: String,
    section_index: usize,
    styles: &StyleCatalog,
    near_admin_header: bool,
    in_sig_or_recipient: bool,
    past_doc_type_heading: bool,
    forced_role: Option<ParagraphRole>,
) -> ParagraphInfo {
    let text = p.extract_text();
    let trimmed = text.trim().to_string();
    let upper = collapse_spaces_upper(&trimmed);

    let p_pr = p.child("w:pPr");
    let style_id = p_pr
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"))
        .map(|s| s.to_string());
    let style_info = styles.effective_style(style_id.as_deref());

    let direct_jc = p_pr
        .and_then(|pp| pp.child("w:jc"))
        .and_then(|j| j.get_attr("w:val"))
        .map(|s| s.to_string());
    let effective_jc = direct_jc
        .clone()
        .or_else(|| style_info.and_then(|s| s.jc.clone()))
        .unwrap_or_else(|| "left".to_string());

    let ind = p_pr.and_then(|pp| pp.child("w:ind"));
    let indent_first_line = ind
        .and_then(|i| i.get_attr("w:firstLine"))
        .and_then(|v| v.parse().ok())
        .or_else(|| style_info.and_then(|s| s.indent_first_line))
        .unwrap_or(0);
    let indent_left = ind
        .and_then(|i| i.get_attr("w:left"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let indent_right = ind
        .and_then(|i| i.get_attr("w:right"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let sp = p_pr.and_then(|pp| pp.child("w:spacing"));
    let spacing_before = sp
        .and_then(|s| s.get_attr("w:before"))
        .and_then(|v| v.parse().ok())
        .or_else(|| style_info.and_then(|s| s.spacing_before))
        .unwrap_or(0);
    let spacing_after = sp
        .and_then(|s| s.get_attr("w:after"))
        .and_then(|v| v.parse().ok())
        .or_else(|| style_info.and_then(|s| s.spacing_after))
        .unwrap_or(0);
    let line_spacing = sp
        .and_then(|s| s.get_attr("w:line"))
        .and_then(|v| v.parse().ok())
        .or_else(|| style_info.and_then(|s| s.line_spacing))
        .unwrap_or(240);
    let line_rule = sp
        .and_then(|s| s.get_attr("w:lineRule"))
        .unwrap_or("auto")
        .to_string();

    let outline_lvl = p_pr
        .and_then(|pp| pp.child("w:outlineLvl"))
        .and_then(|o| o.get_attr("w:val"))
        .and_then(|v| v.parse().ok())
        .or_else(|| style_info.and_then(|s| s.outline_lvl));

    let has_soft_break = contains_soft_break(p);
    let is_empty_removable = is_removable_empty_paragraph(p);

    let mut font_set: Vec<String> = Vec::new();
    let mut size_set: Vec<f64> = Vec::new();
    let mut color_set: Vec<String> = Vec::new();
    let mut has_legacy_encoding = false;
    let mut has_missing_font_decl = false;
    let mut bold_chars = 0usize;
    let mut italic_chars = 0usize;
    let mut total_chars = 0usize;

    let mut has_drawing = false;
    let mut has_field = false;
    let mut has_hyperlink = false;
    let mut has_bookmark = false;
    let mut has_footnote_ref = false;
    let mut has_endnote_ref = false;
    let mut has_comment_ref = false;

    inspect_para_tree(
        p,
        style_id.as_deref(),
        styles,
        false,
        &mut font_set,
        &mut size_set,
        &mut color_set,
        &mut has_legacy_encoding,
        &mut has_missing_font_decl,
        &mut bold_chars,
        &mut italic_chars,
        &mut total_chars,
        &mut has_drawing,
        &mut has_field,
        &mut has_hyperlink,
        &mut has_bookmark,
        &mut has_footnote_ref,
        &mut has_endnote_ref,
        &mut has_comment_ref,
    );

    let is_bold_majority = (total_chars > 0 && bold_chars * 2 >= total_chars)
        || (style_info.and_then(|s| s.bold).unwrap_or(false) && bold_chars == 0);
    let is_italic_majority = (total_chars > 0 && italic_chars * 2 >= total_chars)
        || (style_info.and_then(|s| s.italic).unwrap_or(false) && italic_chars == 0);

    let replaceable_fonts: Vec<String> = font_set
        .iter()
        .filter(|f| !is_non_replaceable_font(f))
        .cloned()
        .collect();
    let has_mixed_fonts = replaceable_fonts.len() > 1;
    let primary_font = font_set
        .first()
        .cloned()
        .or_else(|| styles.effective_font(style_id.as_deref(), None))
        .unwrap_or_default();
    let primary_size_pt = size_set
        .first()
        .copied()
        .or_else(|| styles.effective_size_pt(style_id.as_deref(), None));

    let has_multi_space = if trimmed.is_empty() {
        false
    } else {
        re_multi_spaces().is_match(&text)
    };
    let has_punct_space_error = if trimmed.is_empty() {
        false
    } else {
        re_space_before_punct().is_match(&text)
    };
    let has_control_chars = text
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r');

    let mut has_direct_formatting_conflict = false;
    if let Some(st) = style_id.as_deref().and_then(|id| styles.by_id.get(id)) {
        if let (Some(ref st_jc), Some(ref d_jc)) = (&st.jc, &direct_jc) {
            if st_jc != d_jc {
                has_direct_formatting_conflict = true;
            }
        }
        if let Some(ref st_font) = st.font_name {
            if font_set
                .iter()
                .any(|f| !is_non_replaceable_font(f) && !f.eq_ignore_ascii_case(st_font))
            {
                has_direct_formatting_conflict = true;
            }
        }
    }

    let role = forced_role.unwrap_or_else(|| {
        classify_paragraph_role(
            p,
            global_index,
            &trimmed,
            &upper,
            style_id.as_deref(),
            &effective_jc,
            is_bold_majority,
            near_admin_header,
            in_sig_or_recipient,
            past_doc_type_heading,
        )
    });

    ParagraphInfo {
        body_child_idx,
        in_table_idx,
        global_index,
        element_path,
        section_index,
        role,
        text,
        trimmed,
        style_id,
        direct_jc,
        effective_jc,
        indent_first_line,
        indent_left,
        indent_right,
        spacing_before,
        spacing_after,
        line_spacing,
        line_rule,
        has_direct_formatting_conflict,
        has_soft_break,
        is_empty_removable,
        is_bold_majority,
        is_italic_majority,
        primary_font,
        font_names: font_set,
        primary_size_pt,
        sizes_pt: size_set,
        non_black_colors: color_set,
        has_legacy_encoding,
        has_mixed_fonts,
        has_missing_font_decl,
        has_multi_space,
        has_punct_space_error,
        has_control_chars,
        has_drawing,
        has_field,
        has_hyperlink,
        has_bookmark,
        has_footnote_ref,
        has_endnote_ref,
        has_comment_ref,
        outline_lvl,
    }
}

#[allow(clippy::too_many_arguments)]
fn inspect_para_tree(
    el: &XmlElement,
    p_style: Option<&str>,
    styles: &StyleCatalog,
    inside_hyperlink: bool,
    font_set: &mut Vec<String>,
    size_set: &mut Vec<f64>,
    color_set: &mut Vec<String>,
    has_legacy_encoding: &mut bool,
    has_missing_font_decl: &mut bool,
    bold_chars: &mut usize,
    italic_chars: &mut usize,
    total_chars: &mut usize,
    has_drawing: &mut bool,
    has_field: &mut bool,
    has_hyperlink: &mut bool,
    has_bookmark: &mut bool,
    has_footnote_ref: &mut bool,
    has_endnote_ref: &mut bool,
    has_comment_ref: &mut bool,
) {
    for node in &el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:hyperlink" => {
                *has_hyperlink = true;
                inspect_para_tree(
                    child,
                    p_style,
                    styles,
                    true,
                    font_set,
                    size_set,
                    color_set,
                    has_legacy_encoding,
                    has_missing_font_decl,
                    bold_chars,
                    italic_chars,
                    total_chars,
                    has_drawing,
                    has_field,
                    has_hyperlink,
                    has_bookmark,
                    has_footnote_ref,
                    has_endnote_ref,
                    has_comment_ref,
                );
            }
            "w:bookmarkStart" | "w:bookmarkEnd" => {
                *has_bookmark = true;
            }
            "w:commentRangeStart" | "w:commentRangeEnd" => {
                *has_comment_ref = true;
            }
            "w:fldSimple" => {
                *has_field = true;
                inspect_para_tree(
                    child,
                    p_style,
                    styles,
                    inside_hyperlink,
                    font_set,
                    size_set,
                    color_set,
                    has_legacy_encoding,
                    has_missing_font_decl,
                    bold_chars,
                    italic_chars,
                    total_chars,
                    has_drawing,
                    has_field,
                    has_hyperlink,
                    has_bookmark,
                    has_footnote_ref,
                    has_endnote_ref,
                    has_comment_ref,
                );
            }
            "w:drawing" | "w:pict" | "mc:AlternateContent" => {
                *has_drawing = true;
            }
            "w:r" => {
                inspect_run(
                    child,
                    p_style,
                    styles,
                    inside_hyperlink,
                    font_set,
                    size_set,
                    color_set,
                    has_legacy_encoding,
                    has_missing_font_decl,
                    bold_chars,
                    italic_chars,
                    total_chars,
                    has_drawing,
                    has_field,
                    has_footnote_ref,
                    has_endnote_ref,
                    has_comment_ref,
                );
            }
            "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                inspect_para_tree(
                    child,
                    p_style,
                    styles,
                    inside_hyperlink,
                    font_set,
                    size_set,
                    color_set,
                    has_legacy_encoding,
                    has_missing_font_decl,
                    bold_chars,
                    italic_chars,
                    total_chars,
                    has_drawing,
                    has_field,
                    has_hyperlink,
                    has_bookmark,
                    has_footnote_ref,
                    has_endnote_ref,
                    has_comment_ref,
                );
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn inspect_run(
    r: &XmlElement,
    p_style: Option<&str>,
    styles: &StyleCatalog,
    inside_hyperlink: bool,
    font_set: &mut Vec<String>,
    size_set: &mut Vec<f64>,
    color_set: &mut Vec<String>,
    has_legacy_encoding: &mut bool,
    has_missing_font_decl: &mut bool,
    bold_chars: &mut usize,
    italic_chars: &mut usize,
    total_chars: &mut usize,
    has_drawing: &mut bool,
    has_field: &mut bool,
    has_footnote_ref: &mut bool,
    has_endnote_ref: &mut bool,
    has_comment_ref: &mut bool,
) {
    let mut txt_len = 0usize;
    for sub in r.children_elements() {
        match sub.name.as_str() {
            "w:drawing" | "w:pict" | "mc:AlternateContent" => *has_drawing = true,
            "w:fldChar" | "w:instrText" => *has_field = true,
            "w:footnoteReference" => *has_footnote_ref = true,
            "w:endnoteReference" => *has_endnote_ref = true,
            "w:commentReference" => *has_comment_ref = true,
            "w:t" => txt_len += sub.extract_text().trim().chars().count(),
            _ => {}
        }
    }

    if txt_len == 0 {
        return;
    }

    *total_chars += txt_len;
    let r_pr = r.child("w:rPr");
    let r_style = r_pr
        .and_then(|rp| rp.child("w:rStyle"))
        .and_then(|rs| rs.get_attr("w:val"));

    let is_bold = r_pr
        .and_then(|rp| read_rpr_flag(rp, "w:b"))
        .unwrap_or_else(|| {
            p_style
                .and_then(|ps| styles.by_id.get(ps))
                .and_then(|s| s.bold)
                .unwrap_or(false)
        });
    let is_italic = r_pr
        .and_then(|rp| read_rpr_flag(rp, "w:i"))
        .unwrap_or_else(|| {
            p_style
                .and_then(|ps| styles.by_id.get(ps))
                .and_then(|s| s.italic)
                .unwrap_or(false)
        });
    if is_bold {
        *bold_chars += txt_len;
    }
    if is_italic {
        *italic_chars += txt_len;
    }

    if let Some(f) = extract_run_font(r).or_else(|| styles.effective_font(p_style, r_style)) {
        let fl = f.to_ascii_lowercase();
        if fl.starts_with(".vn") || fl.starts_with("vni-") {
            *has_legacy_encoding = true;
        }
        if !font_set.iter().any(|x| x.eq_ignore_ascii_case(&f)) {
            font_set.push(f);
        }
    } else {
        *has_missing_font_decl = true;
    }

    if let Some(sz) = r_pr
        .and_then(read_rpr_sz_pt)
        .or_else(|| styles.effective_size_pt(p_style, r_style))
    {
        if !size_set.iter().any(|&v| (v - sz).abs() < 0.1) {
            size_set.push(sz);
        }
    }

    if !inside_hyperlink {
        if let Some(col) = r_pr
            .and_then(|rp| rp.child("w:color"))
            .and_then(|c| c.get_attr("w:val"))
        {
            let cu = col.trim().to_ascii_uppercase();
            if cu != "000000" && cu != "AUTO" && !cu.is_empty() && !color_set.contains(&cu) {
                color_set.push(cu);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn classify_paragraph_role(
    p: &XmlElement,
    global_index: usize,
    trimmed: &str,
    upper: &str,
    style_id: Option<&str>,
    jc: &str,
    is_bold: bool,
    near_admin_header: bool,
    in_sig_or_recipient: bool,
    past_doc_type_heading: bool,
) -> ParagraphRole {
    if trimmed.is_empty() {
        return ParagraphRole::EmptyParagraph;
    }

    if (global_index <= 12 || near_admin_header)
        && (is_national_header_cue(upper) || upper.starts_with("SỐ:") || upper.starts_with("SỐ :"))
    {
        return ParagraphRole::AdminHeaderCell;
    }

    if (global_index <= 16 || near_admin_header)
        && !past_doc_type_heading
        && is_doc_type_title(upper)
        && (jc == "center" || is_bold)
    {
        return ParagraphRole::DocTypeHeading;
    }

    if (near_admin_header || (past_doc_type_heading && global_index <= 18))
        && (upper.starts_with("V/V") || upper.starts_with("VỀ VIỆC") || (jc == "center" && is_bold && !trimmed.ends_with('.') && trimmed.chars().count() <= 220))
    {
        return ParagraphRole::DocSubject;
    }

    if upper.starts_with("CĂN CỨ") && (global_index <= 25 || is_protected_admin_paragraph_ctx(p, true)) {
        return ParagraphRole::LegalBasisCanCu;
    }

    if upper.starts_with("KÍNH GỬI") && global_index <= 25 {
        return ParagraphRole::RecipientKinhGui;
    }

    if upper.starts_with("NƠI NHẬN") || (in_sig_or_recipient && (trimmed.starts_with('-') || trimmed.starts_with('+'))) {
        return ParagraphRole::RecipientNoiNhan;
    }

    if is_signature_trigger_paragraph(p, trimmed)
        || (in_sig_or_recipient && trimmed.chars().count() <= 60)
    {
        return ParagraphRole::SignatureBlock;
    }

    if upper.starts_with("PHỤ LỤC") && trimmed.chars().count() <= 140 && (is_bold || jc == "center") {
        return ParagraphRole::AppendixHeading;
    }

    if let Some(lvl) = detect_structural_heading_level(trimmed, upper, style_id, jc, is_bold) {
        return ParagraphRole::StructuralHeading(lvl);
    }

    if global_index <= 12
        && !past_doc_type_heading
        && !trimmed.ends_with('.')
        && is_protected_admin_paragraph_ctx(p, near_admin_header)
    {
        return ParagraphRole::AdminHeaderCell;
    }

    ParagraphRole::BodyProse
}

fn is_doc_type_title(upper: &str) -> bool {
    const TITLES: &[&str] = &[
        "QUYẾT ĐỊNH",
        "NGHỊ QUYẾT",
        "CHỈ THỊ",
        "QUY CHẾ",
        "QUY ĐỊNH",
        "THÔNG CÁO",
        "THÔNG BÁO",
        "HƯỚNG DẪN",
        "CHƯƠNG TRÌNH",
        "KẾ HOẠCH",
        "PHƯƠNG ÁN",
        "ĐỀ ÁN",
        "DỰ ÁN",
        "BÁO CÁO",
        "BIÊN BẢN",
        "TỜ TRÌNH",
        "HỢP ĐỒNG",
        "CÔNG ĐIỆN",
        "BẢN GHI NHỚ",
        "BẢN THỎA THUẬN",
        "GIẤY ỦY QUYỀN",
        "GIẤY MỜI",
        "GIẤY GIỚI THIỆU",
        "PHIẾU GỬI",
        "PHIẾU CHUYỂN",
        "PHIẾU BÁO",
        "LUẬT",
        "NGHỊ ĐỊNH",
        "THÔNG TƯ",
    ];
    TITLES.contains(&upper)
}

pub fn detect_structural_heading_level(
    trimmed: &str,
    upper: &str,
    style_id: Option<&str>,
    jc: &str,
    is_bold: bool,
) -> Option<u8> {
    if let Some(sid) = style_id {
        let low = sid.to_ascii_lowercase();
        if low == "heading1" || low == "heading 1" {
            return Some(1);
        }
        if low == "heading2" || low == "heading 2" {
            return Some(2);
        }
        if low == "heading3" || low == "heading 3" {
            return Some(3);
        }
        if low.starts_with("heading") {
            return Some(4);
        }
    }

    let len = trimmed.chars().count();
    if len > 160 || trimmed.is_empty() {
        return None;
    }

    if (upper.starts_with("PHẦN ") || upper.starts_with("CHƯƠNG ")) && !trimmed.ends_with('.') {
        return Some(1);
    }
    if re_roman_heading().is_match(trimmed) && (is_bold || jc == "center" || upper == trimmed) {
        return Some(1);
    }
    if upper.starts_with("MỤC ") && len <= 120 && !trimmed.ends_with('.') && (is_bold || jc == "center" || upper == trimmed) {
        return Some(2);
    }
    if re_dieu_heading().is_match(trimmed) && len <= 120 && is_bold {
        return Some(3);
    }
    if is_bold && !trimmed.ends_with('.') && re_decimal_heading().is_match(trimmed) {
        let dot_count = trimmed
            .split_whitespace()
            .next()
            .map(|tok| tok.trim_end_matches('.').matches('.').count())
            .unwrap_or(0);
        return Some(match dot_count {
            0 => 2,
            1 => 3,
            _ => 4,
        });
    }
    None
}

fn estimate_table_width_twips(tbl: &XmlElement, fallback: i32) -> i32 {
    if let Some(tbl_w) = tbl.child("w:tblPr").and_then(|tp| tp.child("w:tblW")) {
        let t = tbl_w.get_attr("w:type").unwrap_or("dxa");
        if t == "dxa" {
            if let Some(w) = tbl_w.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()) {
                if w > 0 {
                    return w;
                }
            }
        } else if t == "pct" {
            if let Some(pct) = tbl_w.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()) {
                if pct > 0 {
                    return ((fallback as i64 * pct as i64) / 5000) as i32;
                }
            }
        }
    }
    if let Some(grid) = tbl.child("w:tblGrid") {
        let sum: i32 = grid
            .children_elements()
            .filter(|e| e.name == "w:gridCol")
            .filter_map(|e| e.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()))
            .sum();
        if sum > 0 {
            return sum;
        }
    }
    fallback
}

fn build_asset_inventory(
    view: &SelectivePackageView,
    doc_root: &XmlElement,
    paragraphs: &[ParagraphInfo],
    _target_to_rid: &HashMap<String, String>,
) -> AssetInventory {
    let mut inv = AssetInventory::default();
    for m in &view.part_metas {
        let low = m.name.to_ascii_lowercase();
        if low.starts_with("word/media/") {
            inv.media_count += 1;
            inv.media_bytes += m.uncompressed_size;
        } else if low == "word/vbaproject.bin" {
            inv.has_vba_macros = true;
            inv.vba_bytes = m.uncompressed_size;
        }
    }

    inv.drawing_count = count_elements_recursive(doc_root, &["w:drawing", "w:pict", "mc:AlternateContent"]);
    inv.hyperlink_count = count_elements_recursive(doc_root, &["w:hyperlink"]);
    inv.bookmark_count = count_elements_recursive(doc_root, &["w:bookmarkStart"]);
    inv.field_count = paragraphs.iter().filter(|p| p.has_field).count()
        + count_elements_recursive(doc_root, &["w:fldSimple", "w:instrText"]);

    inv.footnote_count = count_note_entries(view.get_xml("word/footnotes.xml"), "w:footnote");
    inv.endnote_count = count_note_entries(view.get_xml("word/endnotes.xml"), "w:endnote");
    inv.comment_count = count_note_entries(view.get_xml("word/comments.xml"), "w:comment");

    inv
}

fn count_elements_recursive(el: &XmlElement, names: &[&str]) -> usize {
    let mut c = usize::from(names.contains(&el.name.as_str()));
    for sub in el.children_elements() {
        c += count_elements_recursive(sub, names);
    }
    c
}

fn count_note_entries(xml: Option<&str>, tag: &str) -> usize {
    let Some(raw) = xml else {
        return 0;
    };
    let Ok(doc) = XmlDocument::parse(raw) else {
        return 0;
    };
    doc.root
        .children_elements()
        .filter(|e| {
            if e.name != tag {
                return false;
            }
            // Word reserves w:type="separator" / "continuationSeparator" (ids -1, 0) for internal UI
            !matches!(
                e.get_attr("w:type"),
                Some("separator") | Some("continuationSeparator")
            )
        })
        .count()
}

fn resolve_scope(
    requested: StdDocScope,
    paragraphs: &[ParagraphInfo],
    tables: &[TableInfo],
    org: Option<&OrganizationProfile>,
) -> (StdDocScope, String, bool) {
    if requested != StdDocScope::AutoDetect && requested != StdDocScope::UnknownRequiresSelection {
        return (
            requested,
            format!("Áp dụng theo lựa chọn của người dùng ({:?}).", requested),
            false,
        );
    }

    let sample_text: String = paragraphs
        .iter()
        .take(40)
        .map(|p| collapse_spaces_upper(&p.trimmed))
        .collect::<Vec<_>>()
        .join("\n");

    let tail_text: String = paragraphs
        .iter()
        .rev()
        .take(15)
        .map(|p| collapse_spaces_upper(&p.trimmed))
        .collect::<Vec<_>>()
        .join("\n");

    if sample_text.contains("ĐẢNG CỘNG SẢN VIỆT NAM")
        || org.map(|o| o.sector == OrgSector::PartyOrg).unwrap_or(false)
    {
        return (
            StdDocScope::PartyDocument,
            "Nhận diện tiêu đề Đảng / Hồ sơ Tổ chức Đảng (Hướng dẫn 36-HD/VPTW).".to_string(),
            false,
        );
    }

    if (sample_text.contains("QUỐC HỘI")
        || sample_text.contains("NGHỊ ĐỊNH SỐ")
        || sample_text.contains("THÔNG TƯ SỐ")
        || paragraphs
            .iter()
            .take(15)
            .any(|p| matches!(p.trimmed.trim(), "LUẬT" | "NGHỊ ĐỊNH" | "THÔNG TƯ" | "PHÁP LỆNH")))
        && (sample_text.contains("CHƯƠNG ") || sample_text.contains("ĐIỀU 1"))
    {
        return (
            StdDocScope::SpecializedLegal,
            "Nhận diện văn bản quy phạm pháp luật / chuyên ngành (không áp dụng máy móc quy tắc hành chính).".to_string(),
            false,
        );
    }

    if sample_text.contains("LUẬN VĂN")
        || sample_text.contains("LUẬN ÁN")
        || sample_text.contains("KHÓA LUẬN")
        || sample_text.contains("ĐỒ ÁN TỐT NGHIỆP")
        || tail_text.contains("TÀI LIỆU THAM KHẢO")
    {
        return (
            StdDocScope::AcademicThesis,
            "Nhận diện văn bản luận văn / báo cáo học thuật.".to_string(),
            false,
        );
    }

    let has_quoc_hieu = sample_text.contains("CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM")
        || sample_text.contains("ĐỘC LẬP - TỰ DO - HẠNH PHÚC");
    let has_admin_cues = has_quoc_hieu
        || sample_text.contains("SỐ:")
        || sample_text.contains("KÍNH GỬI")
        || tail_text.contains("NƠI NHẬN:")
        || tables.iter().any(|t| t.admin_kind != AdminTableKind::None)
        || paragraphs.iter().any(|p| {
            matches!(
                p.role,
                ParagraphRole::DocTypeHeading | ParagraphRole::LegalBasisCanCu | ParagraphRole::RecipientNoiNhan
            )
        });

    if let Some(o) = org {
        if o.sector == OrgSector::Enterprise {
            return (
                StdDocScope::EnterpriseInternal,
                if o.use_nd30_layout_for_enterprise {
                    "Hồ sơ Doanh nghiệp có bật tương thích NĐ 30/2020/NĐ-CP.".to_string()
                } else {
                    "Hồ sơ Doanh nghiệp (bộ quy tắc linh hoạt, không áp đặt quy định công quyền).".to_string()
                },
                false,
            );
        }
        if o.sector == OrgSector::StateAgency && has_admin_cues {
            return (
                StdDocScope::AdminNd30,
                "Hồ sơ Cơ quan Nhà nước kết hợp dấu hiệu văn bản hành chính NĐ 30/2020/NĐ-CP.".to_string(),
                false,
            );
        }
    }

    if has_admin_cues {
        return (
            StdDocScope::AdminNd30,
            "Nhận diện dấu hiệu thể thức văn bản hành chính (Quốc hiệu / Số ký hiệu / Kính gửi / Nơi nhận / Tên loại).".to_string(),
            false,
        );
    }

    (
        StdDocScope::UnknownRequiresSelection,
        "Không đủ dấu hiệu chắc chắn để tự phân loại (hành chính / doanh nghiệp / chuyên ngành / học thuật / tự do). Vui lòng chọn phạm vi áp dụng.".to_string(),
        true,
    )
}

pub fn read_rpr_font(r_pr: &XmlElement) -> Option<String> {
    let rf = r_pr.child("w:rFonts")?;
    rf.get_attr("w:ascii")
        .or_else(|| rf.get_attr("w:hAnsi"))
        .or_else(|| rf.get_attr("w:cs"))
        .map(|s| s.to_string())
}

pub fn read_rpr_sz_pt(r_pr: &XmlElement) -> Option<f64> {
    let half_pts: f64 = r_pr
        .child("w:sz")
        .or_else(|| r_pr.child("w:szCs"))
        .and_then(|s| s.get_attr("w:val"))
        .and_then(|v| v.parse().ok())?;
    Some(half_pts / 2.0)
}

pub fn read_rpr_flag(r_pr: &XmlElement, tag: &str) -> Option<bool> {
    let el = r_pr.child(tag)?;
    Some(el.get_attr("w:val") != Some("0") && el.get_attr("w:val") != Some("false"))
}

pub fn is_non_replaceable_font(font_name: &str) -> bool {
    let low = font_name.trim().to_ascii_lowercase();
    NON_REPLACEABLE_SYMBOL_FONTS.iter().any(|&f| low == f)
}

fn re_multi_spaces() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r" {2,}").expect("valid"))
}

fn re_space_before_punct() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+([,.;:!?\)\]”])").expect("valid"))
}

fn re_roman_heading() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[IVXLCDM]+\.\s+\S+").expect("valid"))
}

fn re_dieu_heading() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^Điều\s+\d+[\.:]").expect("valid"))
}

fn re_decimal_heading() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d+(\.\d+)*\.?\s+\S+").expect("valid"))
}
