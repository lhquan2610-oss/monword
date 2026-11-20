use crate::models::{
    IntelligentDocumentMap, LayoutProposal, LayoutProposalTier, LayoutWarning, LayoutWarningKind,
    ParagraphMapEntry, RecognizedStructureKind, SectionMapEntry, SmartProposalKind, StdSeverity,
};
use std::collections::{HashMap, HashSet};

pub fn analyze_layout(
    maps: &mut IntelligentDocumentMap,
) -> (Vec<LayoutWarning>, Vec<LayoutProposal>) {
    let mut warnings: Vec<LayoutWarning> = Vec::new();
    let mut proposals: Vec<LayoutProposal> = Vec::new();
    let mut warn_seq = 0usize;
    let mut prop_seq = 0usize;

    let sec_lookup: HashMap<usize, &SectionMapEntry> = maps
        .section_map
        .iter()
        .map(|s| (s.section_index, s))
        .collect();
    let page_lookup: HashMap<usize, (f64, f64)> = maps
        .page_layout_map
        .iter()
        .map(|p| (p.page_number, (p.used_height_mm, p.printable_height_mm)))
        .collect();

    let mut flagged_orphan_pages: HashSet<usize> = HashSet::new();
    let mut flagged_ws_pages: HashSet<usize> = HashSet::new();
    let mut flagged_overflow_pages: HashSet<usize> = HashSet::new();

    // ------------------------------------------------------------------------
    // 1. TABLE LAYOUT & OVERFLOW & ORIENTATION ANALYSIS
    // ------------------------------------------------------------------------
    let mut wide_tables_by_section: HashMap<usize, Vec<usize>> = HashMap::new();
    for tbl in &maps.table_map {
        if tbl.table_role != "dataTable" {
            continue;
        }
        if tbl.overflows_width {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::TableOverflowWidth,
                severity: StdSeverity::Error,
                title: format!(
                    "Bảng {} tràn lề ngang (+{:.1} mm)",
                    tbl.table_index + 1,
                    tbl.overflow_mm
                ),
                detail: format!(
                    "Tổng độ rộng bảng {:.1} mm vượt khổ in khả dụng {:.1} mm ({} cột).",
                    tbl.total_width_mm, tbl.printable_width_mm, tbl.col_count
                ),
                target_element_id: tbl.table_id.clone(),
                locator: tbl.locator.clone(),
            });
            flagged_overflow_pages.insert(tbl.locator.estimated_page_start);

            prop_seq += 1;
            proposals.push(LayoutProposal {
                proposal_id: format!("prop-{}", prop_seq),
                kind: SmartProposalKind::ResolveOverflow,
                tier: LayoutProposalTier::RequiresConfirmation,
                confidence: 0.94,
                title: format!("Co bảng {} vừa khít vùng in ({:.1} mm)", tbl.table_index + 1, tbl.printable_width_mm),
                reason: format!("Bảng vượt lề phải {:.1} mm gây cắt nội dung khi in hoặc xuất PDF.", tbl.overflow_mm),
                suggested_change: "Co tỷ lệ lưới cột (tblGrid) và độ rộng ô (tcW) vừa đúng khổ in Section.".to_string(),
                target_element_ids: vec![tbl.table_id.clone()],
                locator: tbl.locator.clone(),
                affects_pagination: true,
                affects_section: false,
                affects_table: true,
                affects_signature: false,
                param_value: Some("fitTablePrintableWidth".to_string()),
            });

            if tbl.recommended_orientation == "landscape" {
                wide_tables_by_section
                    .entry(tbl.locator.section_index)
                    .or_default()
                    .push(tbl.table_index);
            }
        }

        let min_col_mm = tbl.col_widths_mm.iter().copied().fold(999.0f64, f64::min);
        if tbl.col_count >= 6 && min_col_mm < 15.0 {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::UnbalancedLayout,
                severity: StdSeverity::Warning,
                title: format!(
                    "Bảng {} nhiều cột ({} cột) bị ép hẹp (cột nhỏ nhất {:.1} mm)",
                    tbl.table_index + 1,
                    tbl.col_count,
                    min_col_mm
                ),
                detail: "Cột quá hẹp làm chữ ngắt dòng vụn và kéo dài chiều cao bảng bất cân đối trên trang dọc.".to_string(),
                target_element_id: tbl.table_id.clone(),
                locator: tbl.locator.clone(),
            });
            wide_tables_by_section
                .entry(tbl.locator.section_index)
                .or_default()
                .push(tbl.table_index);
        }

        let needs_table_align_or_flow = tbl.indent_left_mm.abs() > 2.5
            || (tbl.row_count >= 5 && !tbl.has_repeat_header)
            || tbl.locked_long_row_count > 0
            || tbl.alignment != "center";
        if needs_table_align_or_flow {
            if tbl.indent_left_mm.abs() > 2.5 {
                warn_seq += 1;
                warnings.push(LayoutWarning {
                    warning_id: format!("warn-{}", warn_seq),
                    kind: LayoutWarningKind::MisalignedComponent,
                    severity: StdSeverity::Warning,
                    title: format!("Bảng {} bị lệch lề trái ({:.1} mm)", tbl.table_index + 1, tbl.indent_left_mm),
                    detail: "Thuộc tính w:tblInd khiến bảng không thẳng hàng với lề văn bản chuẩn.".to_string(),
                    target_element_id: tbl.table_id.clone(),
                    locator: tbl.locator.clone(),
                });
            }
            prop_seq += 1;
            proposals.push(LayoutProposal {
                proposal_id: format!("prop-{}", prop_seq),
                kind: SmartProposalKind::AdjustTableLayout,
                tier: LayoutProposalTier::RequiresConfirmation,
                confidence: 0.90,
                title: format!("Căn chỉnh & tối ưu phân trang Bảng {}", tbl.table_index + 1),
                reason: "Bảng bị lệch lề, thiếu lặp dòng tiêu đề (tblHeader) ở bảng dài hoặc khóa cantSplit ở hàng dài.".to_string(),
                suggested_change: "Đặt w:tblInd=0, căn giữa bảng, bật tblHeader hàng đầu (nếu ≥ 5 hàng) và mở ngắt hàng dài.".to_string(),
                target_element_ids: vec![tbl.table_id.clone()],
                locator: tbl.locator.clone(),
                affects_pagination: true,
                affects_section: false,
                affects_table: true,
                affects_signature: false,
                param_value: Some("normalizeTable".to_string()),
            });
        }
    }

    for (sec_idx, tbl_indices) in wide_tables_by_section {
        let Some(&sec) = sec_lookup.get(&sec_idx) else {
            continue;
        };
        if sec.orientation != "landscape" {
            let has_header_in_sec = maps.document_map.iter().any(|d| {
                d.locator.section_index == sec_idx
                    && matches!(
                        d.kind,
                        RecognizedStructureKind::DocumentHeader | RecognizedStructureKind::DocTypeTitle
                    )
            });
            let first_tbl_id = tbl_indices
                .first()
                .map(|i| format!("tbl-{}", i))
                .unwrap_or_else(|| format!("sec-{}", sec_idx));
            let first_loc = maps
                .table_map
                .iter()
                .find(|t| tbl_indices.contains(&t.table_index))
                .map(|t| t.locator.clone())
                .unwrap_or_default();

            prop_seq += 1;
            proposals.push(LayoutProposal {
                proposal_id: format!("prop-{}", prop_seq),
                kind: SmartProposalKind::RecommendOrientation,
                tier: LayoutProposalTier::RequiresConfirmation,
                confidence: 0.87,
                title: format!("Đề xuất chuyển Section {} sang khổ giấy Ngang (Landscape)", sec_idx),
                reason: if has_header_in_sec {
                    format!(
                        "Section {} chứa bảng rộng ({} cột) nhưng đang dùng chung với phần đầu hành chính — chỉ xoay ngang khi xác nhận hoặc tách Section.",
                        sec_idx,
                        maps.table_map.iter().find(|t| tbl_indices.contains(&t.table_index)).map(|t| t.col_count).unwrap_or(6)
                    )
                } else {
                    format!("Section {} chứa bảng rộng nhiều cột thích hợp trình bày khổ A4 Ngang.", sec_idx)
                },
                suggested_change: "Đặt hướng giấy Section sang Landscape (297×210 mm) và xoay lề chuẩn NĐ 30.".to_string(),
                target_element_ids: vec![format!("sec-{}", sec_idx), first_tbl_id],
                locator: first_loc,
                affects_pagination: true,
                affects_section: true,
                affects_table: true,
                affects_signature: has_header_in_sec,
                param_value: Some(format!("landscapeSection:{}", sec_idx)),
            });
        }
    }

    // ------------------------------------------------------------------------
    // 2. VISUAL OBJECT OVERFLOW (IMAGES & TEXT BOXES)
    // ------------------------------------------------------------------------
    for vis in &maps.visual_objects {
        if vis.overflows_printable_area {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::ImageOverflowPage,
                severity: StdSeverity::Error,
                title: format!(
                    "{} ({:.1}×{:.1} mm) tràn vùng in",
                    if vis.object_kind == "textBox" { "Text Box" } else { "Hình ảnh" },
                    vis.width_mm,
                    vis.height_mm
                ),
                detail: format!(
                    "Đối tượng {} tại Section {} vượt giới hạn lề trang hoặc chiều cao vùng in.",
                    vis.object_id, vis.locator.section_index
                ),
                target_element_id: vis.object_id.clone(),
                locator: vis.locator.clone(),
            });
            flagged_overflow_pages.insert(vis.locator.estimated_page_start);

            if vis.object_kind == "image" && vis.anchor_mode == "inline" {
                prop_seq += 1;
                proposals.push(LayoutProposal {
                    proposal_id: format!("prop-{}", prop_seq),
                    kind: SmartProposalKind::ResolveOverflow,
                    tier: LayoutProposalTier::RequiresConfirmation,
                    confidence: 0.93,
                    title: format!("Thu tỷ lệ đồng dạng ảnh {} vừa khổ trang in", vis.object_id),
                    reason: format!("Ảnh rộng {:.1} mm vượt lề vùng in gây tràn lề phải.", vis.width_mm),
                    suggested_change: "Thu nhỏ đồng dạng wp:extent cx/cy theo đúng tỷ lệ gốc để nằm trọn lề in.".to_string(),
                    target_element_ids: vec![vis.object_id.clone()],
                    locator: vis.locator.clone(),
                    affects_pagination: true,
                    affects_section: false,
                    affects_table: false,
                    affects_signature: false,
                    param_value: Some(format!("fitInlineImage:{}", vis.object_id)),
                });
            }
        }
    }

    // ------------------------------------------------------------------------
    // 3. PARAGRAPH, HEADING, WHITESPACE & PAGINATION ANALYSIS (BODY SCOPE)
    // ------------------------------------------------------------------------
    let body_paras: Vec<&ParagraphMapEntry> = maps
        .paragraph_map
        .iter()
        .filter(|p| p.in_table_id.is_none() && p.in_text_box_id.is_none())
        .collect();

    let mut widow_risk_ids: Vec<String> = Vec::new();
    let mut stranded_heading_ids: Vec<String> = Vec::new();
    let mut heading_misaligned_ids: Vec<String> = Vec::new();
    let mut body_format_unify_ids: Vec<String> = Vec::new();
    let mut spacing_norm_ids: Vec<String> = Vec::new();

    // Group headings by level to check alignment & spacing consistency
    let mut heading_before_pts: HashMap<u8, Vec<f64>> = HashMap::new();
    for p in &body_paras {
        if let Some(lvl) = heading_level_opt(p.kind) {
            heading_before_pts.entry(lvl).or_default().push(p.space_before_pt);
        }
    }
    let dominant_h_before: HashMap<u8, f64> = heading_before_pts
        .into_iter()
        .map(|(lvl, mut vals)| {
            vals.sort_by(|a, b| a.total_cmp(b));
            (lvl, vals[vals.len() / 2])
        })
        .collect();

    // Detect runs of >= 2 empty paragraphs (stacked whitespace)
    let mut empty_run: Vec<&ParagraphMapEntry> = Vec::new();
    let mut collapsed_empty_runs: Vec<Vec<String>> = Vec::new();

    for p in &body_paras {
        let is_empty = p.excerpt.trim().is_empty() && !p.page_break_before;
        if is_empty {
            empty_run.push(p);
        } else {
            if empty_run.len() >= 2 {
                let first = empty_run[0];
                let ids: Vec<String> = empty_run.iter().map(|e| e.para_id.clone()).collect();
                warn_seq += 1;
                warnings.push(LayoutWarning {
                    warning_id: format!("warn-{}", warn_seq),
                    kind: LayoutWarningKind::StackedEmptyParagraphs,
                    severity: StdSeverity::Warning,
                    title: format!("{} đoạn trống liên tiếp tạo khoảng trắng giả", empty_run.len()),
                    detail: "Dùng nhiều phím Enter trống đẩy bố cục rời rạc và dễ làm nhảy trang ngoài ý muốn.".to_string(),
                    target_element_id: first.para_id.clone(),
                    locator: first.locator.clone(),
                });
                flagged_ws_pages.insert(first.locator.estimated_page_start);
                collapsed_empty_runs.push(ids);
            }
            empty_run.clear();
        }
    }
    if empty_run.len() >= 2 {
        let first = empty_run[0];
        let ids: Vec<String> = empty_run.iter().map(|e| e.para_id.clone()).collect();
        warn_seq += 1;
        warnings.push(LayoutWarning {
            warning_id: format!("warn-{}", warn_seq),
            kind: LayoutWarningKind::StackedEmptyParagraphs,
            severity: StdSeverity::Warning,
            title: format!("{} đoạn trống liên tiếp ở cuối phần", empty_run.len()),
            detail: "Các đoạn Enter trống dư thừa dễ sinh thêm trang trắng cuối văn bản.".to_string(),
            target_element_id: first.para_id.clone(),
            locator: first.locator.clone(),
        });
        flagged_ws_pages.insert(first.locator.estimated_page_start);
        collapsed_empty_runs.push(ids);
    }

    if !collapsed_empty_runs.is_empty() {
        let all_ids: Vec<String> = collapsed_empty_runs.iter().flatten().cloned().collect();
        let first_loc = body_paras
            .iter()
            .find(|p| all_ids.contains(&p.para_id))
            .map(|p| p.locator.clone())
            .unwrap_or_default();
        prop_seq += 1;
        proposals.push(LayoutProposal {
            proposal_id: format!("prop-{}", prop_seq),
            kind: SmartProposalKind::NormalizeSectionSpacing,
            tier: LayoutProposalTier::RequiresConfirmation,
            confidence: 0.89,
            title: format!("Thu gọn {} cụm đoạn trống liên tiếp thành khoảng cách chuẩn", collapsed_empty_runs.len()),
            reason: "Loại bỏ các dòng Enter trống lặp (giữ nguyên toàn bộ đoạn có chữ và hình ảnh) để ổn định phân trang.".to_string(),
            suggested_change: "Xóa đoạn trống dư thừa (≥ 2 đoạn trống liền kề chỉ giữ lại khoảng cách paragraph spacing).".to_string(),
            target_element_ids: all_ids,
            locator: first_loc,
            affects_pagination: true,
            affects_section: false,
            affects_table: false,
            affects_signature: false,
            param_value: Some("collapseStackedEmptyParagraphs".to_string()),
        });
    }

    for (idx, p) in body_paras.iter().enumerate() {
        let printable_w = sec_lookup
            .get(&p.locator.section_index)
            .map(|s| s.printable_width_mm)
            .unwrap_or(165.0);

        // Paragraph indent squeeze / overflow check
        if !p.excerpt.is_empty() && printable_w - p.indent_left_mm - p.indent_right_mm < 45.0 {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::ParagraphIndentOverflow,
                severity: StdSeverity::Warning,
                title: "Thụt lề trái/phải làm hẹp vùng chữ (< 45 mm)".to_string(),
                detail: format!(
                    "Đoạn \"{}\" thụt trái {:.1} mm, phải {:.1} mm.",
                    p.excerpt, p.indent_left_mm, p.indent_right_mm
                ),
                target_element_id: p.para_id.clone(),
                locator: p.locator.clone(),
            });
            flagged_overflow_pages.insert(p.locator.estimated_page_start);
        }

        // Excessive paragraph spacing check (> 24 pt before or after)
        if !p.excerpt.is_empty() && (p.space_before_pt > 24.0 || p.space_after_pt > 24.0) {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::ExcessiveSpacing,
                severity: StdSeverity::Warning,
                title: format!(
                    "Khoảng cách đoạn lớn bất thường (Trước {:.0}pt / Sau {:.0}pt)",
                    p.space_before_pt, p.space_after_pt
                ),
                detail: format!("Tạo khoảng hở lớn quanh đoạn \"{}\".", p.excerpt),
                target_element_id: p.para_id.clone(),
                locator: p.locator.clone(),
            });
            flagged_ws_pages.insert(p.locator.estimated_page_start);
            spacing_norm_ids.push(p.para_id.clone());
        }

        // Premature hard page break leaving previous page sparsely filled (< 35%)
        if p.page_break_before && idx > 0 {
            let prev_page_no = body_paras[idx - 1].locator.estimated_page_start;
            if let Some(&(used, print_h)) = page_lookup.get(&prev_page_no) {
                let ratio = used / print_h.max(100.0);
                if ratio < 0.35 && ratio > 0.01 {
                    warn_seq += 1;
                    warnings.push(LayoutWarning {
                        warning_id: format!("warn-{}", warn_seq),
                        kind: LayoutWarningKind::PrematurePageBreak,
                        severity: StdSeverity::Warning,
                        title: format!("Ngắt trang sớm khiến trang {} mới lấp {:.0}%", prev_page_no, ratio * 100.0),
                        detail: format!("Thuộc tính pageBreakBefore tại \"{}\" bỏ trống phần lớn trang trước.", p.excerpt),
                        target_element_id: p.para_id.clone(),
                        locator: p.locator.clone(),
                    });
                    flagged_ws_pages.insert(prev_page_no);

                    prop_seq += 1;
                    proposals.push(LayoutProposal {
                        proposal_id: format!("prop-{}", prop_seq),
                        kind: SmartProposalKind::RebalanceLayout,
                        tier: LayoutProposalTier::RequiresConfirmation,
                        confidence: 0.85,
                        title: format!("Gỡ ngắt trang sớm trước \"{}\"", truncate_chars(&p.excerpt, 40)),
                        reason: format!("Trang {} hiện chỉ đầy {:.0}%, gây khoảng trắng lớn bất thường.", prev_page_no, ratio * 100.0),
                        suggested_change: "Bỏ thuộc tính w:pageBreakBefore nếu không bắt buộc sang chương ở trang mới.".to_string(),
                        target_element_ids: vec![p.para_id.clone()],
                        locator: p.locator.clone(),
                        affects_pagination: true,
                        affects_section: false,
                        affects_table: false,
                        affects_signature: false,
                        param_value: Some(format!("removePageBreakBefore:{}", p.para_id)),
                    });
                }
            }
        }

        // Widow / Orphan line control on multi-line body prose
        if p.kind == RecognizedStructureKind::MainBody && p.estimated_lines >= 3 && !p.widow_control {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::WidowOrphanRisk,
                severity: StdSeverity::Warning,
                title: "Nguy cơ dòng đơn độc đầu/cuối trang (Widow/Orphan tắt)".to_string(),
                detail: format!("Đoạn \"{}\" dài {} dòng nhưng tắt w:widowControl.", p.excerpt, p.estimated_lines),
                target_element_id: p.para_id.clone(),
                locator: p.locator.clone(),
            });
            flagged_orphan_pages.insert(p.locator.estimated_page_start);
            widow_risk_ids.push(p.para_id.clone());
        }

        // Main body spacing / alignment / indent drift checks (HighConfidenceAuto candidates)
        if p.kind == RecognizedStructureKind::MainBody && p.excerpt.chars().count() >= 35 {
            if !p.widow_control
                || p.alignment == "left"
                || (p.indent_first_line_mm - 12.7).abs() > 3.5 && p.indent_first_line_mm < 1.0 && !p.excerpt.starts_with('-')
            {
                body_format_unify_ids.push(p.para_id.clone());
            }
            if (p.space_before_pt > 12.0 || p.space_after_pt > 12.0 || p.line_spacing_mult < 1.0 || p.line_spacing_mult > 1.8)
                && !spacing_norm_ids.contains(&p.para_id)
            {
                spacing_norm_ids.push(p.para_id.clone());
            }
        }

        // Headings alignment, spacing consistency & stranded bottom-of-page heading
        if let Some(lvl) = heading_level_opt(p.kind) {
            if !p.keep_next {
                warn_seq += 1;
                warnings.push(LayoutWarning {
                    warning_id: format!("warn-{}", warn_seq),
                    kind: LayoutWarningKind::StrandedHeadingBottom,
                    severity: StdSeverity::Warning,
                    title: format!("Tiêu đề cấp {} thiếu keepNext (nguy cơ đứng trơ cuối trang)", lvl),
                    detail: format!("Tiêu đề \"{}\" có thể tách rời khỏi đoạn nội dung tiếp theo khi phân trang.", p.excerpt),
                    target_element_id: p.para_id.clone(),
                    locator: p.locator.clone(),
                });
                flagged_orphan_pages.insert(p.locator.estimated_page_start);
                stranded_heading_ids.push(p.para_id.clone());
            }

            if (p.alignment == "right")
                || (lvl == 1 && p.indent_first_line_mm > 1.5)
                || (lvl >= 2 && (p.indent_first_line_mm > 1.5 || p.alignment == "center"))
            {
                warn_seq += 1;
                warnings.push(LayoutWarning {
                    warning_id: format!("warn-{}", warn_seq),
                    kind: LayoutWarningKind::MisalignedComponent,
                    severity: StdSeverity::Warning,
                    title: format!("Tiêu đề cấp {} lệch căn chỉnh / thụt đầu dòng", lvl),
                    detail: format!(
                        "Tiêu đề \"{}\" đang đặt căn lề '{}', thụt đầu dòng {:.1} mm.",
                        p.excerpt, p.alignment, p.indent_first_line_mm
                    ),
                    target_element_id: p.para_id.clone(),
                    locator: p.locator.clone(),
                });
                heading_misaligned_ids.push(p.para_id.clone());
            }

            if let Some(&dom_before) = dominant_h_before.get(&lvl) {
                if (p.space_before_pt - dom_before).abs() > 4.0 {
                    warn_seq += 1;
                    warnings.push(LayoutWarning {
                        warning_id: format!("warn-{}", warn_seq),
                        kind: LayoutWarningKind::InconsistentSectionSpacing,
                        severity: StdSeverity::Information,
                        title: format!("Khoảng cách trước tiêu đề cấp {} không đồng nhất", lvl),
                        detail: format!(
                            "\"{}\" có khoảng cách trước {:.0}pt (đa số cùng cấp là {:.0}pt).",
                            p.excerpt, p.space_before_pt, dom_before
                        ),
                        target_element_id: p.para_id.clone(),
                        locator: p.locator.clone(),
                    });
                    if !spacing_norm_ids.contains(&p.para_id) {
                        spacing_norm_ids.push(p.para_id.clone());
                    }
                }
            }
        }

        // Centered DocTypeTitle / MainTitle shifted off-center due to firstLine indent
        if matches!(
            p.kind,
            RecognizedStructureKind::DocTypeTitle | RecognizedStructureKind::MainTitle | RecognizedStructureKind::DocSubject
        ) && (p.alignment != "center" || p.indent_first_line_mm > 0.5)
        {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::MisalignedComponent,
                severity: StdSeverity::Warning,
                title: "Tiêu đề mở đầu / Trích yếu bị lệch tâm".to_string(),
                detail: format!(
                    "\"{}\" đang căn '{}' với thụt đầu dòng {:.1} mm làm lệch trục giữa trang.",
                    p.excerpt, p.alignment, p.indent_first_line_mm
                ),
                target_element_id: p.para_id.clone(),
                locator: p.locator.clone(),
            });
            heading_misaligned_ids.push(p.para_id.clone());
        }
    }

    // ------------------------------------------------------------------------
    // 4. SIGNATURE BLOCK COHESION CHECK
    // ------------------------------------------------------------------------
    let sig_paras: Vec<&ParagraphMapEntry> = body_paras
        .iter()
        .copied()
        .filter(|p| p.kind == RecognizedStructureKind::SignatureBlock)
        .collect();
    if sig_paras.len() >= 2 {
        let first_p = sig_paras[0].locator.estimated_page_start;
        let last_p = sig_paras[sig_paras.len() - 1].locator.estimated_page_start;
        let missing_keep = sig_paras
            .iter()
            .take(sig_paras.len().saturating_sub(1))
            .any(|p| !p.keep_next);
        if first_p != last_p || missing_keep {
            warn_seq += 1;
            warnings.push(LayoutWarning {
                warning_id: format!("warn-{}", warn_seq),
                kind: LayoutWarningKind::SignaturePageSplit,
                severity: StdSeverity::Warning,
                title: "Nguy cơ khối Chữ ký bị tách qua 2 trang".to_string(),
                detail: "Cụm Quyền hạn / Chức vụ / Người ký thiếu keepNext giữ liền khối hoặc đang nằm vắt qua ranh giới trang.".to_string(),
                target_element_id: sig_paras[0].para_id.clone(),
                locator: sig_paras[0].locator.clone(),
            });

            prop_seq += 1;
            proposals.push(LayoutProposal {
                proposal_id: format!("prop-{}", prop_seq),
                kind: SmartProposalKind::RebalanceLayout,
                tier: LayoutProposalTier::RequiresConfirmation,
                confidence: 0.91,
                title: "Giữ trọn khối Chữ ký liền một trang (keepNext + keepLines)".to_string(),
                reason: "Tránh tình trạng Chức vụ ký nằm cuối trang trước còn Họ tên người ký bị rớt sang trang mới.".to_string(),
                suggested_change: "Gắn w:keepNext và w:keepLines cho các dòng trong cụm Chữ ký.".to_string(),
                target_element_ids: sig_paras.iter().map(|p| p.para_id.clone()).collect(),
                locator: sig_paras[0].locator.clone(),
                affects_pagination: true,
                affects_section: false,
                affects_table: false,
                affects_signature: true,
                param_value: Some("keepSignatureTogether".to_string()),
            });
        }
    }

    // ------------------------------------------------------------------------
    // 5. AGGREGATED HIGH-CONFIDENCE AUTO & CONFIRMATION PROPOSALS
    // ------------------------------------------------------------------------
    if !spacing_norm_ids.is_empty() {
        let first_loc = body_paras
            .iter()
            .find(|p| spacing_norm_ids.contains(&p.para_id))
            .map(|p| p.locator.clone())
            .unwrap_or_default();
        prop_seq += 1;
        proposals.push(LayoutProposal {
            proposal_id: format!("prop-{}", prop_seq),
            kind: SmartProposalKind::NormalizeSpacing,
            tier: LayoutProposalTier::HighConfidenceAuto,
            confidence: 0.96,
            title: format!("Chuẩn hóa khoảng cách cho {} đoạn văn / tiêu đề", spacing_norm_ids.len()),
            reason: "Đưa khoảng cách Before/After quá lớn hoặc giãn dòng lệch chuẩn về mức cân đối (6pt / giãn dòng 1.15–1.25).".to_string(),
            suggested_change: "Đặt w:spacing (before=120, after=120, line=276) không làm thay đổi cấu trúc trang hay section.".to_string(),
            target_element_ids: spacing_norm_ids,
            locator: first_loc,
            affects_pagination: false,
            affects_section: false,
            affects_table: false,
            affects_signature: false,
            param_value: Some("normalizeSpacingAuto".to_string()),
        });
    }

    if !heading_misaligned_ids.is_empty() {
        let first_loc = body_paras
            .iter()
            .find(|p| heading_misaligned_ids.contains(&p.para_id))
            .map(|p| p.locator.clone())
            .unwrap_or_default();
        prop_seq += 1;
        proposals.push(LayoutProposal {
            proposal_id: format!("prop-{}", prop_seq),
            kind: SmartProposalKind::AlignHeadings,
            tier: LayoutProposalTier::HighConfidenceAuto,
            confidence: 0.97,
            title: format!("Căn chỉnh & xóa thụt dòng thừa cho {} tiêu đề", heading_misaligned_ids.len()),
            reason: "Loại bỏ w:firstLine gây lệch tâm tiêu đề và đồng nhất căn lề theo đúng cấp tiêu đề.".to_string(),
            suggested_change: "Đặt firstLine=0, căn giữa tên loại/tiêu đề chính/H1 và căn trái tiêu đề mục cấp 2–4.".to_string(),
            target_element_ids: heading_misaligned_ids,
            locator: first_loc,
            affects_pagination: false,
            affects_section: false,
            affects_table: false,
            affects_signature: false,
            param_value: Some("alignHeadingsAuto".to_string()),
        });
    }

    // Merge widow_risk_ids into body_format_unify_ids cleanly
    for wid in widow_risk_ids {
        if !body_format_unify_ids.contains(&wid) {
            body_format_unify_ids.push(wid);
        }
    }
    if !body_format_unify_ids.is_empty() {
        let first_loc = body_paras
            .iter()
            .find(|p| body_format_unify_ids.contains(&p.para_id))
            .map(|p| p.locator.clone())
            .unwrap_or_default();
        prop_seq += 1;
        proposals.push(LayoutProposal {
            proposal_id: format!("prop-{}", prop_seq),
            kind: SmartProposalKind::UnifyParagraphFormat,
            tier: LayoutProposalTier::HighConfidenceAuto,
            confidence: 0.95,
            title: format!("Đồng nhất định dạng & bật Widow/Orphan cho {} đoạn nội dung", body_format_unify_ids.len()),
            reason: "Đảm bảo đoạn văn chính căn đều 2 bên (justified), thụt đầu dòng chuẩn và bật chống dòng đơn độc.".to_string(),
            suggested_change: "Áp dụng w:jc=both, firstLine=720 (1.27cm) và bật w:widowControl.".to_string(),
            target_element_ids: body_format_unify_ids,
            locator: first_loc,
            affects_pagination: false,
            affects_section: false,
            affects_table: false,
            affects_signature: false,
            param_value: Some("unifyBodyParagraphsAuto".to_string()),
        });
    }

    if !stranded_heading_ids.is_empty() {
        let first_loc = body_paras
            .iter()
            .find(|p| stranded_heading_ids.contains(&p.para_id))
            .map(|p| p.locator.clone())
            .unwrap_or_default();
        prop_seq += 1;
        proposals.push(LayoutProposal {
            proposal_id: format!("prop-{}", prop_seq),
            kind: SmartProposalKind::NormalizeSectionSpacing,
            tier: LayoutProposalTier::RequiresConfirmation,
            confidence: 0.92,
            title: format!("Gắn giữ liền đoạn sau (keepNext) cho {} tiêu đề", stranded_heading_ids.len()),
            reason: "Ngăn tiêu đề nằm trơ một mình ở dòng cuối trang mà không đi cùng nội dung bên dưới.".to_string(),
            suggested_change: "Bổ sung w:keepNext và w:keepLines cho các tiêu đề thiếu neo phân trang.".to_string(),
            target_element_ids: stranded_heading_ids,
            locator: first_loc,
            affects_pagination: true,
            affects_section: false,
            affects_table: false,
            affects_signature: false,
            param_value: Some("keepHeadingsWithNext".to_string()),
        });
    }

    // Update page_layout_map summary flags
    for page in &mut maps.page_layout_map {
        if flagged_overflow_pages.contains(&page.page_number) {
            page.has_overflow = true;
        }
        if flagged_orphan_pages.contains(&page.page_number) {
            page.has_orphan_or_stranded_heading = true;
        }
        if flagged_ws_pages.contains(&page.page_number) {
            page.has_abnormal_whitespace = true;
        }
    }

    (warnings, proposals)
}

fn heading_level_opt(kind: RecognizedStructureKind) -> Option<u8> {
    match kind {
        RecognizedStructureKind::HeadingLevel1 => Some(1),
        RecognizedStructureKind::HeadingLevel2 => Some(2),
        RecognizedStructureKind::HeadingLevel3 => Some(3),
        RecognizedStructureKind::HeadingLevel4 => Some(4),
        _ => None,
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let head: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{}…", head)
    }
}
