use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::page_toc::{ensure_content_type_override, ensure_doc_relationship};
use crate::engine::std_engine::ir::{is_non_replaceable_font, DocumentIr, ParagraphInfo, ParagraphRole};
use crate::engine::vn_cleaner::{
    clean_paragraph_text, convert_legacy_runs_in_element, is_removable_empty_paragraph,
    split_soft_breaks_in_paragraph, CleanStats,
};
use crate::engine::xml_helpers::{
    set_run_font_and_size, XmlDocument, XmlElement, XmlNode, P_PR_ORDER, R_PR_ORDER,
    SECT_PR_ORDER, TBL_PR_ORDER, TR_PR_ORDER,
};
use crate::models::{
    CleanerConfig, FixActionRecord, FixSafetyTier, FindingDisposition, QuoteStyle,
    StdAnalysisReport, StdExecutionRequest, StdFinding,
};
use chrono::Local;
use std::collections::HashSet;

pub struct FixApplicationOutput {
    pub actions: Vec<FixActionRecord>,
    pub logs: Vec<String>,
    pub updated_findings: Vec<StdFinding>,
}

pub fn apply_selected_fixes(
    pkg: &mut OpenXmlPackage,
    ir: &DocumentIr,
    report: &StdAnalysisReport,
    req: &StdExecutionRequest,
) -> Result<FixApplicationOutput, String> {
    let disabled_rules: HashSet<&str> = req.disabled_rule_ids.iter().map(|s| s.as_str()).collect();
    let approved_set: HashSet<&str> = req.approved_finding_ids.iter().map(|s| s.as_str()).collect();
    let skipped_set: HashSet<&str> = req.skipped_finding_ids.iter().map(|s| s.as_str()).collect();
    let manual_set: HashSet<&str> = req
        .manual_review_finding_ids
        .iter()
        .map(|s| s.as_str())
        .collect();

    let mut updated_findings = report.findings.clone();
    let mut active_findings: Vec<StdFinding> = Vec::new();

    for f in &mut updated_findings {
        if disabled_rules.contains(f.rule_id.as_str()) || skipped_set.contains(f.finding_id.as_str()) {
            f.disposition = FindingDisposition::Skipped;
            continue;
        }
        if manual_set.contains(f.finding_id.as_str())
            || f.safety_tier == FixSafetyTier::GroupCManualOnly
            || !f.auto_fix_supported
        {
            if manual_set.contains(f.finding_id.as_str()) {
                f.disposition = FindingDisposition::NeedsManualReview;
            }
            continue;
        }

        let explicit_approved = approved_set.contains(f.finding_id.as_str())
            || approved_set.contains(f.rule_id.as_str());

        let should_run = match f.safety_tier {
            FixSafetyTier::GroupAAuto => req.apply_all_group_a || explicit_approved,
            FixSafetyTier::GroupBConfirm => explicit_approved,
            FixSafetyTier::GroupCManualOnly => false,
        };

        if should_run {
            f.disposition = if f.safety_tier == FixSafetyTier::GroupAAuto && !explicit_approved {
                FindingDisposition::AutoFixed
            } else {
                FindingDisposition::ApprovedFix
            };
            active_findings.push(f.clone());
        }
    }

    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Không tìm thấy word/document.xml để sửa.".to_string())?;
    let mut doc = XmlDocument::parse(&doc_xml)?;

    let mut actions: Vec<FixActionRecord> = Vec::new();
    let mut logs: Vec<String> = Vec::new();
    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    if active_findings.is_empty() {
        logs.push("Không có lỗi Nhóm A/B nào được chọn sửa trong lượt này (dữ liệu tệp giữ nguyên).".to_string());
        return Ok(FixApplicationOutput {
            actions,
            logs,
            updated_findings,
        });
    }

    // Group findings by rule category/order for deterministic, non-overlapping DOM passes
    let mut need_heading_styles = false;
    let mut para4_delete_body_indices: HashSet<usize> = HashSet::new();
    let mut para4_softbreak_paths: HashSet<String> = HashSet::new();
    let mut pgnum_fix_requested: Option<StdFinding> = None;

    for f in &active_findings {
        match f.rule_id.as_str() {
            "PAGE-01" | "PAGE-02" => {
                if let Some(sect_num) = parse_bracket_num(&f.element_path, "sect[") {
                    if let Some(body) = doc.root.child_mut("w:body") {
                        if let Some(sect_pr) = get_section_pr_mut(body, sect_num) {
                            let ok_desc = if f.rule_id == "PAGE-01" {
                                fix_section_a4(sect_pr)
                            } else {
                                fix_section_margins(sect_pr)
                            };
                            actions.push(FixActionRecord {
                                action_id: format!("ACT-{}", actions.len() + 1),
                                rule_id: f.rule_id.clone(),
                                safety_tier: f.safety_tier,
                                location: f.location.clone(),
                                element_path: f.element_path.clone(),
                                before_value: f.current_value.clone(),
                                after_value: ok_desc,
                                timestamp: stamp.clone(),
                                reversible: true,
                            });
                        }
                    }
                }
            }
            "FONT-01" | "FONT-02" | "FONT-03" | "FONT-04" | "FONT-05" | "PARA-01" | "PARA-02"
            | "PARA-03" | "PARA-05" | "STRUCT-01" | "STRUCT-02" | "STRUCT-03" => {
                if f.rule_id == "STRUCT-03" {
                    need_heading_styles = true;
                }
                let p_meta = ir.paragraphs.iter().find(|p| p.element_path == f.element_path);
                if let Some(body) = doc.root.child_mut("w:body") {
                    if let Some(p_el) = find_paragraph_by_path_mut(body, &f.element_path) {
                        let after_desc = apply_paragraph_rule_fix(
                            p_el,
                            p_meta,
                            &f.rule_id,
                            &mut logs,
                        );
                        if let Some(after_val) = after_desc {
                            actions.push(FixActionRecord {
                                action_id: format!("ACT-{}", actions.len() + 1),
                                rule_id: f.rule_id.clone(),
                                safety_tier: f.safety_tier,
                                location: f.location.clone(),
                                element_path: f.element_path.clone(),
                                before_value: f.current_value.clone(),
                                after_value: after_val,
                                timestamp: stamp.clone(),
                                reversible: true,
                            });
                        }
                    }
                }
            }
            "TBL-01" | "TBL-02" | "TBL-03" => {
                let t_meta = ir
                    .tables
                    .iter()
                    .find(|t| t.summary.element_path == f.element_path);
                if let (Some(t_info), Some(body)) = (t_meta, doc.root.child_mut("w:body")) {
                    if let Some(tbl_el) = body
                        .children
                        .get_mut(t_info.body_child_idx)
                        .and_then(|n| match n {
                            XmlNode::Element(e) if e.name == "w:tbl" => Some(e),
                            _ => None,
                        })
                    {
                        let after_val = match f.rule_id.as_str() {
                            "TBL-01" => fix_table_width(
                                tbl_el,
                                t_info.summary.total_width_twips,
                                t_info.summary.printable_width_twips,
                            ),
                            "TBL-02" => fix_table_pagination(tbl_el, &t_info.locked_long_row_indices),
                            _ => fix_table_cells_font_and_indent(tbl_el),
                        };
                        actions.push(FixActionRecord {
                            action_id: format!("ACT-{}", actions.len() + 1),
                            rule_id: f.rule_id.clone(),
                            safety_tier: f.safety_tier,
                            location: f.location.clone(),
                            element_path: f.element_path.clone(),
                            before_value: f.current_value.clone(),
                            after_value: after_val,
                            timestamp: stamp.clone(),
                            reversible: true,
                        });
                    }
                }
            }
            "PGNUM-01" => {
                pgnum_fix_requested = Some(f.clone());
            }
            "PARA-04" => {
                if let Some(p_meta) = ir.paragraphs.iter().find(|p| p.element_path == f.element_path) {
                    if p_meta.role == ParagraphRole::EmptyParagraph {
                        para4_delete_body_indices.insert(p_meta.body_child_idx);
                    } else if p_meta.has_soft_break {
                        para4_softbreak_paths.insert(f.element_path.clone());
                    }
                }
            }
            _ => {}
        }
    }

    if need_heading_styles {
        ensure_black_heading_styles_in_pkg(pkg)?;
        logs.push("Đã chuẩn hóa Heading1–4 trong word/styles.xml (Times New Roman, màu đen).".to_string());
    }

    if let Some(pg_f) = pgnum_fix_requested {
        let after_val = apply_nd30_page_number_header(pkg, &mut doc)?;
        actions.push(FixActionRecord {
            action_id: format!("ACT-{}", actions.len() + 1),
            rule_id: pg_f.rule_id.clone(),
            safety_tier: pg_f.safety_tier,
            location: pg_f.location.clone(),
            element_path: pg_f.element_path.clone(),
            before_value: pg_f.current_value.clone(),
            after_value: after_val,
            timestamp: stamp.clone(),
            reversible: true,
        });
    }

    // Apply PARA-04 structural mutations LAST so earlier element_path indices remain exact
    if !para4_delete_body_indices.is_empty() || !para4_softbreak_paths.is_empty() {
        if let Some(body) = doc.root.child_mut("w:body") {
            let mut new_children: Vec<XmlNode> = Vec::with_capacity(body.children.len());
            for (idx, node) in std::mem::take(&mut body.children).into_iter().enumerate() {
                let path = format!("body/p[{}]", idx);
                if para4_delete_body_indices.contains(&idx) {
                    if let XmlNode::Element(ref el) = node {
                        if el.name == "w:p" && is_removable_empty_paragraph(el) {
                            actions.push(FixActionRecord {
                                action_id: format!("ACT-{}", actions.len() + 1),
                                rule_id: "PARA-04".to_string(),
                                safety_tier: FixSafetyTier::GroupAAuto,
                                location: format!("Đoạn body[{}]", idx),
                                element_path: path,
                                before_value: "Đoạn trống thừa liên tiếp".to_string(),
                                after_value: "Đã xóa đoạn trống thừa".to_string(),
                                timestamp: stamp.clone(),
                                reversible: true,
                            });
                            continue;
                        }
                    }
                }
                if para4_softbreak_paths.contains(&path) {
                    if let XmlNode::Element(p) = node {
                        let mut st = CleanStats::default();
                        let split = split_soft_breaks_in_paragraph(p, &mut st);
                        let cnt = split.len();
                        for sp in split {
                            new_children.push(XmlNode::Element(sp));
                        }
                        actions.push(FixActionRecord {
                            action_id: format!("ACT-{}", actions.len() + 1),
                            rule_id: "PARA-04".to_string(),
                            safety_tier: FixSafetyTier::GroupAAuto,
                            location: format!("Đoạn body[{}]", idx),
                            element_path: path,
                            before_value: "Chứa ngắt dòng mềm <w:br/>".to_string(),
                            after_value: format!("Đã tách thành {} đoạn chuẩn", cnt),
                            timestamp: stamp.clone(),
                            reversible: true,
                        });
                        continue;
                    }
                }
                new_children.push(node);
            }
            body.children = new_children;
        }
    }

    pkg.set_xml("word/document.xml", doc.to_xml_string()?);
    logs.push(format!("Đã thực thi {} thao tác sửa chọn lọc trên cấu trúc OpenXML.", actions.len()));

    Ok(FixApplicationOutput {
        actions,
        logs,
        updated_findings,
    })
}

fn parse_bracket_num(s: &str, prefix: &str) -> Option<usize> {
    let start = s.find(prefix)? + prefix.len();
    let rest = &s[start..];
    let end = rest.find(']')?;
    rest[..end].parse::<usize>().ok()
}

fn get_section_pr_mut<'a>(body: &'a mut XmlElement, target_sect_1based: usize) -> Option<&'a mut XmlElement> {
    let mut cur = 0usize;
    for node in &mut body.children {
        if let XmlNode::Element(el) = node {
            if el.name == "w:p" {
                if el
                    .child("w:pPr")
                    .and_then(|pp| pp.child("w:sectPr"))
                    .is_some()
                {
                    cur += 1;
                    if cur == target_sect_1based {
                        return el
                            .child_mut("w:pPr")
                            .and_then(|pp| pp.child_mut("w:sectPr"));
                    }
                }
            } else if el.name == "w:sectPr" {
                cur += 1;
                if cur == target_sect_1based {
                    return Some(el);
                }
            }
        }
    }
    None
}

fn find_paragraph_by_path_mut<'a>(body: &'a mut XmlElement, path: &str) -> Option<&'a mut XmlElement> {
    if path.starts_with("body/p[") {
        let idx = parse_bracket_num(path, "body/p[")?;
        return match body.children.get_mut(idx)? {
            XmlNode::Element(p) if p.name == "w:p" => Some(p),
            _ => None,
        };
    }
    if path.starts_with("body/tbl[") {
        let tbl_idx = parse_bracket_num(path, "body/tbl[")?;
        let tr_idx = parse_bracket_num(path, "/tr[")?;
        let tc_idx = parse_bracket_num(path, "/tc[")?;
        let p_idx = parse_bracket_num(path, "/p[")?;
        let tbl = match body.children.get_mut(tbl_idx)? {
            XmlNode::Element(t) if t.name == "w:tbl" => t,
            _ => return None,
        };
        let mut r_cur = 0usize;
        for tr_n in &mut tbl.children {
            if let XmlNode::Element(tr) = tr_n {
                if tr.name == "w:tr" {
                    if r_cur == tr_idx {
                        let mut c_cur = 0usize;
                        for tc_n in &mut tr.children {
                            if let XmlNode::Element(tc) = tc_n {
                                if tc.name == "w:tc" {
                                    if c_cur == tc_idx {
                                        let mut p_cur = 0usize;
                                        for p_n in &mut tc.children {
                                            if let XmlNode::Element(p) = p_n {
                                                if p.name == "w:p" {
                                                    if p_cur == p_idx {
                                                        return Some(p);
                                                    }
                                                    p_cur += 1;
                                                }
                                            }
                                        }
                                        return None;
                                    }
                                    c_cur += 1;
                                }
                            }
                        }
                        return None;
                    }
                    r_cur += 1;
                }
            }
        }
    }
    None
}

fn fix_section_a4(sect_pr: &mut XmlElement) -> String {
    let is_land = sect_pr
        .child("w:pgSz")
        .and_then(|s| s.get_attr("w:orient"))
        .map(|o| o == "landscape")
        .unwrap_or(false);
    let pg_sz = sect_pr.ensure_child_ordered("w:pgSz", SECT_PR_ORDER);
    if is_land {
        pg_sz.set_attr("w:w", "16838");
        pg_sz.set_attr("w:h", "11906");
        pg_sz.set_attr("w:orient", "landscape");
        "297.0 × 210.0 mm (A4 ngang)".to_string()
    } else {
        pg_sz.set_attr("w:w", "11906");
        pg_sz.set_attr("w:h", "16838");
        pg_sz.remove_attr("w:orient");
        "210.0 × 297.0 mm (A4 dọc)".to_string()
    }
}

fn fix_section_margins(sect_pr: &mut XmlElement) -> String {
    let is_land = sect_pr
        .child("w:pgSz")
        .and_then(|s| s.get_attr("w:orient"))
        .map(|o| o == "landscape")
        .unwrap_or(false);
    let mar = sect_pr.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
    if is_land {
        mar.set_attr("w:top", "1701");
        mar.set_attr("w:bottom", "1134");
        mar.set_attr("w:left", "1134");
        mar.set_attr("w:right", "1134");
        "Lề ngang: Trên 30.0 mm · Dưới/Trái/Phải 20.0 mm".to_string()
    } else {
        mar.set_attr("w:top", "1134");
        mar.set_attr("w:bottom", "1134");
        mar.set_attr("w:left", "1701");
        mar.set_attr("w:right", "1134");
        "Lề dọc: Trên 20.0 mm · Dưới 20.0 mm · Trái 30.0 mm · Phải 20.0 mm".to_string()
    }
}

fn apply_paragraph_rule_fix(
    p: &mut XmlElement,
    p_info: Option<&ParagraphInfo>,
    rule_id: &str,
    _logs: &mut Vec<String>,
) -> Option<String> {
    let role = p_info.map(|pi| pi.role).unwrap_or(ParagraphRole::BodyProse);
    match rule_id {
        "FONT-01" => {
            let mut st = CleanStats::default();
            convert_legacy_runs_in_element(p, &mut st);
            Some(format!("Unicode Times New Roman ({} run đã chuyển mã)", st.legacy_runs_converted))
        }
        "FONT-02" => {
            set_paragraph_font_preserving_anchors(p, "Times New Roman");
            Some("Times New Roman".to_string())
        }
        "FONT-03" => {
            let target_pt = match role {
                ParagraphRole::AdminHeaderCell => 13.0,
                ParagraphRole::RecipientNoiNhan => 11.0,
                ParagraphRole::DataTableCell => 12.0,
                _ => 14.0,
            };
            set_paragraph_size_preserving_anchors(p, target_pt);
            Some(format!("{:.0}pt", target_pt))
        }
        "FONT-04" => {
            set_paragraph_black_color_preserving_hyperlinks(p);
            Some("#000000 (Đen)".to_string())
        }
        "FONT-05" => {
            strip_control_chars_in_element(p);
            let mut st = CleanStats::default();
            let cleaner_cfg = CleanerConfig {
                enabled: true,
                fix_multiple_spaces: true,
                fix_punctuation_spacing: true,
                fix_soft_breaks: false,
                remove_empty_paragraphs: false,
                quote_style: QuoteStyle::Keep,
                normalize_nd30_dates: false,
                convert_legacy_encoding: false,
            };
            clean_paragraph_text(p, &cleaner_cfg, &mut st);
            Some("Đã chuẩn hóa khoảng trắng & xóa ký tự điều khiển".to_string())
        }
        "PARA-01" => {
            let p_pr = p.ensure_child_first("w:pPr");
            let jc = p_pr.ensure_child_ordered("w:jc", P_PR_ORDER);
            jc.set_attr("w:val", "both");
            Some("both (Căn đều hai bên)".to_string())
        }
        "PARA-02" => {
            let p_pr = p.ensure_child_first("w:pPr");
            let ind = p_pr.ensure_child_ordered("w:ind", P_PR_ORDER);
            ind.set_attr("w:firstLine", "720");
            ind.remove_attr("w:left");
            ind.remove_attr("w:right");
            ind.remove_attr("w:hanging");
            Some("Thụt đầu dòng 1.27 cm (720 twips); lề trái/phải = 0".to_string())
        }
        "PARA-03" => {
            let p_pr = p.ensure_child_first("w:pPr");
            let sp = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
            sp.set_attr("w:before", "0");
            sp.set_attr("w:after", "120");
            sp.set_attr("w:line", "276");
            sp.set_attr("w:lineRule", "auto");
            Some("Giãn dòng 1.15x (276) · Trước 0pt · Sau 6pt (120)".to_string())
        }
        "PARA-05" => {
            if let Some(p_pr) = p.child_mut("w:pPr") {
                p_pr.remove_children_by_name("w:jc");
            }
            strip_direct_run_fonts_when_styled(p);
            Some("Đã gỡ định dạng ghi đè để đồng bộ theo Paragraph Style".to_string())
        }
        "STRUCT-01" => {
            if role == ParagraphRole::LegalBasisCanCu {
                set_paragraph_flag_preserving_anchors(p, "w:i");
                Some("In nghiêng (Căn cứ pháp lý)".to_string())
            } else {
                set_paragraph_flag_preserving_anchors(p, "w:b");
                let p_pr = p.ensure_child_first("w:pPr");
                p_pr.ensure_child_ordered("w:jc", P_PR_ORDER)
                    .set_attr("w:val", "center");
                Some("In đậm, căn giữa (Tên loại VB)".to_string())
            }
        }
        "STRUCT-02" => {
            set_paragraph_flag_preserving_anchors(p, "w:b");
            if role == ParagraphRole::StructuralHeading(1) || role == ParagraphRole::AppendixHeading {
                let p_pr = p.ensure_child_first("w:pPr");
                p_pr.ensure_child_ordered("w:jc", P_PR_ORDER)
                    .set_attr("w:val", "center");
            }
            Some("In đậm chuẩn đề mục (bảo toàn số hiệu & nội dung gốc)".to_string())
        }
        "STRUCT-03" => {
            if let ParagraphRole::StructuralHeading(lvl) = role {
                let sid = format!("Heading{}", lvl);
                let p_pr = p.ensure_child_first("w:pPr");
                p_pr.ensure_child_ordered("w:pStyle", P_PR_ORDER)
                    .set_attr("w:val", &sid);
                return Some(format!("Đã gán Style {}", sid));
            }
            None
        }
        _ => None,
    }
}

fn set_paragraph_font_preserving_anchors(el: &mut XmlElement, font_name: &str) {
    for node in &mut el.children {
        if let XmlNode::Element(child) = node {
            match child.name.as_str() {
                "w:r" => {
                    if run_should_preserve_font(child) {
                        continue;
                    }
                    let r_pr = child.ensure_child_first("w:rPr");
                    set_run_font_and_size(r_pr, font_name, None);
                }
                "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                    set_paragraph_font_preserving_anchors(child, font_name);
                }
                _ => {}
            }
        }
    }
}

fn set_paragraph_size_preserving_anchors(el: &mut XmlElement, size_pt: f64) {
    for node in &mut el.children {
        if let XmlNode::Element(child) = node {
            match child.name.as_str() {
                "w:r" => {
                    let has_text = child.children_elements().any(|s| s.name == "w:t");
                    if !has_text {
                        continue;
                    }
                    let r_pr = child.ensure_child_first("w:rPr");
                    let cur_font = r_pr
                        .child("w:rFonts")
                        .and_then(|f| f.get_attr("w:ascii"))
                        .unwrap_or("")
                        .to_string();
                    if is_non_replaceable_font(&cur_font) {
                        continue;
                    }
                    let hp = (size_pt * 2.0).round() as i32;
                    r_pr.ensure_child_ordered("w:sz", R_PR_ORDER)
                        .set_attr("w:val", hp.to_string());
                    r_pr.ensure_child_ordered("w:szCs", R_PR_ORDER)
                        .set_attr("w:val", hp.to_string());
                }
                "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                    set_paragraph_size_preserving_anchors(child, size_pt);
                }
                _ => {}
            }
        }
    }
}

fn set_paragraph_black_color_preserving_hyperlinks(el: &mut XmlElement) {
    for node in &mut el.children {
        if let XmlNode::Element(child) = node {
            match child.name.as_str() {
                "w:r" => {
                    if !child.children_elements().any(|s| s.name == "w:t") {
                        continue;
                    }
                    let r_pr = child.ensure_child_first("w:rPr");
                    let col = r_pr.ensure_child_ordered("w:color", R_PR_ORDER);
                    col.set_attr("w:val", "000000");
                    col.remove_attr("w:themeColor");
                    col.remove_attr("w:themeShade");
                    col.remove_attr("w:themeTint");
                }
                "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" => {
                    // Notice: deliberately skips "w:hyperlink"
                    set_paragraph_black_color_preserving_hyperlinks(child);
                }
                _ => {}
            }
        }
    }
}

fn set_paragraph_flag_preserving_anchors(el: &mut XmlElement, tag: &str) {
    for node in &mut el.children {
        if let XmlNode::Element(child) = node {
            if child.name == "w:r" && child.children_elements().any(|s| s.name == "w:t") {
                let r_pr = child.ensure_child_first("w:rPr");
                let flag = r_pr.ensure_child_ordered(tag, R_PR_ORDER);
                flag.remove_attr("w:val");
            }
        }
    }
}

fn strip_direct_run_fonts_when_styled(el: &mut XmlElement) {
    for node in &mut el.children {
        if let XmlNode::Element(child) = node {
            if child.name == "w:r" {
                if let Some(r_pr) = child.child_mut("w:rPr") {
                    r_pr.remove_children_by_name("w:rFonts");
                }
            }
        }
    }
}

fn strip_control_chars_in_element(el: &mut XmlElement) {
    for node in &mut el.children {
        match node {
            XmlNode::Text(t) => {
                if t.chars()
                    .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
                {
                    *t = t
                        .chars()
                        .filter(|c| !c.is_control() || *c == '\t' || *c == '\n' || *c == '\r')
                        .collect();
                }
            }
            XmlNode::Element(sub) => {
                if sub.name != "w:instrText" {
                    strip_control_chars_in_element(sub);
                }
            }
            _ => {}
        }
    }
}

fn run_should_preserve_font(r: &XmlElement) -> bool {
    let has_text = r.children_elements().any(|s| s.name == "w:t");
    if !has_text {
        return true;
    }
    if let Some(rf) = r.child("w:rPr").and_then(|rp| rp.child("w:rFonts")) {
        if let Some(name) = rf.get_attr("w:ascii").or_else(|| rf.get_attr("w:hAnsi")) {
            if is_non_replaceable_font(name) {
                return true;
            }
        }
    }
    false
}

fn fix_table_width(tbl: &mut XmlElement, total_twips: i32, printable_twips: i32) -> String {
    let tbl_pr = tbl.ensure_child_first("w:tblPr");
    let tw = tbl_pr.ensure_child_ordered("w:tblW", TBL_PR_ORDER);
    tw.set_attr("w:w", printable_twips.to_string());
    tw.set_attr("w:type", "dxa");
    tbl_pr
        .ensure_child_ordered("w:tblLayout", TBL_PR_ORDER)
        .set_attr("w:type", "fixed");

    let ratio = if total_twips > 0 {
        printable_twips as f64 / total_twips as f64
    } else {
        1.0
    };

    if let Some(grid) = tbl.child_mut("w:tblGrid") {
        let mut cols: Vec<i32> = grid
            .children_elements()
            .filter(|e| e.name == "w:gridCol")
            .map(|e| e.get_attr("w:w").and_then(|v| v.parse().ok()).unwrap_or(1000))
            .collect();
        let scaled: Vec<i32> = cols
            .iter()
            .map(|&w| ((w as f64 * ratio).round() as i32).max(400))
            .collect();
        let sum: i32 = scaled.iter().sum();
        if let Some(last) = cols.last_mut() {
            let _ = last;
        }
        let diff = printable_twips - sum;
        for (i, gc) in grid
            .children
            .iter_mut()
            .filter_map(|n| match n {
                XmlNode::Element(e) if e.name == "w:gridCol" => Some(e),
                _ => None,
            })
            .enumerate()
        {
            let mut w = *scaled.get(i).unwrap_or(&1000);
            if i + 1 == scaled.len() {
                w = (w + diff).max(400);
            }
            gc.set_attr("w:w", w.to_string());
        }
    }

    for tr_n in &mut tbl.children {
        if let XmlNode::Element(tr) = tr_n {
            if tr.name == "w:tr" {
                for tc_n in &mut tr.children {
                    if let XmlNode::Element(tc) = tc_n {
                        if tc.name == "w:tc" {
                            if let Some(tc_w) = tc.child_mut("w:tcPr").and_then(|tp| tp.child_mut("w:tcW")) {
                                if tc_w.get_attr("w:type").unwrap_or("dxa") == "dxa" {
                                    if let Some(cur) = tc_w.get_attr("w:w").and_then(|v| v.parse::<i32>().ok()) {
                                        let next = ((cur as f64 * ratio).round() as i32).max(400);
                                        tc_w.set_attr("w:w", next.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    format!(
        "Co tỷ lệ vừa vùng in {:.1} mm ({} twips)",
        printable_twips as f64 / 56.6929,
        printable_twips
    )
}

fn fix_table_pagination(tbl: &mut XmlElement, locked_long_rows: &[usize]) -> String {
    let locked_set: HashSet<usize> = locked_long_rows.iter().copied().collect();
    let mut row_idx = 0usize;
    for tr_n in &mut tbl.children {
        if let XmlNode::Element(tr) = tr_n {
            if tr.name == "w:tr" {
                let tr_pr = tr.ensure_child_first("w:trPr");
                if row_idx == 0 {
                    tr_pr.ensure_child_ordered("w:tblHeader", TR_PR_ORDER);
                } else if locked_set.contains(&row_idx) {
                    tr_pr.remove_children_by_name("w:cantSplit");
                    tr_pr.remove_children_by_name("w:trHeight");
                }
                row_idx += 1;
            }
        }
    }
    "Đã bật w:tblHeader hàng đầu & gỡ khóa ngắt trang hàng dài".to_string()
}

fn fix_table_cells_font_and_indent(tbl: &mut XmlElement) -> String {
    for tr_n in &mut tbl.children {
        if let XmlNode::Element(tr) = tr_n {
            if tr.name == "w:tr" {
                for tc_n in &mut tr.children {
                    if let XmlNode::Element(tc) = tc_n {
                        if tc.name == "w:tc" {
                            for p_n in &mut tc.children {
                                if let XmlNode::Element(p) = p_n {
                                    if p.name == "w:p" {
                                        set_paragraph_font_preserving_anchors(p, "Times New Roman");
                                        if let Some(ind) = p.child_mut("w:pPr").and_then(|pp| pp.child_mut("w:ind")) {
                                            ind.set_attr("w:firstLine", "0");
                                            ind.remove_attr("w:left");
                                        }
                                        // Clamp out-of-range cell font sizes to 12pt (24)
                                        for r_n in &mut p.children {
                                            if let XmlNode::Element(r) = r_n {
                                                if r.name == "w:r" {
                                                    if let Some(sz) = r.child("w:rPr").and_then(|rp| rp.child("w:sz")).and_then(|s| s.get_attr("w:val")).and_then(|v| v.parse::<f64>().ok()) {
                                                        if !(21.0..=27.0).contains(&sz) {
                                                            let r_pr = r.ensure_child_first("w:rPr");
                                                            r_pr.ensure_child_ordered("w:sz", R_PR_ORDER).set_attr("w:val", "24");
                                                            r_pr.ensure_child_ordered("w:szCs", R_PR_ORDER).set_attr("w:val", "24");
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
            }
        }
    }
    "Times New Roman (11–13pt) & firstLine = 0 trong ô bảng".to_string()
}

fn ensure_black_heading_styles_in_pkg(pkg: &mut OpenXmlPackage) -> Result<(), String> {
    let raw = pkg.get_xml("word/styles.xml").unwrap_or_else(|| {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"></w:styles>"#.to_string()
    });
    let mut doc = XmlDocument::parse(&raw)?;
    let configs = [
        ("Heading1", "heading 1", 0, "28", "center"),
        ("Heading2", "heading 2", 1, "28", "left"),
        ("Heading3", "heading 3", 2, "28", "left"),
        ("Heading4", "heading 4", 3, "26", "left"),
    ];
    for &(id, name, lvl, hp, jc_val) in &configs {
        let exists = doc
            .root
            .children_elements()
            .any(|e| e.name == "w:style" && e.get_attr("w:styleId") == Some(id));
        if exists {
            for node in &mut doc.root.children {
                if let XmlNode::Element(st) = node {
                    if st.name == "w:style" && st.get_attr("w:styleId") == Some(id) {
                        let r_pr = st.ensure_child_ordered("w:rPr", &["w:name", "w:basedOn", "w:next", "w:pPr", "w:rPr"]);
                        set_run_font_and_size(
                            r_pr,
                            "Times New Roman",
                            Some(hp.parse::<f64>().unwrap_or(28.0) / 2.0),
                        );
                        let col = r_pr.ensure_child_ordered("w:color", R_PR_ORDER);
                        col.set_attr("w:val", "000000");
                        col.remove_attr("w:themeColor");
                    }
                }
            }
        } else {
            let st_xml = format!(
                r#"<w:style w:type="paragraph" w:styleId="{}"><w:name w:val="{}"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="{}"/><w:outlineLvl w:val="{}"/></w:pPr><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:b/><w:color w:val="000000"/><w:sz w:val="{}"/><w:szCs w:val="{}"/></w:rPr></w:style>"#,
                id, name, jc_val, lvl, hp, hp
            );
            if let Ok(parsed) = XmlDocument::parse(&st_xml) {
                doc.root.children.push(XmlNode::Element(parsed.root));
            }
        }
    }
    pkg.set_xml("word/styles.xml", doc.to_xml_string()?);
    Ok(())
}

fn apply_nd30_page_number_header(
    pkg: &mut OpenXmlPackage,
    doc: &mut XmlDocument,
) -> Result<String, String> {
    let hdr_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p>
    <w:pPr><w:jc w:val="center"/><w:spacing w:before="0" w:after="0"/></w:pPr>
    <w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:color w:val="000000"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr><w:fldChar w:fldCharType="begin"/></w:r>
    <w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:color w:val="000000"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>
    <w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:color w:val="000000"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr><w:fldChar w:fldCharType="separate"/></w:r>
    <w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:color w:val="000000"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr><w:t>1</w:t></w:r>
    <w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:color w:val="000000"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr><w:fldChar w:fldCharType="end"/></w:r>
  </w:p>
</w:hdr>"#.to_string();

    pkg.set_xml("word/header_std_pg.xml", hdr_xml);
    ensure_content_type_override(
        pkg,
        "/word/header_std_pg.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
    )?;
    let rid = ensure_doc_relationship(
        pkg,
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header",
        "header_std_pg.xml",
    )?;
    doc.root.set_attr(
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    );

    if let Some(body) = doc.root.child_mut("w:body") {
        let attach_hdr = |sect: &mut XmlElement, is_first_sect: bool| {
            sect.children.retain(|n| {
                !matches!(n, XmlNode::Element(e) if e.name == "w:headerReference" && e.get_attr("w:type") == Some("default"))
            });
            let mut hr = XmlElement::new("w:headerReference");
            hr.set_attr("w:type", "default");
            hr.set_attr("r:id", &rid);
            sect.insert_child_ordered(hr, SECT_PR_ORDER);
            if is_first_sect {
                sect.ensure_child_ordered("w:titlePg", SECT_PR_ORDER);
            }
        };

        let mut sect_idx = 0usize;
        for node in &mut body.children {
            if let XmlNode::Element(el) = node {
                if el.name == "w:p" {
                    if let Some(sect) = el.child_mut("w:pPr").and_then(|pp| pp.child_mut("w:sectPr")) {
                        attach_hdr(sect, sect_idx == 0);
                        sect_idx += 1;
                    }
                } else if el.name == "w:sectPr" {
                    attach_hdr(el, sect_idx == 0);
                    sect_idx += 1;
                }
            }
        }
    }

    // Also update existing header PAGE font size if present while leaving footer text intact
    let header_names: Vec<String> = pkg
        .entries
        .iter()
        .filter(|e| e.name.to_ascii_lowercase().starts_with("word/header") && e.name.ends_with(".xml"))
        .map(|e| e.name.clone())
        .collect();
    for hn in header_names {
        if hn == "word/header_std_pg.xml" {
            continue;
        }
        if let Some(xml) = pkg.get_xml(&hn) {
            if let Ok(mut hd) = XmlDocument::parse(&xml) {
                for node in &mut hd.root.children {
                    if let XmlNode::Element(p) = node {
                        if p.name == "w:p" {
                            set_paragraph_font_preserving_anchors(p, "Times New Roman");
                            set_paragraph_size_preserving_anchors(p, 13.0);
                        }
                    }
                }
                if let Ok(s) = hd.to_xml_string() {
                    pkg.set_xml(&hn, s);
                }
            }
        }
    }

    Ok("Header canh giữa 13pt Times New Roman, ẩn số trang đầu (w:titlePg)".to_string())
}
