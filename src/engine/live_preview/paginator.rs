use super::cancel::CancelToken;
use super::extractor::ExtractedPreviewDoc;
use crate::engine::layout_intelligence::mapper::{round1, round2};
use crate::models::{
    PreviewBlock, PreviewPageModel, PreviewParagraphBlock, PreviewSectionSpec, PreviewTableBlock,
};
use std::collections::HashSet;

pub fn estimate_table_row_height_mm(row: &crate::models::PreviewTableRow) -> f64 {
    let max_cell_h = row
        .cells
        .iter()
        .filter(|c| !c.merged_continuation)
        .map(|c| {
            let p_sum: f64 = c.paragraphs.iter().map(|p| p.estimated_height_mm).sum();
            p_sum + 2.4
        })
        .fold(5.0f64, f64::max);
    round2(max_cell_h.max(row.min_height_mm))
}

pub fn estimate_table_height_mm(tbl: &PreviewTableBlock) -> f64 {
    round2(
        tbl.rows
            .iter()
            .map(estimate_table_row_height_mm)
            .sum::<f64>()
            .max(6.0)
            + 2.0,
    )
}

fn fill_hf_dynamic_spans(
    blocks: &[PreviewParagraphBlock],
    display_page: usize,
    total_pages: usize,
) -> Vec<PreviewParagraphBlock> {
    blocks
        .iter()
        .map(|b| {
            let mut nb = b.clone();
            let mut plain = String::new();
            if !nb.list_prefix.is_empty() {
                plain.push_str(&nb.list_prefix);
            }
            for s in &mut nb.spans {
                if let Some(ref fk) = s.is_dynamic_page_field {
                    s.text = if fk == "NUMPAGES" {
                        total_pages.to_string()
                    } else {
                        display_page.to_string()
                    };
                }
                plain.push_str(&s.text);
            }
            nb.plain_text = plain;
            nb
        })
        .collect()
}

pub fn paginate_document(
    extracted: &ExtractedPreviewDoc,
    prefer_last_rendered_breaks: bool,
    cancel: Option<&CancelToken>,
) -> Result<Vec<PreviewPageModel>, String> {
    let sections = &extracted.sections;
    if sections.is_empty() {
        return Ok(Vec::new());
    }

    struct DraftPage {
        section_index: usize,
        is_first_page_of_section: bool,
        blocks: Vec<PreviewBlock>,
        used_height_mm: f64,
        break_reason: String,
        has_overflow_warning: bool,
    }

    let mut pages: Vec<DraftPage> = Vec::new();
    let mut active_sect_idx: usize = sections[0].section_index;

    let get_sect = |idx: usize| -> &PreviewSectionSpec {
        sections
            .iter()
            .find(|s| s.section_index == idx)
            .unwrap_or(&sections[0])
    };

    let push_empty_page = |pages: &mut Vec<DraftPage>,
                           sect_idx: usize,
                           first_in_sect: bool,
                           reason: &str| {
        pages.push(DraftPage {
            section_index: sect_idx,
            is_first_page_of_section: first_in_sect,
            blocks: Vec::new(),
            used_height_mm: 0.0,
            break_reason: reason.to_string(),
            has_overflow_warning: false,
        });
    };

    push_empty_page(&mut pages, active_sect_idx, true, "sectionStart");

    for (blk_idx, blk) in extracted.blocks.iter().enumerate() {
        if let Some(tok) = cancel {
            if tok.is_cancelled() {
                return Err("CANCELLED".to_string());
            }
        }

        let blk_sect = match blk {
            PreviewBlock::Paragraph(p) => p.section_index,
            PreviewBlock::Table(t) => t.section_index,
        };

        if blk_sect != active_sect_idx {
            active_sect_idx = blk_sect;
            if !pages.last().map(|pg| pg.blocks.is_empty()).unwrap_or(false) {
                push_empty_page(&mut pages, active_sect_idx, true, "sectionBreak");
            } else if let Some(last) = pages.last_mut() {
                last.section_index = active_sect_idx;
                last.is_first_page_of_section = true;
                last.break_reason = "sectionBreak".to_string();
            }
        }

        let sect = get_sect(active_sect_idx);
        let content_cap_mm =
            (sect.height_mm - sect.margin_top_mm - sect.margin_bottom_mm).max(40.0);

        match blk {
            PreviewBlock::Paragraph(p) => {
                // Explicit page break before paragraph or Word cached lastRenderedPageBreak
                let should_break_before = !pages.last().map(|pg| pg.blocks.is_empty()).unwrap_or(true)
                    && (p.page_break_before
                        || (prefer_last_rendered_breaks && p.has_last_rendered_break));

                if should_break_before {
                    let reason = if p.page_break_before {
                        "pageBreakBefore"
                    } else {
                        "wordCachedBreak"
                    };
                    push_empty_page(&mut pages, active_sect_idx, false, reason);
                }

                // Check height fit + keepNext lookahead
                let next_h = if p.keep_next {
                    extracted.blocks.get(blk_idx + 1).map(|nb| match nb {
                        PreviewBlock::Paragraph(np) => np.estimated_height_mm.min(28.0),
                        PreviewBlock::Table(nt) => {
                            nt.rows.first().map(estimate_table_row_height_mm).unwrap_or(8.0)
                        }
                    })
                    .unwrap_or(0.0)
                } else {
                    0.0
                };

                let cur_used = pages.last().map(|pg| pg.used_height_mm).unwrap_or(0.0);
                if cur_used > 1.0 && (cur_used + p.estimated_height_mm + next_h) > content_cap_mm {
                    push_empty_page(&mut pages, active_sect_idx, false, "autoFlow");
                }

                let img_overflow = p.images.iter().any(|i| i.overflows_printable_area);
                let cur_page = pages.last_mut().unwrap();
                cur_page.blocks.push(PreviewBlock::Paragraph(p.clone()));
                cur_page.used_height_mm = round2(cur_page.used_height_mm + p.estimated_height_mm);
                if img_overflow || cur_page.used_height_mm > content_cap_mm + 2.0 {
                    cur_page.has_overflow_warning = true;
                }

                if p.has_explicit_page_break {
                    push_empty_page(&mut pages, active_sect_idx, false, "explicitPageBreak");
                }
            }
            PreviewBlock::Table(tbl) => {
                let total_tbl_h = estimate_table_height_mm(tbl);
                let cur_used = pages.last().map(|pg| pg.used_height_mm).unwrap_or(0.0);

                // Fits whole table on current page
                if cur_used + total_tbl_h <= content_cap_mm || (tbl.rows.len() <= 2 && cur_used < 1.0)
                {
                    let cur_page = pages.last_mut().unwrap();
                    if tbl.overflows_printable_width {
                        cur_page.has_overflow_warning = true;
                    }
                    cur_page.blocks.push(PreviewBlock::Table(tbl.clone()));
                    cur_page.used_height_mm = round2(cur_page.used_height_mm + total_tbl_h);
                    continue;
                }

                // If current page already has content and even first two rows won't fit comfortably -> break first
                let header_rows: Vec<_> =
                    tbl.rows.iter().filter(|r| r.is_header_row).cloned().collect();
                let header_h: f64 = header_rows.iter().map(estimate_table_row_height_mm).sum();
                let first_data_h = tbl
                    .rows
                    .iter()
                    .find(|r| !r.is_header_row)
                    .map(estimate_table_row_height_mm)
                    .unwrap_or(8.0);
                if cur_used > 1.0 && (cur_used + header_h + first_data_h) > content_cap_mm {
                    push_empty_page(&mut pages, active_sect_idx, false, "tableFlow");
                }

                // Slice rows across pages, repeating tblHeader rows at top of continued pages
                let mut row_cursor = 0usize;
                let mut is_continued = false;
                while row_cursor < tbl.rows.len() {
                    let cur_page = pages.last_mut().unwrap();
                    let mut slice_rows = Vec::new();
                    let mut slice_h = 0.0f64;

                    if is_continued && !header_rows.is_empty() {
                        for hr in &header_rows {
                            slice_h += estimate_table_row_height_mm(hr);
                            slice_rows.push(hr.clone());
                        }
                    }

                    let start_row_idx = tbl.rows[row_cursor].row_index;
                    let mut end_row_idx = start_row_idx;

                    while row_cursor < tbl.rows.len() {
                        let r = &tbl.rows[row_cursor];
                        let rh = estimate_table_row_height_mm(r);
                        let cap_left = (content_cap_mm - cur_page.used_height_mm).max(8.0);
                        if !slice_rows.is_empty() && (slice_h + rh) > cap_left {
                            break;
                        }
                        slice_rows.push(r.clone());
                        slice_h += rh;
                        end_row_idx = r.row_index + 1;
                        row_cursor += 1;
                        if slice_h >= cap_left {
                            break;
                        }
                    }

                    let mut sub_tbl = tbl.clone();
                    sub_tbl.rows = slice_rows;
                    sub_tbl.row_slice_range = (start_row_idx, end_row_idx);

                    if tbl.overflows_printable_width {
                        cur_page.has_overflow_warning = true;
                    }
                    cur_page.blocks.push(PreviewBlock::Table(sub_tbl));
                    cur_page.used_height_mm = round2(cur_page.used_height_mm + slice_h + 1.5);

                    if row_cursor < tbl.rows.len() {
                        is_continued = true;
                        push_empty_page(&mut pages, active_sect_idx, false, "tableContinued");
                    }
                }
            }
        }
    }

    // Remove trailing empty page created purely after explicit page break if document ended cleanly
    if pages.len() > 1
        && pages
            .last()
            .map(|p| p.blocks.is_empty() && p.break_reason == "explicitPageBreak")
            .unwrap_or(false)
    {
        pages.pop();
    }

    let total_pages = pages.len().max(1);
    let mut final_pages: Vec<PreviewPageModel> = Vec::with_capacity(pages.len());
    let mut running_display_num: usize = sections
        .first()
        .and_then(|s| s.page_num_start.map(|v| v as usize))
        .unwrap_or(1);
    let mut prev_sect_idx: Option<usize> = None;

    for (idx, pg) in pages.into_iter().enumerate() {
        let sect = get_sect(pg.section_index);
        if prev_sect_idx != Some(pg.section_index) {
            if let Some(restart) = sect.page_num_start {
                running_display_num = restart.max(1) as usize;
            }
            prev_sect_idx = Some(pg.section_index);
        }
        let disp_num = running_display_num;
        running_display_num += 1;

        let raw_hdr = if pg.is_first_page_of_section && sect.different_first_page {
            &sect.first_header_blocks
        } else {
            &sect.default_header_blocks
        };
        let raw_ftr = if pg.is_first_page_of_section && sect.different_first_page {
            &sect.first_footer_blocks
        } else {
            &sect.default_footer_blocks
        };

        let header_blocks = fill_hf_dynamic_spans(raw_hdr, disp_num, total_pages);
        let footer_blocks = fill_hf_dynamic_spans(raw_ftr, disp_num, total_pages);

        let content_cap_mm =
            (sect.height_mm - sect.margin_top_mm - sect.margin_bottom_mm).max(40.0);
        let fill_ratio_pct = round1(((pg.used_height_mm / content_cap_mm) * 100.0).min(120.0));

        let mut path_set: HashSet<String> = HashSet::new();
        let mut paths = Vec::new();
        for b in &pg.blocks {
            match b {
                PreviewBlock::Paragraph(p) => {
                    if path_set.insert(p.element_path.clone()) {
                        paths.push(p.element_path.clone());
                    }
                }
                PreviewBlock::Table(t) => {
                    if path_set.insert(t.element_path.clone()) {
                        paths.push(t.element_path.clone());
                    }
                    let tbl_alt = format!("tbl[{}]", t.table_index);
                    if path_set.insert(tbl_alt.clone()) {
                        paths.push(tbl_alt);
                    }
                    for r in &t.rows {
                        for c in &r.cells {
                            for cp in &c.paragraphs {
                                if path_set.insert(cp.element_path.clone()) {
                                    paths.push(cp.element_path.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        final_pages.push(PreviewPageModel {
            page_number: idx + 1,
            display_page_number: disp_num,
            total_pages,
            section_index: pg.section_index,
            is_first_page_of_section: pg.is_first_page_of_section,
            paper_label: sect.paper_label.clone(),
            orientation: sect.orientation.clone(),
            width_mm: sect.width_mm,
            height_mm: sect.height_mm,
            margin_top_mm: sect.margin_top_mm,
            margin_bottom_mm: sect.margin_bottom_mm,
            margin_left_mm: sect.margin_left_mm,
            margin_right_mm: sect.margin_right_mm,
            header_blocks,
            footer_blocks,
            blocks: pg.blocks,
            used_height_mm: pg.used_height_mm,
            fill_ratio_pct,
            break_reason: pg.break_reason,
            has_overflow_warning: pg.has_overflow_warning,
            element_paths_on_page: paths,
        });
    }

    Ok(final_pages)
}
