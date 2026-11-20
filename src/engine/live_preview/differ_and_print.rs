use crate::engine::compliance_audit::{self, AuditHistoryStore, ComplianceRulePackStore};
use crate::engine::layout_intelligence::mapper::round1;
use crate::models::{
    AuditSyncMarker, ComplianceAuditRequest, OrganizationProfile, PreviewBlock, PreviewElementDiff,
    PreviewPageModel, PreviewSectionSpec, PrintCheckReport, PrintDiagnosticItem, StdDocScope,
    StdSeverity,
};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[derive(Clone)]
struct BlockSnapshot<'a> {
    page_number: usize,
    summary_short: String,
    font_sig: String,
    spacing_sig: String,
    align_sig: String,
    text_sig: String,
    _blk: &'a PreviewBlock,
}

fn collect_block_map(pages: &[PreviewPageModel]) -> BTreeMap<String, BlockSnapshot<'_>> {
    let mut map = BTreeMap::new();
    for pg in pages {
        for blk in &pg.blocks {
            match blk {
                PreviewBlock::Paragraph(p) => {
                    let font = p
                        .spans
                        .first()
                        .map(|s| format!("{} {}pt", s.font_family, round1(s.font_size_pt)))
                        .unwrap_or_else(|| "Normal".to_string());
                    let spacing = format!(
                        "line:{}x bf:{}pt af:{}pt ind:{}mm",
                        round1(p.line_spacing_mult),
                        round1(p.space_before_pt),
                        round1(p.space_after_pt),
                        round1(p.first_line_indent_mm)
                    );
                    let snippet: String = p.plain_text.chars().take(55).collect();
                    map.insert(
                        p.element_path.clone(),
                        BlockSnapshot {
                            page_number: pg.page_number,
                            summary_short: format!("{} · {} · {}", font, p.alignment, snippet),
                            font_sig: font,
                            spacing_sig: spacing,
                            align_sig: p.alignment.clone(),
                            text_sig: p.plain_text.clone(),
                            _blk: blk,
                        },
                    );
                }
                PreviewBlock::Table(t) => {
                    let key = if t.row_slice_range.0 > 0 {
                        format!("{}#slice{}", t.element_path, t.row_slice_range.0)
                    } else {
                        t.element_path.clone()
                    };
                    let font = t
                        .rows
                        .first()
                        .and_then(|r| r.cells.first())
                        .and_then(|c| c.paragraphs.first())
                        .and_then(|p| p.spans.first())
                        .map(|s| format!("{} {}pt", s.font_family, round1(s.font_size_pt)))
                        .unwrap_or_else(|| "Table".to_string());
                    map.insert(
                        key,
                        BlockSnapshot {
                            page_number: pg.page_number,
                            summary_short: format!(
                                "Bảng {} ({} dòng · {}mm)",
                                t.table_role,
                                t.rows.len(),
                                round1(t.total_width_mm)
                            ),
                            font_sig: font,
                            spacing_sig: format!("w:{}mm ind:{}mm", round1(t.total_width_mm), round1(t.indent_left_mm)),
                            align_sig: t.alignment.clone(),
                            text_sig: format!("rows={}", t.rows.len()),
                            _blk: blk,
                        },
                    );
                }
            }
        }
    }
    map
}

pub fn compute_before_after_diffs(
    before_pages: &[PreviewPageModel],
    after_pages: &[PreviewPageModel],
    before_sections: &[PreviewSectionSpec],
    after_sections: &[PreviewSectionSpec],
) -> Vec<PreviewElementDiff> {
    let mut diffs = Vec::new();
    let mut seq = 0usize;

    // 1. Section-level paper/margin diffs
    for (i, a_sec) in after_sections.iter().enumerate() {
        if let Some(b_sec) = before_sections.get(i) {
            let geom_changed = (b_sec.width_mm - a_sec.width_mm).abs() > 0.3
                || (b_sec.height_mm - a_sec.height_mm).abs() > 0.3
                || b_sec.orientation != a_sec.orientation
                || (b_sec.margin_top_mm - a_sec.margin_top_mm).abs() > 0.3
                || (b_sec.margin_bottom_mm - a_sec.margin_bottom_mm).abs() > 0.3
                || (b_sec.margin_left_mm - a_sec.margin_left_mm).abs() > 0.3
                || (b_sec.margin_right_mm - a_sec.margin_right_mm).abs() > 0.3;
            if geom_changed {
                seq += 1;
                let sect_page = after_pages
                    .iter()
                    .find(|p| p.section_index == a_sec.section_index)
                    .map(|p| p.page_number);
                diffs.push(PreviewElementDiff {
                    diff_id: format!("DIFF-{:04}", seq),
                    element_path: format!("section[{}]", a_sec.section_index),
                    before_page: before_pages
                        .iter()
                        .find(|p| p.section_index == b_sec.section_index)
                        .map(|p| p.page_number),
                    after_page: sect_page,
                    change_kinds: vec!["pageSetup".to_string()],
                    summary_vi: format!(
                        "Section {}: thay đổi khổ/hướng hoặc lề trang ({} {})",
                        a_sec.section_index, a_sec.paper_label, a_sec.orientation
                    ),
                    before_summary: format!(
                        "{} {} · Lề T/D/Tr/P: {}/{}/{}/{}mm",
                        b_sec.paper_label,
                        b_sec.orientation,
                        b_sec.margin_top_mm,
                        b_sec.margin_bottom_mm,
                        b_sec.margin_left_mm,
                        b_sec.margin_right_mm
                    ),
                    after_summary: format!(
                        "{} {} · Lề T/D/Tr/P: {}/{}/{}/{}mm",
                        a_sec.paper_label,
                        a_sec.orientation,
                        a_sec.margin_top_mm,
                        a_sec.margin_bottom_mm,
                        a_sec.margin_left_mm,
                        a_sec.margin_right_mm
                    ),
                });
            }
        }
    }

    // 2. Element-level diffs
    let b_map = collect_block_map(before_pages);
    let a_map = collect_block_map(after_pages);

    for (path, a_snap) in &a_map {
        match b_map.get(path) {
            Some(b_snap) => {
                let mut kinds = Vec::new();
                if b_snap.font_sig != a_snap.font_sig {
                    kinds.push("typography".to_string());
                }
                if b_snap.align_sig != a_snap.align_sig {
                    kinds.push("alignment".to_string());
                }
                if b_snap.spacing_sig != a_snap.spacing_sig {
                    kinds.push("spacing".to_string());
                }
                if b_snap.text_sig != a_snap.text_sig {
                    kinds.push("content".to_string());
                }
                if b_snap.page_number != a_snap.page_number && kinds.is_empty() {
                    kinds.push("pageMoved".to_string());
                }
                if !kinds.is_empty() {
                    seq += 1;
                    diffs.push(PreviewElementDiff {
                        diff_id: format!("DIFF-{:04}", seq),
                        element_path: path.clone(),
                        before_page: Some(b_snap.page_number),
                        after_page: Some(a_snap.page_number),
                        summary_vi: format!(
                            "Thay đổi [{}] tại {}",
                            kinds.join(", "),
                            path
                        ),
                        change_kinds: kinds,
                        before_summary: b_snap.summary_short.clone(),
                        after_summary: a_snap.summary_short.clone(),
                    });
                }
            }
            None => {
                seq += 1;
                diffs.push(PreviewElementDiff {
                    diff_id: format!("DIFF-{:04}", seq),
                    element_path: path.clone(),
                    before_page: None,
                    after_page: Some(a_snap.page_number),
                    change_kinds: vec!["added".to_string()],
                    summary_vi: format!("Khối mới / tách mới tại trang {}", a_snap.page_number),
                    before_summary: "(Chưa có trong bản gốc)".to_string(),
                    after_summary: a_snap.summary_short.clone(),
                });
            }
        }
    }

    diffs
}

pub fn evaluate_print_readiness(
    pages: &[PreviewPageModel],
    sections: &[PreviewSectionSpec],
    scope: StdDocScope,
    fidelity_disclaimer: &str,
) -> PrintCheckReport {
    let mut diagnostics = Vec::new();
    let mut seq = 0usize;
    let mut clipped_count = 0usize;
    let mut overflow_table_count = 0usize;

    let is_formal = matches!(
        scope,
        StdDocScope::AdminNd30
            | StdDocScope::PartyDocument
            | StdDocScope::SpecializedLegal
            | StdDocScope::AcademicThesis
    );

    // 1. Paper & Orientation & Margin check per Section
    for s in sections {
        let first_pg = pages
            .iter()
            .find(|p| p.section_index == s.section_index)
            .map(|p| p.page_number)
            .unwrap_or(1);

        if is_formal && s.paper_label != "A4" {
            seq += 1;
            diagnostics.push(PrintDiagnosticItem {
                item_id: format!("PRT-{:03}", seq),
                kind: "paperSize".to_string(),
                severity: StdSeverity::Error,
                page_number: first_pg,
                element_path: format!("section[{}]", s.section_index),
                title: format!("Section {}: khổ giấy {} (chuẩn NĐ30 là A4)", s.section_index, s.paper_label),
                detail: format!(
                    "Kích thước trang hiện là {}×{}mm; khi in ra giấy A4 dễ bị co tỷ lệ hoặc tràn lề.",
                    s.width_mm, s.height_mm
                ),
            });
        }

        let min_mar = s
            .margin_top_mm
            .min(s.margin_bottom_mm)
            .min(s.margin_left_mm)
            .min(s.margin_right_mm);
        if min_mar < 5.0 {
            clipped_count += 1;
            seq += 1;
            diagnostics.push(PrintDiagnosticItem {
                item_id: format!("PRT-{:03}", seq),
                kind: "clippedContent".to_string(),
                severity: StdSeverity::Critical,
                page_number: first_pg,
                element_path: format!("section[{}]", s.section_index),
                title: format!("Lề Section {} quá nhỏ (< 5 mm) — nguy cơ máy in cắt mất chữ", s.section_index),
                detail: format!(
                    "Lề hiện tại T/D/Tr/P = {}/{}/{}/{}mm. Hầu hết máy in văn phòng có vùng không in từ 4–5mm.",
                    s.margin_top_mm, s.margin_bottom_mm, s.margin_left_mm, s.margin_right_mm
                ),
            });
        } else if is_formal
            && ((s.orientation == "portrait"
                && (s.margin_top_mm < 18.0
                    || s.margin_bottom_mm < 18.0
                    || s.margin_left_mm < 28.0
                    || s.margin_right_mm < 14.0))
                || (s.orientation == "landscape" && s.margin_top_mm < 28.0))
        {
            seq += 1;
            diagnostics.push(PrintDiagnosticItem {
                item_id: format!("PRT-{:03}", seq),
                kind: "printMargin".to_string(),
                severity: StdSeverity::Warning,
                page_number: first_pg,
                element_path: format!("section[{}]", s.section_index),
                title: format!("Lề Section {} ({}) hẹp hơn khuyến nghị đóng gáy NĐ30", s.section_index, s.orientation),
                detail: format!(
                    "Lề hiện tại T/D/Tr/P = {}/{}/{}/{}mm.",
                    s.margin_top_mm, s.margin_bottom_mm, s.margin_left_mm, s.margin_right_mm
                ),
            });
        }

        if s.header_distance_mm < 5.0 || s.footer_distance_mm < 5.0 {
            seq += 1;
            diagnostics.push(PrintDiagnosticItem {
                item_id: format!("PRT-{:03}", seq),
                kind: "headerFooter".to_string(),
                severity: StdSeverity::Warning,
                page_number: first_pg,
                element_path: format!("section[{}]", s.section_index),
                title: "Khoảng cách Header/Footer sát mép giấy (< 5 mm)".to_string(),
                detail: "Số trang hoặc dòng đầu/chân trang có thể bị máy in cắt mất phần mép trên/dưới.".to_string(),
            });
        }
    }

    // 2. Per-page table overflow, clipped image, and page-break checks
    for pg in pages {
        if pg.has_overflow_warning {
            // Inspect specific offending blocks
            for b in &pg.blocks {
                match b {
                    PreviewBlock::Paragraph(p) => {
                        for img in &p.images {
                            if img.overflows_printable_area {
                                clipped_count += 1;
                                seq += 1;
                                diagnostics.push(PrintDiagnosticItem {
                                    item_id: format!("PRT-{:03}", seq),
                                    kind: "clippedContent".to_string(),
                                    severity: StdSeverity::Error,
                                    page_number: pg.page_number,
                                    element_path: p.element_path.clone(),
                                    title: format!("Trang {}: hình ảnh vượt bề rộng vùng in ({}mm)", pg.page_number, img.width_mm),
                                    detail: format!(
                                        "Ảnh \"{}\" rộng {}mm vượt khỏi vùng in của Section ({}mm).",
                                        img.alt_text,
                                        img.width_mm,
                                        round1(pg.width_mm - pg.margin_left_mm - pg.margin_right_mm)
                                    ),
                                });
                            }
                        }
                    }
                    PreviewBlock::Table(t) => {
                        if t.overflows_printable_width {
                            overflow_table_count += 1;
                            seq += 1;
                            diagnostics.push(PrintDiagnosticItem {
                                item_id: format!("PRT-{:03}", seq),
                                kind: "tableOverflow".to_string(),
                                severity: StdSeverity::Error,
                                page_number: pg.page_number,
                                element_path: t.element_path.clone(),
                                title: format!("Trang {}: bảng #{} rộng {}mm tràn lề phải", pg.page_number, t.table_index + 1, t.total_width_mm),
                                detail: format!(
                                    "Tổng bề rộng bảng {}mm + thụt trái {}mm vượt vùng in {}mm.",
                                    t.total_width_mm,
                                    t.indent_left_mm,
                                    round1(pg.width_mm - pg.margin_left_mm - pg.margin_right_mm)
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    let any_dynamic_pg = sections.iter().any(|s| s.has_dynamic_page_field);
    if is_formal && pages.len() >= 2 && !any_dynamic_pg {
        seq += 1;
        diagnostics.push(PrintDiagnosticItem {
            item_id: format!("PRT-{:03}", seq),
            kind: "pageNumbering".to_string(),
            severity: StdSeverity::Warning,
            page_number: 2,
            element_path: "section[1]".to_string(),
            title: "Tài liệu nhiều trang nhưng chưa có trường đánh số trang tự động".to_string(),
            detail: format!(
                "Tài liệu có {} trang ước tính nhưng chưa phát hiện trường động PAGE trong Header/Footer.",
                pages.len()
            ),
        });
    }

    let paper_labels: Vec<String> = {
        let mut v: Vec<String> = sections.iter().map(|s| s.paper_label.clone()).collect();
        v.dedup();
        v
    };
    let orient_labels: Vec<String> = {
        let mut v: Vec<String> = sections
            .iter()
            .map(|s| if s.orientation == "landscape" { "Ngang".to_string() } else { "Dọc".to_string() })
            .collect();
        v.dedup();
        v
    };

    let warn_count = diagnostics.len();
    let error_or_crit = diagnostics
        .iter()
        .any(|d| matches!(d.severity, StdSeverity::Critical | StdSeverity::Error));

    PrintCheckReport {
        is_print_ready: !error_or_crit && clipped_count == 0 && overflow_table_count == 0,
        paper_summary: format!("{} ({} Section)", paper_labels.join(" + "), sections.len()),
        orientation_summary: orient_labels.join(" → "),
        margin_summary: sections
            .first()
            .map(|s| {
                format!(
                    "S1: Trên {} · Dưới {} · Trái {} · Phải {} mm",
                    s.margin_top_mm, s.margin_bottom_mm, s.margin_left_mm, s.margin_right_mm
                )
            })
            .unwrap_or_default(),
        header_footer_summary: if sections
            .iter()
            .any(|s| !s.default_header_blocks.is_empty() || !s.default_footer_blocks.is_empty())
        {
            "Có Header/Footer (hỗ trợ bỏ trang đầu w:titlePg)".to_string()
        } else {
            "Chưa gắn Header/Footer riêng".to_string()
        },
        page_number_summary: if any_dynamic_pg {
            format!("Đã đánh số tự động ({} trang)", pages.len())
        } else {
            format!("{} trang (chưa có trường PAGE)", pages.len())
        },
        clipped_count,
        overflow_table_count,
        warning_count: warn_count,
        diagnostics,
        fidelity_disclaimer: fidelity_disclaimer.to_string(),
    }
}

pub fn resolve_page_for_element(pages: &[PreviewPageModel], element_path: &str) -> usize {
    if element_path.is_empty() || element_path == "body" || element_path.starts_with("config/") {
        return 1;
    }
    if let Some(sect_num) = element_path
        .strip_prefix("section[")
        .and_then(|s| s.strip_suffix(']'))
        .and_then(|s| s.parse::<usize>().ok())
    {
        return pages
            .iter()
            .find(|p| p.section_index == sect_num)
            .map(|p| p.page_number)
            .unwrap_or(1);
    }
    for pg in pages {
        if pg
            .element_paths_on_page
            .iter()
            .any(|p| p == element_path || element_path.starts_with(p) || p.starts_with(element_path))
        {
            return pg.page_number;
        }
    }
    1
}

pub fn build_audit_sync_markers(
    src_path: &Path,
    scope: StdDocScope,
    before_pages: &[PreviewPageModel],
    org_profile: Option<&OrganizationProfile>,
) -> Vec<AuditSyncMarker> {
    let pack_store = ComplianceRulePackStore::default_store();
    let hist_store = AuditHistoryStore::default_store();
    let req = ComplianceAuditRequest {
        path: src_path.to_string_lossy().to_string(),
        scope,
        doc_type_hint: None,
        org_profile: org_profile.cloned(),
        disabled_rule_ids: Vec::new(),
        record_history: false,
    };

    let Ok(report) = compliance_audit::run_compliance_audit(&req, &pack_store, &hist_store) else {
        return Vec::new();
    };

    let mut markers = Vec::new();
    let mut seq = 0usize;
    let obligation_map: HashMap<&str, &str> = [
        ("Mandatory", "Bắt buộc"),
        ("Conditional", "Có điều kiện"),
        ("Recommended", "Khuyến nghị"),
        ("NotApplicable", "Không áp dụng"),
    ]
    .into_iter()
    .collect();

    for f in &report.traceable_findings {
        seq += 1;
        let ob_str = format!("{:?}", f.obligation);
        markers.push(AuditSyncMarker {
            marker_id: format!("AUD-{:04}", seq),
            source: "ruleFinding".to_string(),
            rule_or_issue_id: f.rule_id.clone(),
            title: f.rule_name.clone(),
            severity: f.severity,
            obligation: obligation_map
                .get(ob_str.as_str())
                .copied()
                .unwrap_or(&ob_str)
                .to_string(),
            legal_basis: f.legal_basis.clone(),
            element_path: f.element_path.clone(),
            page_number: resolve_page_for_element(before_pages, &f.element_path),
            current_value: f.current_value.clone(),
            expected_value: f.expected_value.clone(),
            remediation: f.remediation_method.clone(),
        });
    }

    for adv in &report.advanced_issues {
        seq += 1;
        markers.push(AuditSyncMarker {
            marker_id: format!("AUD-{:04}", seq),
            source: "advancedIssue".to_string(),
            rule_or_issue_id: adv.issue_id.clone(),
            title: adv.title.clone(),
            severity: adv.severity,
            obligation: "Cảnh báo nâng cao".to_string(),
            legal_basis: adv.legal_basis.clone(),
            element_path: adv.element_path.clone(),
            page_number: resolve_page_for_element(before_pages, &adv.element_path),
            current_value: adv.location.clone(),
            expected_value: String::new(),
            remediation: adv.remediation.clone(),
        });
    }

    markers
}
