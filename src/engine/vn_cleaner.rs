use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::xml_helpers::{
    ensure_wt_space_preserve, set_run_font_and_size, XmlDocument, XmlElement, XmlNode,
};
use crate::models::{AuditIssue, AuditReport, AuditSeverity, CleanerConfig, QuoteStyle};
use regex::{Captures, Regex};
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Default, Clone)]
pub struct CleanStats {
    pub legacy_runs_converted: usize,
    pub soft_breaks_fixed: usize,
    pub empty_paragraphs_removed: usize,
    pub spaces_and_punct_fixed: usize,
    pub quotes_normalized: usize,
    pub dates_normalized: usize,
}

pub fn clean_vietnamese_document(
    pkg: &mut OpenXmlPackage,
    cfg: &CleanerConfig,
    logs: &mut Vec<String>,
) -> Result<CleanStats, String> {
    if !cfg.enabled || !pkg.is_word() {
        return Ok(CleanStats::default());
    }

    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Không tìm thấy word/document.xml".to_string())?;
    let mut doc = XmlDocument::parse(&doc_xml)?;
    let mut stats = CleanStats::default();

    if let Some(body) = doc.root.child_mut("w:body") {
        clean_container(body, cfg, &mut stats);
    }

    pkg.set_xml("word/document.xml", doc.to_xml_string()?);

    logs.push(format!(
        "Làm sạch tiếng Việt: chuyển mã {} run, {} ngắt dòng mềm, {} đoạn rỗng, {} lỗi khoảng trắng/dấu câu, {} ngoặc kép, {} ngày tháng NĐ30.",
        stats.legacy_runs_converted,
        stats.soft_breaks_fixed,
        stats.empty_paragraphs_removed,
        stats.spaces_and_punct_fixed,
        stats.quotes_normalized,
        stats.dates_normalized
    ));

    Ok(stats)
}

fn clean_container(container: &mut XmlElement, cfg: &CleanerConfig, stats: &mut CleanStats) {
    let is_body = container.name == "w:body";
    let mut in_signature_zone = false;
    let mut just_after_header_table = false;

    // Step 1: Recursively clean tables/nested containers and split soft breaks inside paragraphs
    // Store (node, preserve_if_empty)
    let mut new_children: Vec<(XmlNode, bool)> = Vec::with_capacity(container.children.len());
    for node in std::mem::take(&mut container.children) {
        match node {
            XmlNode::Element(mut el) => match el.name.as_str() {
                "w:p" => {
                    let text = el.extract_text();
                    let trimmed = text.trim();
                    let has_tabs = el
                        .child("w:pPr")
                        .and_then(|pp| pp.child("w:tabs"))
                        .is_some();

                    if is_body {
                        if crate::engine::formatter::is_signature_trigger_paragraph(&el, trimmed) {
                            in_signature_zone = true;
                        } else if in_signature_zone {
                            let still_sig =
                                !crate::engine::formatter::starts_with_heading_numbering(trimmed)
                                    && (trimmed.is_empty()
                                        || trimmed.starts_with('-')
                                        || trimmed.starts_with('+')
                                        || trimmed.chars().count() <= 60
                                        || has_tabs);
                            if !still_sig {
                                in_signature_zone = false;
                            }
                        }
                    }

                    let preserve_empty =
                        in_signature_zone || (just_after_header_table && trimmed.is_empty());
                    if !trimmed.is_empty() {
                        just_after_header_table = false;
                    }

                    if cfg.convert_legacy_encoding {
                        convert_legacy_runs_in_element(&mut el, stats);
                    }
                    clean_paragraph_text(&mut el, cfg, stats);
                    if cfg.fix_soft_breaks {
                        let split = split_soft_breaks_in_paragraph(el, stats);
                        for p in split {
                            new_children.push((XmlNode::Element(p), preserve_empty));
                        }
                    } else {
                        new_children.push((XmlNode::Element(el), preserve_empty));
                    }
                }
                "w:tbl" => {
                    in_signature_zone = false;
                    let is_admin_tbl = crate::engine::formatter::is_admin_layout_table(&el);
                    just_after_header_table = crate::engine::formatter::is_admin_header_table(&el);
                    if is_admin_tbl {
                        if cfg.convert_legacy_encoding {
                            convert_legacy_runs_in_table(&mut el, stats);
                        }
                    } else {
                        for row_node in &mut el.children {
                            if let XmlNode::Element(tr) = row_node {
                                if tr.name == "w:tr" {
                                    for cell_node in &mut tr.children {
                                        if let XmlNode::Element(tc) = cell_node {
                                            if tc.name == "w:tc" {
                                                clean_container(tc, cfg, stats);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    new_children.push((XmlNode::Element(el), true));
                }
                "w:sdt" | "w:sdtContent" | "w:txbxContent" => {
                    clean_container(&mut el, cfg, stats);
                    new_children.push((XmlNode::Element(el), false));
                }
                _ => new_children.push((XmlNode::Element(el), false)),
            },
            other => new_children.push((other, false)),
        }
    }

    // Step 2: Remove redundant empty paragraphs (preserving drawings/breaks/sectPr/signatures)
    if cfg.remove_empty_paragraphs {
        let mut filtered: Vec<XmlNode> = Vec::with_capacity(new_children.len());
        let is_table_cell = container.name == "w:tc";
        let mut para_count_in_cell = 0usize;
        if is_table_cell {
            para_count_in_cell = new_children
                .iter()
                .filter(|(n, _)| matches!(n, XmlNode::Element(e) if e.name == "w:p"))
                .count();
        }
        let mut kept_paras = 0usize;
        let total_nodes = new_children.len();
        for (idx, (node, preserve_empty)) in new_children.into_iter().enumerate() {
            if let XmlNode::Element(ref p) = node {
                if p.name == "w:p" && !preserve_empty && is_removable_empty_paragraph(p) {
                    // Word requires at least one <w:p> inside a <w:tc>
                    let is_last_para_in_cell =
                        is_table_cell && kept_paras == 0 && idx + 1 >= total_nodes;
                    if is_table_cell && para_count_in_cell <= 1 {
                        kept_paras += 1;
                        filtered.push(node);
                        continue;
                    }
                    if is_last_para_in_cell {
                        kept_paras += 1;
                        filtered.push(node);
                        continue;
                    }
                    if is_table_cell {
                        para_count_in_cell = para_count_in_cell.saturating_sub(1);
                    }
                    stats.empty_paragraphs_removed += 1;
                    continue;
                }
                if p.name == "w:p" {
                    kept_paras += 1;
                }
            }
            filtered.push(node);
        }
        container.children = filtered;
    } else {
        container.children = new_children.into_iter().map(|(n, _)| n).collect();
    }
}

fn convert_legacy_runs_in_table(tbl: &mut XmlElement, stats: &mut CleanStats) {
    for row_node in &mut tbl.children {
        if let XmlNode::Element(tr) = row_node {
            if tr.name == "w:tr" {
                for cell_node in &mut tr.children {
                    if let XmlNode::Element(tc) = cell_node {
                        if tc.name == "w:tc" {
                            for p_node in &mut tc.children {
                                if let XmlNode::Element(p) = p_node {
                                    if p.name == "w:p" {
                                        convert_legacy_runs_in_element(p, stats);
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

fn is_removable_empty_paragraph(p: &XmlElement) -> bool {
    if !p.extract_text().trim().is_empty() {
        return false;
    }
    // Keep if paragraph contains section break, drawing, page break, object, math, or field
    !has_structural_child(p)
}

fn has_structural_child(el: &XmlElement) -> bool {
    for node in &el.children {
        if let XmlNode::Element(sub) = node {
            match sub.name.as_str() {
                "w:sectPr" | "w:drawing" | "w:pict" | "w:object" | "m:oMath" | "m:oMathPara"
                | "w:fldChar" | "w:instrText" => return true,
                "w:br" => {
                    if let Some(t) = sub.get_attr("w:type") {
                        if t == "page" || t == "column" {
                            return true;
                        }
                    }
                }
                _ => {}
            }
            if has_structural_child(sub) {
                return true;
            }
        }
    }
    false
}

fn split_soft_breaks_in_paragraph(p: XmlElement, stats: &mut CleanStats) -> Vec<XmlElement> {
    if !contains_soft_break(&p) {
        return vec![p];
    }

    let p_pr = p.child("w:pPr").cloned();
    let mut result_paragraphs: Vec<XmlElement> = Vec::new();
    let mut current_p = XmlElement {
        name: "w:p".to_string(),
        attrs: p.attrs.clone(),
        children: Vec::new(),
    };
    if let Some(ref pr) = p_pr {
        current_p.children.push(XmlNode::Element(pr.clone()));
    }

    for node in p.children {
        match node {
            XmlNode::Element(el) if el.name == "w:pPr" => {}
            XmlNode::Element(run) if run.name == "w:r" && contains_direct_soft_break(&run) => {
                let r_pr = run.child("w:rPr").cloned();
                let mut pending_run = XmlElement {
                    name: "w:r".to_string(),
                    attrs: run.attrs.clone(),
                    children: r_pr.clone().into_iter().map(XmlNode::Element).collect(),
                };
                for r_child in run.children {
                    match r_child {
                        XmlNode::Element(ref b)
                            if b.name == "w:br"
                                && !matches!(
                                    b.get_attr("w:type"),
                                    Some("page") | Some("column")
                                ) =>
                        {
                            stats.soft_breaks_fixed += 1;
                            if pending_run
                                .children
                                .iter()
                                .any(|n| !matches!(n, XmlNode::Element(e) if e.name == "w:rPr"))
                            {
                                current_p.children.push(XmlNode::Element(pending_run));
                            }
                            result_paragraphs.push(current_p);

                            current_p = XmlElement {
                                name: "w:p".to_string(),
                                attrs: p.attrs.clone(),
                                children: p_pr
                                    .clone()
                                    .into_iter()
                                    .map(|mut pr| {
                                        pr.remove_children_by_name("w:sectPr");
                                        XmlNode::Element(pr)
                                    })
                                    .collect(),
                            };
                            pending_run = XmlElement {
                                name: "w:r".to_string(),
                                attrs: run.attrs.clone(),
                                children: r_pr.clone().into_iter().map(XmlNode::Element).collect(),
                            };
                        }
                        XmlNode::Element(e) if e.name == "w:rPr" => {}
                        other => pending_run.children.push(other),
                    }
                }
                if pending_run
                    .children
                    .iter()
                    .any(|n| !matches!(n, XmlNode::Element(e) if e.name == "w:rPr"))
                {
                    current_p.children.push(XmlNode::Element(pending_run));
                }
            }
            other => current_p.children.push(other),
        }
    }

    result_paragraphs.push(current_p);
    result_paragraphs
}

fn contains_soft_break(p: &XmlElement) -> bool {
    p.children.iter().any(|n| match n {
        XmlNode::Element(r) if r.name == "w:r" => contains_direct_soft_break(r),
        _ => false,
    })
}

fn contains_direct_soft_break(r: &XmlElement) -> bool {
    r.children.iter().any(|n| match n {
        XmlNode::Element(b) if b.name == "w:br" => {
            !matches!(b.get_attr("w:type"), Some("page") | Some("column"))
        }
        _ => false,
    })
}

fn convert_legacy_runs_in_element(el: &mut XmlElement, stats: &mut CleanStats) {
    for node in &mut el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        if child.name == "w:r" {
            let font_name = extract_run_font(child).unwrap_or_default();
            let lower = font_name.to_ascii_lowercase();
            let is_tcvn3 = lower.starts_with(".vn");
            let is_vni = lower.starts_with("vni-");
            if is_tcvn3 || is_vni {
                let all_caps = is_tcvn3 && lower.ends_with('h');
                let mut changed = false;
                for r_node in &mut child.children {
                    if let XmlNode::Element(t_el) = r_node {
                        if t_el.name == "w:t" {
                            let raw = t_el.extract_text();
                            let converted = if is_tcvn3 {
                                convert_tcvn3_to_unicode(&raw, all_caps)
                            } else {
                                convert_vni_to_unicode(&raw)
                            };
                            if converted != raw {
                                ensure_wt_space_preserve(t_el, &converted);
                                changed = true;
                            }
                        }
                    }
                }
                let r_pr = child.ensure_child_first("w:rPr");
                set_run_font_and_size(r_pr, "Times New Roman", None);
                if changed {
                    stats.legacy_runs_converted += 1;
                }
            }
        } else if matches!(
            child.name.as_str(),
            "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent"
        ) {
            convert_legacy_runs_in_element(child, stats);
        }
    }
}

fn extract_run_font(run: &XmlElement) -> Option<String> {
    let r_fonts = run.child("w:rPr")?.child("w:rFonts")?;
    r_fonts
        .get_attr("w:ascii")
        .or_else(|| r_fonts.get_attr("w:hAnsi"))
        .or_else(|| r_fonts.get_attr("w:cs"))
        .map(|s| s.to_string())
}

fn clean_paragraph_text(p: &mut XmlElement, cfg: &CleanerConfig, stats: &mut CleanStats) {
    // Collect mutable references to every <w:t> text inside the paragraph runs (skipping instrText)
    let mut text_segments: Vec<(Vec<usize>, String)> = Vec::new();
    collect_wt_segments(p, &mut Vec::new(), &mut text_segments);
    if text_segments.is_empty() {
        return;
    }

    // Track quote parity across the whole paragraph
    let mut quote_open = true;

    let re_space_before_punct = re_space_before_punct();
    let re_missing_space_after = re_missing_space_after();
    let re_nd30_date = re_nd30_dates();

    for idx in 0..text_segments.len() {
        let prev_text = if idx > 0 {
            text_segments[idx - 1].1.as_str()
        } else {
            ""
        };
        let next_text = if idx + 1 < text_segments.len() {
            text_segments[idx + 1].1.as_str()
        } else {
            ""
        };
        let original = text_segments[idx].1.clone();

        // Keep dedicated whitespace fill-in-the-blank runs (e.g., "           " between ngày/tháng/năm)
        if original.len() >= 2 && original.chars().all(|c| c == ' ') {
            continue;
        }

        let mut s = original.clone();

        if cfg.fix_multiple_spaces {
            let replaced = collapse_spaces_preserving_blanks(&s, prev_text, next_text);
            if replaced != s {
                stats.spaces_and_punct_fixed += 1;
                s = replaced;
            }
        }

        if cfg.fix_punctuation_spacing {
            let s1 = re_space_before_punct.replace_all(&s, "$1").into_owned();
            let s2 = re_missing_space_after.replace_all(&s1, "$1 $2").into_owned();
            if s2 != s {
                stats.spaces_and_punct_fixed += 1;
                s = s2;
            }
        }

        if cfg.quote_style != QuoteStyle::Keep {
            let quoted = normalize_quotes(&s, cfg.quote_style, &mut quote_open);
            if quoted != s {
                stats.quotes_normalized += 1;
                s = quoted;
            }
        }

        if cfg.normalize_nd30_dates {
            let dated = normalize_dates_in_str(&s, re_nd30_date, &mut stats.dates_normalized);
            s = dated;
        }

        if s != original {
            text_segments[idx].1 = s;
        }
    }

    // Trim leading spaces at paragraph start and trailing spaces at paragraph end
    // unless it is a fill-in-the-blank field.
    if cfg.fix_multiple_spaces {
        if let Some((_, first)) = text_segments.first_mut() {
            if !(first.len() >= 2 && first.chars().all(|c| c == ' ')) {
                let trimmed = first.trim_start_matches(' ').to_string();
                if trimmed != *first {
                    *first = trimmed;
                    stats.spaces_and_punct_fixed += 1;
                }
            }
        }
        if let Some((_, last)) = text_segments.last_mut() {
            if !(last.len() >= 2 && last.chars().all(|c| c == ' ')) {
                let trimmed = last.trim_end_matches(' ').to_string();
                if trimmed != *last {
                    *last = trimmed;
                    stats.spaces_and_punct_fixed += 1;
                }
            }
        }
    }

    // Write back updated text segments
    for (path, seg) in text_segments {
        if let Some(el) = get_element_by_path_mut(p, &path) {
            ensure_wt_space_preserve(el, &seg);
        }
    }
}

fn collapse_spaces_preserving_blanks(input: &str, prev_seg: &str, next_seg: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] != ' ' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        let count = i - start;
        if count == 1 {
            out.push(' ');
            continue;
        }

        // Preserve intentional >= 3-space blanks for Số: / ngày / tháng / năm / '/' fields
        let before_str: String = if start > 0 {
            chars[..start].iter().collect()
        } else {
            prev_seg.to_string()
        };
        let after_str: String = if i < chars.len() {
            chars[i..].iter().collect()
        } else {
            next_seg.to_string()
        };
        let before_low = before_str.trim_end().to_lowercase();
        let after_low = after_str.trim_start().to_lowercase();

        let is_blank_field = count >= 3
            && (before_low.ends_with("số:")
                || before_low.ends_with("số")
                || before_low.ends_with("ngày")
                || before_low.ends_with("tháng")
                || before_low.ends_with('/')
                || after_low.starts_with('/')
                || after_low.starts_with("tháng")
                || after_low.starts_with("năm"));

        if is_blank_field {
            for _ in 0..count {
                out.push(' ');
            }
        } else {
            out.push(' ');
        }
    }
    out
}

fn collect_wt_segments(
    el: &XmlElement,
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, String)>,
) {
    for (idx, node) in el.children.iter().enumerate() {
        if let XmlNode::Element(sub) = node {
            path.push(idx);
            if sub.name == "w:t" {
                out.push((path.clone(), sub.extract_text()));
            } else if matches!(
                sub.name.as_str(),
                "w:r" | "w:hyperlink" | "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent"
            ) {
                collect_wt_segments(sub, path, out);
            }
            path.pop();
        }
    }
}

fn get_element_by_path_mut<'a>(
    mut el: &'a mut XmlElement,
    path: &[usize],
) -> Option<&'a mut XmlElement> {
    for &idx in path {
        match el.children.get_mut(idx)? {
            XmlNode::Element(next) => el = next,
            _ => return None,
        }
    }
    Some(el)
}

fn normalize_quotes(input: &str, style: QuoteStyle, open: &mut bool) -> String {
    if !input.contains('"') && !input.contains('“') && !input.contains('”') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match style {
            QuoteStyle::CurlyNd30 => match ch {
                '"' => {
                    if *open {
                        out.push('“');
                    } else {
                        out.push('”');
                    }
                    *open = !*open;
                }
                '“' => {
                    out.push('“');
                    *open = false;
                }
                '”' => {
                    out.push('”');
                    *open = true;
                }
                _ => out.push(ch),
            },
            QuoteStyle::StraightDang => match ch {
                '“' | '”' | '"' => out.push('"'),
                _ => out.push(ch),
            },
            QuoteStyle::Keep => out.push(ch),
        }
    }
    out
}

fn normalize_dates_in_str(input: &str, re: &Regex, count: &mut usize) -> String {
    re.replace_all(input, |caps: &Captures| {
        let prefix = &caps[1];
        let day: u32 = caps[2].parse().unwrap_or(1);
        let mid = &caps[3];
        let month: u32 = caps[4].parse().unwrap_or(1);
        let suffix = &caps[5];

        let formatted_day = if (1..=9).contains(&day) {
            format!("0{}", day)
        } else {
            day.to_string()
        };
        let formatted_month = if month == 1 || month == 2 {
            format!("0{}", month)
        } else {
            month.to_string()
        };

        let result = format!(
            "{}{}{}{}{}",
            prefix, formatted_day, mid, formatted_month, suffix
        );
        if result != caps[0] {
            *count += 1;
        }
        result
    })
    .into_owned()
}

pub fn convert_tcvn3_to_unicode(input: &str, all_caps: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let mapped = match ch {
            '\u{00A8}' => 'ă',
            '\u{00A1}' => 'Ă',
            '\u{00A9}' => 'â',
            '\u{00A2}' => 'Â',
            '\u{00AE}' => 'đ',
            '\u{00A7}' => 'Đ',
            '\u{00AA}' => 'ê',
            '\u{00A3}' => 'Ê',
            '\u{00AB}' => 'ô',
            '\u{00A4}' => 'Ô',
            '\u{00AC}' => 'ơ',
            '\u{00A5}' => 'Ơ',
            '\u{00AD}' => 'ư',
            '\u{00A6}' => 'Ư',
            '\u{00B5}' => 'à',
            '\u{00B6}' => 'ả',
            '\u{00B7}' => 'ã',
            '\u{00B8}' => 'á',
            '\u{00B9}' => 'ạ',
            '\u{00BB}' => 'ằ',
            '\u{00BC}' => 'ẳ',
            '\u{00BD}' => 'ẵ',
            '\u{00BE}' => 'ắ',
            '\u{00C6}' => 'ặ',
            '\u{00C7}' => 'ầ',
            '\u{00C8}' => 'ẩ',
            '\u{00C9}' => 'ẫ',
            '\u{00CA}' => 'ấ',
            '\u{00CB}' => 'ậ',
            '\u{00CC}' => 'è',
            '\u{00CE}' => 'ẻ',
            '\u{00CF}' => 'ẽ',
            '\u{00D0}' => 'é',
            '\u{00D1}' => 'ẹ',
            '\u{00D2}' => 'ề',
            '\u{00D3}' => 'ể',
            '\u{00D4}' => 'ễ',
            '\u{00D5}' => 'ế',
            '\u{00D6}' => 'ệ',
            '\u{00D7}' => 'ì',
            '\u{00D8}' => 'ỉ',
            '\u{00DC}' => 'ĩ',
            '\u{00DD}' => 'í',
            '\u{00DE}' => 'ị',
            '\u{00DF}' => 'ò',
            '\u{00E1}' => 'ỏ',
            '\u{00E2}' => 'õ',
            '\u{00E3}' => 'ó',
            '\u{00E4}' => 'ọ',
            '\u{00E5}' => 'ồ',
            '\u{00E6}' => 'ổ',
            '\u{00E7}' => 'ỗ',
            '\u{00E8}' => 'ố',
            '\u{00E9}' => 'ộ',
            '\u{00EA}' => 'ờ',
            '\u{00EB}' => 'ở',
            '\u{00EC}' => 'ỡ',
            '\u{00ED}' => 'ớ',
            '\u{00EE}' => 'ợ',
            '\u{00EF}' => 'ù',
            '\u{00F1}' => 'ủ',
            '\u{00F2}' => 'ũ',
            '\u{00F3}' => 'ú',
            '\u{00F4}' => 'ụ',
            '\u{00F5}' => 'ừ',
            '\u{00F6}' => 'ử',
            '\u{00F7}' => 'ữ',
            '\u{00F8}' => 'ứ',
            '\u{00F9}' => 'ự',
            '\u{00FA}' => 'ỳ',
            '\u{00FB}' => 'ỷ',
            '\u{00FC}' => 'ỹ',
            '\u{00FD}' => 'ý',
            '\u{00FE}' => 'ỵ',
            other => other,
        };
        if all_caps {
            for up in mapped.to_uppercase() {
                out.push(up);
            }
        } else {
            out.push(mapped);
        }
    }
    out
}

pub fn convert_vni_to_unicode(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < chars.len() {
        let cur = chars[i];
        let next = chars.get(i + 1).copied();

        if let Some(n) = next {
            let pair: Option<char> = match (cur, n) {
                // a + tone
                ('a', '\u{00F9}') => Some('á'),
                ('a', '\u{00F8}') => Some('à'),
                ('a', '\u{00FB}') => Some('ả'),
                ('a', '\u{00F5}') => Some('ã'),
                ('a', '\u{00EF}') => Some('ạ'),
                ('A', '\u{00D9}') => Some('Á'),
                ('A', '\u{00D8}') => Some('À'),
                ('A', '\u{00DB}') => Some('Ả'),
                ('A', '\u{00D5}') => Some('Ã'),
                ('A', '\u{00CF}') => Some('Ạ'),

                // â + tone
                ('a', '\u{00E2}') => Some('â'),
                ('a', '\u{00E1}') => Some('ấ'),
                ('a', '\u{00E0}') => Some('ầ'),
                ('a', '\u{00E5}') => Some('ẩ'),
                ('a', '\u{00E3}') => Some('ẫ'),
                ('a', '\u{00E4}') => Some('ậ'),
                ('A', '\u{00C2}') => Some('Â'),
                ('A', '\u{00C1}') => Some('Ấ'),
                ('A', '\u{00C0}') => Some('Ầ'),
                ('A', '\u{00C5}') => Some('Ẩ'),
                ('A', '\u{00C3}') => Some('Ẫ'),
                ('A', '\u{00C4}') => Some('Ậ'),

                // ă + tone
                ('a', '\u{00EA}') => Some('ă'),
                ('a', '\u{00E9}') => Some('ắ'),
                ('a', '\u{00E8}') => Some('ằ'),
                ('a', '\u{00FA}') => Some('ẳ'),
                ('a', '\u{00FC}') => Some('ẵ'),
                ('a', '\u{00EB}') => Some('ặ'),
                ('A', '\u{00CA}') => Some('Ă'),
                ('A', '\u{00C9}') => Some('Ắ'),
                ('A', '\u{00C8}') => Some('Ằ'),
                ('A', '\u{00DA}') => Some('Ẳ'),
                ('A', '\u{00DC}') => Some('Ẵ'),
                ('A', '\u{00CB}') => Some('Ặ'),

                // e + tone
                ('e', '\u{00F9}') => Some('é'),
                ('e', '\u{00F8}') => Some('è'),
                ('e', '\u{00FB}') => Some('ẻ'),
                ('e', '\u{00F5}') => Some('ẽ'),
                ('e', '\u{00EF}') => Some('ẹ'),
                ('E', '\u{00D9}') => Some('É'),
                ('E', '\u{00D8}') => Some('È'),
                ('E', '\u{00DB}') => Some('Ẻ'),
                ('E', '\u{00D5}') => Some('Ẽ'),
                ('E', '\u{00CF}') => Some('Ẹ'),

                // ê + tone
                ('e', '\u{00E2}') => Some('ê'),
                ('e', '\u{00E1}') => Some('ế'),
                ('e', '\u{00E0}') => Some('ề'),
                ('e', '\u{00E5}') => Some('ể'),
                ('e', '\u{00E3}') => Some('ễ'),
                ('e', '\u{00E4}') => Some('ệ'),
                ('E', '\u{00C2}') => Some('Ê'),
                ('E', '\u{00C1}') => Some('Ế'),
                ('E', '\u{00C0}') => Some('Ề'),
                ('E', '\u{00C5}') => Some('Ể'),
                ('E', '\u{00C3}') => Some('Ễ'),
                ('E', '\u{00C4}') => Some('Ệ'),

                // o + tone
                ('o', '\u{00F9}') => Some('ó'),
                ('o', '\u{00F8}') => Some('ò'),
                ('o', '\u{00FB}') => Some('ỏ'),
                ('o', '\u{00F5}') => Some('õ'),
                ('o', '\u{00EF}') => Some('ọ'),
                ('O', '\u{00D9}') => Some('Ó'),
                ('O', '\u{00D8}') => Some('Ò'),
                ('O', '\u{00DB}') => Some('Ỏ'),
                ('O', '\u{00D5}') => Some('Õ'),
                ('O', '\u{00CF}') => Some('Ọ'),

                // ô + tone
                ('o', '\u{00E2}') => Some('ô'),
                ('o', '\u{00E1}') => Some('ố'),
                ('o', '\u{00E0}') => Some('ồ'),
                ('o', '\u{00E5}') => Some('ổ'),
                ('o', '\u{00E3}') => Some('ỗ'),
                ('o', '\u{00E4}') => Some('ộ'),
                ('O', '\u{00C2}') => Some('Ô'),
                ('O', '\u{00C1}') => Some('Ố'),
                ('O', '\u{00C0}') => Some('Ồ'),
                ('O', '\u{00C5}') => Some('Ổ'),
                ('O', '\u{00C3}') => Some('Ỗ'),
                ('O', '\u{00C4}') => Some('Ộ'),

                // ơ (encoded as 'ô') + tone
                ('\u{00F4}', '\u{00F9}') => Some('ớ'),
                ('\u{00F4}', '\u{00F8}') => Some('ờ'),
                ('\u{00F4}', '\u{00FB}') => Some('ở'),
                ('\u{00F4}', '\u{00F5}') => Some('ỡ'),
                ('\u{00F4}', '\u{00EF}') => Some('ợ'),
                ('\u{00D4}', '\u{00D9}') => Some('Ớ'),
                ('\u{00D4}', '\u{00D8}') => Some('Ờ'),
                ('\u{00D4}', '\u{00DB}') => Some('Ở'),
                ('\u{00D4}', '\u{00D5}') => Some('Ỡ'),
                ('\u{00D4}', '\u{00CF}') => Some('Ợ'),

                // u + tone
                ('u', '\u{00F9}') => Some('ú'),
                ('u', '\u{00F8}') => Some('ù'),
                ('u', '\u{00FB}') => Some('ủ'),
                ('u', '\u{00F5}') => Some('ũ'),
                ('u', '\u{00EF}') => Some('ụ'),
                ('U', '\u{00D9}') => Some('Ú'),
                ('U', '\u{00D8}') => Some('Ù'),
                ('U', '\u{00DB}') => Some('Ủ'),
                ('U', '\u{00D5}') => Some('Ũ'),
                ('U', '\u{00CF}') => Some('Ụ'),

                // ư (encoded as 'ö') + tone
                ('\u{00F6}', '\u{00F9}') => Some('ứ'),
                ('\u{00F6}', '\u{00F8}') => Some('ừ'),
                ('\u{00F6}', '\u{00FB}') => Some('ử'),
                ('\u{00F6}', '\u{00F5}') => Some('ữ'),
                ('\u{00F6}', '\u{00EF}') => Some('ự'),
                ('\u{00D6}', '\u{00D9}') => Some('Ứ'),
                ('\u{00D6}', '\u{00D8}') => Some('Ừ'),
                ('\u{00D6}', '\u{00DB}') => Some('Ử'),
                ('\u{00D6}', '\u{00D5}') => Some('Ữ'),
                ('\u{00D6}', '\u{00CF}') => Some('Ự'),

                // y + tone
                ('y', '\u{00F9}') => Some('ý'),
                ('y', '\u{00F8}') => Some('ỳ'),
                ('y', '\u{00FB}') => Some('ỷ'),
                ('y', '\u{00F5}') => Some('ỹ'),
                ('y', '\u{00EF}') => Some('ỵ'),
                ('Y', '\u{00D9}') => Some('Ý'),
                ('Y', '\u{00D8}') => Some('Ỳ'),
                ('Y', '\u{00DB}') => Some('Ỷ'),
                ('Y', '\u{00D5}') => Some('Ỹ'),
                ('Y', '\u{00CF}') => Some('Ỵ'),

                _ => None,
            };

            if let Some(ch) = pair {
                out.push(ch);
                i += 2;
                continue;
            }
        }

        let single = match cur {
            '\u{00F1}' => 'đ',
            '\u{00D1}' => 'Đ',
            '\u{00F4}' => 'ơ',
            '\u{00D4}' => 'Ơ',
            '\u{00F6}' => 'ư',
            '\u{00D6}' => 'Ư',
            '\u{00ED}' => 'í',
            '\u{00EC}' => 'ì',
            '\u{00E6}' => 'ỉ',
            '\u{00F3}' => 'ĩ',
            '\u{00F2}' => 'ị',
            '\u{00CD}' => 'Í',
            '\u{00CC}' => 'Ì',
            '\u{00C6}' => 'Ỉ',
            '\u{00D3}' => 'Ĩ',
            '\u{00D2}' => 'Ị',
            '\u{00EE}' => 'ỵ',
            '\u{00CE}' => 'Ỵ',
            other => other,
        };
        out.push(single);
        i += 1;
    }
    out
}

pub fn audit_package(path: &Path, pkg: &OpenXmlPackage) -> AuditReport {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let file_type = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let has_macros = pkg.has_entry("word/vbaProject.bin")
        || pkg.has_entry("xl/vbaProject.bin")
        || pkg.has_entry("word/vbaData.xml");

    let mut is_protected = false;
    if let Some(settings) = pkg.get_xml("word/settings.xml") {
        if settings.contains("w:documentProtection") {
            is_protected = true;
        }
    }

    let mut issues: Vec<AuditIssue> = Vec::new();
    let mut section_count = 0usize;
    let mut table_count = 0usize;
    let mut paragraph_count = 0usize;
    let mut has_legacy_encoding = false;

    if pkg.is_word() {
        if let Some(doc_xml) = pkg.get_xml("word/document.xml") {
            if let Ok(doc) = XmlDocument::parse(&doc_xml) {
                let mut non_tnr_runs = 0usize;
                let mut legacy_runs = 0usize;
                let mut multi_space_count = 0usize;
                let mut punct_error_count = 0usize;
                let mut soft_break_count = 0usize;
                let mut empty_para_count = 0usize;
                let mut unoptimized_tables = 0usize;
                let mut bad_margin_sections = 0usize;

                if let Some(body) = doc.root.child("w:body") {
                    audit_element(
                        body,
                        &mut paragraph_count,
                        &mut table_count,
                        &mut section_count,
                        &mut non_tnr_runs,
                        &mut legacy_runs,
                        &mut multi_space_count,
                        &mut punct_error_count,
                        &mut soft_break_count,
                        &mut empty_para_count,
                        &mut unoptimized_tables,
                        &mut bad_margin_sections,
                    );
                }

                has_legacy_encoding = legacy_runs > 0;

                if legacy_runs > 0 {
                    issues.push(AuditIssue {
                        category: "Bảng mã cũ".to_string(),
                        severity: AuditSeverity::Error,
                        message: "Phát hiện đoạn chữ dùng bảng mã TCVN3 (.Vn*) hoặc VNI cũ.".to_string(),
                        count: legacy_runs,
                    });
                }
                if non_tnr_runs > 0 {
                    issues.push(AuditIssue {
                        category: "Font chữ".to_string(),
                        severity: AuditSeverity::Warning,
                        message: "Có đoạn văn bản chưa dùng font chuẩn Times New Roman.".to_string(),
                        count: non_tnr_runs,
                    });
                }
                if bad_margin_sections > 0 {
                    issues.push(AuditIssue {
                        category: "Căn lề trang".to_string(),
                        severity: AuditSeverity::Warning,
                        message: "Lề trang khác chuẩn NĐ30 (Trên 2 - Dưới 2 - Trái 3 - Phải 2 cm).".to_string(),
                        count: bad_margin_sections,
                    });
                }
                if multi_space_count > 0 || punct_error_count > 0 {
                    issues.push(AuditIssue {
                        category: "Chính tả trình bày".to_string(),
                        severity: AuditSeverity::Warning,
                        message: "Có lỗi khoảng trắng kép hoặc khoảng trắng trước/sau dấu câu.".to_string(),
                        count: multi_space_count + punct_error_count,
                    });
                }
                if soft_break_count > 0 {
                    issues.push(AuditIssue {
                        category: "Ngắt dòng".to_string(),
                        severity: AuditSeverity::Info,
                        message: "Có ngắt dòng mềm (Shift+Enter) dễ gây giãn chữ khi căn đều.".to_string(),
                        count: soft_break_count,
                    });
                }
                if empty_para_count > 2 {
                    issues.push(AuditIssue {
                        category: "Đoạn trống".to_string(),
                        severity: AuditSeverity::Info,
                        message: "Nhiều dòng Enter trống dư thừa.".to_string(),
                        count: empty_para_count,
                    });
                }
                if unoptimized_tables > 0 {
                    issues.push(AuditIssue {
                        category: "Bảng biểu".to_string(),
                        severity: AuditSeverity::Info,
                        message: "Bảng chưa bật chống tách đôi dòng (cantSplit) hoặc tự co vừa trang.".to_string(),
                        count: unoptimized_tables,
                    });
                }
            }
        }
    }

    if has_macros {
        issues.push(AuditIssue {
            category: "Bảo mật Macro".to_string(),
            severity: AuditSeverity::Error,
            message: "Tệp có chứa VBA Macro (vbaProject.bin) tiềm ẩn rủi ro bảo mật.".to_string(),
            count: 1,
        });
    }
    if is_protected {
        issues.push(AuditIssue {
            category: "Khóa chỉnh sửa".to_string(),
            severity: AuditSeverity::Info,
            message: "Tệp đang bật chế độ khóa Restrict Editing.".to_string(),
            count: 1,
        });
    }

    if let Some(core_xml) = pkg.get_xml("docProps/core.xml") {
        if core_xml.contains("<dc:creator>") && !core_xml.contains("<dc:creator/>") {
            if let Ok(core_doc) = XmlDocument::parse(&core_xml) {
                if let Some(creator) = core_doc.root.child("dc:creator") {
                    let name = creator.extract_text();
                    if !name.trim().is_empty() && name.trim() != "Admin" {
                        issues.push(AuditIssue {
                            category: "Metadata ẩn".to_string(),
                            severity: AuditSeverity::Info,
                            message: format!("Tệp lưu tên tác giả cá nhân trong Metadata: '{}'.", name.trim()),
                            count: 1,
                        });
                    }
                }
            }
        }
    }

    let mut penalty: i32 = 0;
    for issue in &issues {
        penalty += match issue.severity {
            AuditSeverity::Error => 18,
            AuditSeverity::Warning => 10,
            AuditSeverity::Info => 4,
        };
    }
    let score = (100 - penalty).clamp(15, 100) as u8;

    AuditReport {
        file_path: path.to_string_lossy().to_string(),
        file_name,
        file_type,
        score,
        section_count: section_count.max(1),
        table_count,
        paragraph_count,
        has_macros,
        is_protected,
        has_legacy_encoding,
        issues,
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_element(
    el: &XmlElement,
    paragraph_count: &mut usize,
    table_count: &mut usize,
    section_count: &mut usize,
    non_tnr_runs: &mut usize,
    legacy_runs: &mut usize,
    multi_space_count: &mut usize,
    punct_error_count: &mut usize,
    soft_break_count: &mut usize,
    empty_para_count: &mut usize,
    unoptimized_tables: &mut usize,
    bad_margin_sections: &mut usize,
) {
    let re_multi = re_multi_spaces();
    let re_punct = re_space_before_punct();

    for node in &el.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        match child.name.as_str() {
            "w:sectPr" => {
                *section_count += 1;
                if is_margin_off_nd30(child) {
                    *bad_margin_sections += 1;
                }
            }
            "w:p" => {
                *paragraph_count += 1;
                if let Some(sect) = child.child("w:pPr").and_then(|pp| pp.child("w:sectPr")) {
                    *section_count += 1;
                    if is_margin_off_nd30(sect) {
                        *bad_margin_sections += 1;
                    }
                }
                let text = child.extract_text();
                if text.trim().is_empty() && !has_structural_child(child) {
                    *empty_para_count += 1;
                } else {
                    if re_multi.is_match(&text) {
                        *multi_space_count += 1;
                    }
                    if re_punct.is_match(&text) {
                        *punct_error_count += 1;
                    }
                }
                if contains_soft_break(child) {
                    *soft_break_count += 1;
                }
                for r_node in &child.children {
                    if let XmlNode::Element(r) = r_node {
                        if r.name == "w:r" {
                            if let Some(f) = extract_run_font(r) {
                                let fl = f.to_ascii_lowercase();
                                if fl.starts_with(".vn") || fl.starts_with("vni-") {
                                    *legacy_runs += 1;
                                } else if !fl.is_empty() && fl != "times new roman" {
                                    *non_tnr_runs += 1;
                                }
                            }
                        }
                    }
                }
            }
            "w:tbl" => {
                *table_count += 1;
                if !crate::engine::formatter::is_admin_layout_table(child) {
                    let has_cant_split = child
                        .children_elements()
                        .filter(|tr| tr.name == "w:tr")
                        .enumerate()
                        .all(|(r_idx, tr)| {
                            let mut text_len = 0usize;
                            let mut max_c = 0usize;
                            let mut max_p = 0usize;
                            for tc in tr.children_elements().filter(|e| e.name == "w:tc") {
                                let c_len = tc.extract_text().chars().count();
                                text_len += c_len;
                                max_c = max_c.max(c_len);
                                max_p = max_p.max(
                                    tc.children_elements().filter(|e| e.name == "w:p").count(),
                                );
                            }
                            if r_idx == 0 || (max_p <= 1 && max_c <= 65 && text_len <= 130) {
                                tr.child("w:trPr")
                                    .and_then(|tp| tp.child("w:cantSplit"))
                                    .is_some()
                            } else {
                                true
                            }
                        });
                    if !has_cant_split {
                        *unoptimized_tables += 1;
                    }
                }
                for row in child.children_elements().filter(|tr| tr.name == "w:tr") {
                    for cell in row.children_elements().filter(|tc| tc.name == "w:tc") {
                        audit_element(
                            cell,
                            paragraph_count,
                            table_count,
                            section_count,
                            non_tnr_runs,
                            legacy_runs,
                            multi_space_count,
                            punct_error_count,
                            soft_break_count,
                            empty_para_count,
                            unoptimized_tables,
                            bad_margin_sections,
                        );
                    }
                }
            }
            "w:sdt" | "w:sdtContent" | "w:txbxContent" => {
                audit_element(
                    child,
                    paragraph_count,
                    table_count,
                    section_count,
                    non_tnr_runs,
                    legacy_runs,
                    multi_space_count,
                    punct_error_count,
                    soft_break_count,
                    empty_para_count,
                    unoptimized_tables,
                    bad_margin_sections,
                );
            }
            _ => {}
        }
    }
}

fn is_margin_off_nd30(sect_pr: &XmlElement) -> bool {
    let Some(mar) = sect_pr.child("w:pgMar") else {
        return true;
    };
    let top: i32 = mar.get_attr("w:top").and_then(|v| v.parse().ok()).unwrap_or(0);
    let bottom: i32 = mar.get_attr("w:bottom").and_then(|v| v.parse().ok()).unwrap_or(0);
    let left: i32 = mar.get_attr("w:left").and_then(|v| v.parse().ok()).unwrap_or(0);
    let right: i32 = mar.get_attr("w:right").and_then(|v| v.parse().ok()).unwrap_or(0);

    let is_landscape = sect_pr
        .child("w:pgSz")
        .and_then(|s| s.get_attr("w:orient"))
        .map(|o| o == "landscape")
        .unwrap_or(false);

    if is_landscape {
        // Landscape binding margin allows top~3cm (1701 twips) OR left~3cm
        let ok_binding = (top - 1701).abs() <= 120 || (left - 1701).abs() <= 120;
        !ok_binding
    } else {
        (top - 1134).abs() > 120
            || (bottom - 1134).abs() > 120
            || (left - 1701).abs() > 120
            || (right - 1134).abs() > 120
    }
}

fn re_multi_spaces() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r" {2,}").expect("valid regex"))
}

fn re_space_before_punct() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+([,.;:!?\)\]”])").expect("valid regex"))
}

fn re_missing_space_after() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Add space after comma/semicolon when directly followed by a letter (not numbers like 1,5)
    RE.get_or_init(|| Regex::new(r"([,;])(\p{L})").expect("valid regex"))
}

fn re_nd30_dates() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(ngày\s+)(\d{1,2})(\s+tháng\s+)(\d{1,2})(\s+năm\s+\d{4})")
            .expect("valid regex")
    })
}

fn re_draft_so_colon_blank() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bsố\s*:(\s{2,})/").expect("valid regex"))
}

fn re_draft_so_inline_blank() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bsố(\s{2,})/\d{4}").expect("valid regex"))
}

fn re_draft_ngay_thang_blank() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bngày(\s{2,})tháng\b").expect("valid regex"))
}

fn re_draft_thang_nam_blank() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\btháng(\s{2,})năm\b").expect("valid regex"))
}

fn re_draft_ngay_slash_blank() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bngày(\s{1,})/(\s{1,})/").expect("valid regex"))
}
