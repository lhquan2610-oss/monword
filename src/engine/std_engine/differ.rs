use crate::engine::std_engine::ir::DocumentIr;
use crate::models::{
    DiffDimension, DiffEntry, DocumentDiffReport, FixActionRecord, PostSaveVerification,
    PreservationCheckSummary, StdAnalysisReport,
};
use std::collections::{BTreeMap, HashMap};

pub fn compute_document_diff(
    before: &DocumentIr,
    after: &DocumentIr,
    actions: &[FixActionRecord],
) -> DocumentDiffReport {
    let mut entries: Vec<DiffEntry> = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    let mut push_entry = |dim: DiffDimension,
                          location: String,
                          before_val: String,
                          after_val: String,
                          reason: String,
                          rule_id: String| {
        let key = format!("{:?}", dim);
        *counts.entry(key).or_insert(0) += 1;
        entries.push(DiffEntry {
            dimension: dim,
            location,
            before_val,
            after_val,
            reason,
            rule_id,
        });
    };

    // 1. Page & Section diffs
    let sect_len = before.sections.len().max(after.sections.len());
    for i in 0..sect_len {
        match (before.sections.get(i), after.sections.get(i)) {
            (Some(sb), Some(sa)) => {
                let s_idx = sb.summary.section_index;
                if (sb.summary.paper_width_mm - sa.summary.paper_width_mm).abs() > 0.2
                    || (sb.summary.paper_height_mm - sa.summary.paper_height_mm).abs() > 0.2
                    || (sb.summary.margin_top_mm - sa.summary.margin_top_mm).abs() > 0.2
                    || (sb.summary.margin_bottom_mm - sa.summary.margin_bottom_mm).abs() > 0.2
                    || (sb.summary.margin_left_mm - sa.summary.margin_left_mm).abs() > 0.2
                    || (sb.summary.margin_right_mm - sa.summary.margin_right_mm).abs() > 0.2
                {
                    push_entry(
                        DiffDimension::PageMarginsAndSize,
                        format!("Section {}", s_idx),
                        format!(
                            "{:.1}×{:.1}mm | Lề T:{:.1} D:{:.1} Tr:{:.1} Ph:{:.1}mm",
                            sb.summary.paper_width_mm,
                            sb.summary.paper_height_mm,
                            sb.summary.margin_top_mm,
                            sb.summary.margin_bottom_mm,
                            sb.summary.margin_left_mm,
                            sb.summary.margin_right_mm
                        ),
                        format!(
                            "{:.1}×{:.1}mm | Lề T:{:.1} D:{:.1} Tr:{:.1} Ph:{:.1}mm",
                            sa.summary.paper_width_mm,
                            sa.summary.paper_height_mm,
                            sa.summary.margin_top_mm,
                            sa.summary.margin_bottom_mm,
                            sa.summary.margin_left_mm,
                            sa.summary.margin_right_mm
                        ),
                        "Chuẩn hóa khổ giấy / lề trang theo NĐ 30/2020/NĐ-CP".to_string(),
                        "PAGE-01/02".to_string(),
                    );
                }
                if sb.summary.orientation != sa.summary.orientation
                    || sb.summary.different_first_page != sa.summary.different_first_page
                    || sb.summary.break_type != sa.summary.break_type
                {
                    push_entry(
                        DiffDimension::SectionStructure,
                        format!("Section {}", s_idx),
                        format!(
                            "{}, titlePg={}, break={}",
                            sb.summary.orientation, sb.summary.different_first_page, sb.summary.break_type
                        ),
                        format!(
                            "{}, titlePg={}, break={}",
                            sa.summary.orientation, sa.summary.different_first_page, sa.summary.break_type
                        ),
                        "Cập nhật thuộc tính Section (hướng / ẩn trang đầu)".to_string(),
                        "PGNUM-01/PAGE-03".to_string(),
                    );
                }
            }
            (Some(sb), None) => {
                push_entry(
                    DiffDimension::SectionStructure,
                    format!("Section {}", sb.summary.section_index),
                    "Tồn tại".to_string(),
                    "Đã mất".to_string(),
                    "Cảnh báo mất Section".to_string(),
                    "PRESERVE-01".to_string(),
                );
            }
            (None, Some(sa)) => {
                push_entry(
                    DiffDimension::SectionStructure,
                    format!("Section {}", sa.summary.section_index),
                    "Không có".to_string(),
                    "Mới thêm".to_string(),
                    "Thêm cấu trúc Section".to_string(),
                    "PAGE-03".to_string(),
                );
            }
            (None, None) => {}
        }
    }

    // 2. Table Structure diffs
    for i in 0..before.tables.len().min(after.tables.len()) {
        let tb = &before.tables[i];
        let ta = &after.tables[i];
        if tb.summary.total_width_twips != ta.summary.total_width_twips
            || tb.summary.has_repeat_header != ta.summary.has_repeat_header
            || tb.summary.has_locked_long_rows != ta.summary.has_locked_long_rows
        {
            push_entry(
                DiffDimension::TableStructure,
                format!("Bảng #{}", tb.table_index + 1),
                format!(
                    "Rộng {} twips · tblHeader={} · lockedRows={}",
                    tb.summary.total_width_twips,
                    tb.summary.has_repeat_header,
                    tb.summary.has_locked_long_rows
                ),
                format!(
                    "Rộng {} twips · tblHeader={} · lockedRows={}",
                    ta.summary.total_width_twips,
                    ta.summary.has_repeat_header,
                    ta.summary.has_locked_long_rows
                ),
                "Co bảng vừa vùng in / Tối ưu lặp tiêu đề & ngắt trang bảng".to_string(),
                "TBL-01/02".to_string(),
            );
        }
    }

    // 3. Header/Footer diffs
    let hf_before_str: String = before
        .headers_footers
        .iter()
        .map(|h| format!("{}:page={}:sz={:?}", h.part_name, h.has_page_field, h.font_sizes_pt))
        .collect::<Vec<_>>()
        .join("; ");
    let hf_after_str: String = after
        .headers_footers
        .iter()
        .map(|h| format!("{}:page={}:sz={:?}", h.part_name, h.has_page_field, h.font_sizes_pt))
        .collect::<Vec<_>>()
        .join("; ");
    if hf_before_str != hf_after_str {
        push_entry(
            DiffDimension::HeaderFooterAndPageNum,
            "Header / Footer".to_string(),
            if hf_before_str.is_empty() {
                "Không có Header/Footer số trang".to_string()
            } else {
                hf_before_str
            },
            hf_after_str,
            "Thiết lập hoặc chuẩn hóa số trang Header NĐ 30".to_string(),
            "PGNUM-01".to_string(),
        );
    }

    // 4. Paragraph-level diffs (match by element_path cleanly so PARA-04 removals don't shift everything)
    let after_by_path: HashMap<&str, &crate::engine::std_engine::ir::ParagraphInfo> = after
        .paragraphs
        .iter()
        .map(|p| (p.element_path.as_str(), p))
        .collect();

    let action_rule_by_path: HashMap<&str, Vec<&str>> = {
        let mut m: HashMap<&str, Vec<&str>> = HashMap::new();
        for a in actions {
            m.entry(a.element_path.as_str()).or_default().push(a.rule_id.as_str());
        }
        m
    };

    for pb in &before.paragraphs {
        let Some(pa) = after_by_path.get(pb.element_path.as_str()) else {
            if pb.trimmed.is_empty() {
                push_entry(
                    DiffDimension::SpacingAndIndent,
                    format!("Đoạn #{} ({})", pb.global_index + 1, pb.element_path),
                    "Đoạn trống thừa".to_string(),
                    "Đã loại bỏ".to_string(),
                    "Dọn đoạn trống thừa liên tiếp".to_string(),
                    "PARA-04".to_string(),
                );
            }
            continue;
        };

        if pb.trimmed != pa.trimmed {
            let reason = if pb.has_legacy_encoding {
                "Chuyển bảng mã TCVN3/VNI -> Unicode TCVN 6909:2001".to_string()
            } else {
                "Chuẩn hóa khoảng trắng / dấu câu / ký tự điều khiển".to_string()
            };
            let rid = if pb.has_legacy_encoding {
                "FONT-01"
            } else {
                "FONT-05"
            };
            push_entry(
                DiffDimension::TextContent,
                format!("Đoạn #{} ({})", pb.global_index + 1, pb.role.label_vi()),
                pb.trimmed.chars().take(70).collect(),
                pa.trimmed.chars().take(70).collect(),
                reason,
                rid.to_string(),
            );
        }

        if pb.primary_font != pa.primary_font || pb.font_names != pa.font_names {
            push_entry(
                DiffDimension::FontFamily,
                format!("Đoạn #{} ({})", pb.global_index + 1, pb.role.label_vi()),
                if pb.font_names.is_empty() {
                    "(chưa khai báo)".to_string()
                } else {
                    pb.font_names.join(", ")
                },
                pa.font_names.join(", "),
                "Chuẩn hóa phông chữ Times New Roman".to_string(),
                action_rule_by_path
                    .get(pb.element_path.as_str())
                    .map(|v| v.join(","))
                    .unwrap_or_else(|| "FONT-02".to_string()),
            );
        }

        if pb.sizes_pt != pa.sizes_pt {
            push_entry(
                DiffDimension::FontSize,
                format!("Đoạn #{} ({})", pb.global_index + 1, pb.role.label_vi()),
                format!("{:?} pt", pb.sizes_pt),
                format!("{:?} pt", pa.sizes_pt),
                "Điều chỉnh cỡ chữ theo thành phần thể thức".to_string(),
                "FONT-03".to_string(),
            );
        }

        if pb.is_bold_majority != pa.is_bold_majority
            || pb.is_italic_majority != pa.is_italic_majority
            || pb.non_black_colors != pa.non_black_colors
        {
            push_entry(
                DiffDimension::FontStyle,
                format!("Đoạn #{} ({})", pb.global_index + 1, pb.role.label_vi()),
                format!(
                    "bold={} italic={} colors={:?}",
                    pb.is_bold_majority, pb.is_italic_majority, pb.non_black_colors
                ),
                format!(
                    "bold={} italic={} colors={:?}",
                    pa.is_bold_majority, pa.is_italic_majority, pa.non_black_colors
                ),
                "Chuẩn hóa màu chữ đen / kiểu chữ đậm nghiêng thành phần".to_string(),
                "FONT-04/STRUCT".to_string(),
            );
        }

        if pb.effective_jc != pa.effective_jc {
            push_entry(
                DiffDimension::Alignment,
                format!("Đoạn #{} ({})", pb.global_index + 1, pb.role.label_vi()),
                pb.effective_jc.clone(),
                pa.effective_jc.clone(),
                "Chuẩn hóa căn chỉnh đoạn văn".to_string(),
                "PARA-01".to_string(),
            );
        }

        if pb.indent_first_line != pa.indent_first_line
            || pb.indent_left != pa.indent_left
            || pb.spacing_before != pa.spacing_before
            || pb.spacing_after != pa.spacing_after
            || pb.line_spacing != pa.line_spacing
        {
            push_entry(
                DiffDimension::SpacingAndIndent,
                format!("Đoạn #{} ({})", pb.global_index + 1, pb.role.label_vi()),
                format!(
                    "firstLine={} left={} before={} after={} line={}",
                    pb.indent_first_line,
                    pb.indent_left,
                    pb.spacing_before,
                    pb.spacing_after,
                    pb.line_spacing
                ),
                format!(
                    "firstLine={} left={} before={} after={} line={}",
                    pa.indent_first_line,
                    pa.indent_left,
                    pa.spacing_before,
                    pa.spacing_after,
                    pa.line_spacing
                ),
                "Chuẩn hóa thụt đầu dòng & giãn dòng / khoảng cách đoạn".to_string(),
                "PARA-02/03".to_string(),
            );
        }
    }

    let preservation = verify_preservation(before, after);

    DocumentDiffReport {
        entries,
        dimension_counts: counts,
        preservation,
    }
}

pub fn verify_preservation(before: &DocumentIr, after: &DocumentIr) -> PreservationCheckSummary {
    let text_before = before
        .paragraphs
        .iter()
        .filter(|p| !p.trimmed.is_empty())
        .count();
    let text_after = after
        .paragraphs
        .iter()
        .filter(|p| !p.trimmed.is_empty())
        .count();

    let mut warnings = Vec::new();
    let mut passed = true;

    if text_after < text_before {
        passed = false;
        warnings.push(format!(
            "Số đoạn có nội dung giảm ({} -> {}); có nguy cơ mất nội dung.",
            text_before, text_after
        ));
    }
    if after.tables.len() < before.tables.len() {
        passed = false;
        warnings.push(format!(
            "Số bảng thay đổi ({} -> {}).",
            before.tables.len(),
            after.tables.len()
        ));
    }
    if after.assets.media_count < before.assets.media_count {
        passed = false;
        warnings.push(format!(
            "Mất tệp ảnh nhúng word/media ({} -> {}).",
            before.assets.media_count, after.assets.media_count
        ));
    }
    if after.assets.drawing_count < before.assets.drawing_count {
        passed = false;
        warnings.push(format!(
            "Mất đối tượng vẽ/logo ({} -> {}).",
            before.assets.drawing_count, after.assets.drawing_count
        ));
    }
    if after.assets.hyperlink_count < before.assets.hyperlink_count {
        passed = false;
        warnings.push(format!(
            "Mất siêu liên kết Hyperlink ({} -> {}).",
            before.assets.hyperlink_count, after.assets.hyperlink_count
        ));
    }
    if after.assets.bookmark_count < before.assets.bookmark_count {
        passed = false;
        warnings.push(format!(
            "Mất Bookmark ({} -> {}).",
            before.assets.bookmark_count, after.assets.bookmark_count
        ));
    }
    if after.assets.footnote_count < before.assets.footnote_count {
        passed = false;
        warnings.push(format!(
            "Mất Footnote ({} -> {}).",
            before.assets.footnote_count, after.assets.footnote_count
        ));
    }
    if after.assets.endnote_count < before.assets.endnote_count {
        passed = false;
        warnings.push(format!(
            "Mất Endnote ({} -> {}).",
            before.assets.endnote_count, after.assets.endnote_count
        ));
    }
    if after.assets.comment_count < before.assets.comment_count {
        passed = false;
        warnings.push(format!(
            "Mất Comment ({} -> {}).",
            before.assets.comment_count, after.assets.comment_count
        ));
    }
    if after.assets.field_count < before.assets.field_count {
        passed = false;
        warnings.push(format!(
            "Mất Field tự động ({} -> {}).",
            before.assets.field_count, after.assets.field_count
        ));
    }
    if before.assets.has_vba_macros && !after.assets.has_vba_macros {
        passed = false;
        warnings.push("Mất gói Macro VBA (word/vbaProject.bin) trong .docm.".to_string());
    }
    if after.sections.len() < before.sections.len() {
        passed = false;
        warnings.push(format!(
            "Số Section giảm ({} -> {}).",
            before.sections.len(),
            after.sections.len()
        ));
    }

    PreservationCheckSummary {
        passed,
        text_paragraphs_before: text_before,
        text_paragraphs_after: text_after,
        tables_before: before.tables.len(),
        tables_after: after.tables.len(),
        media_before: before.assets.media_count,
        media_after: after.assets.media_count,
        drawings_before: before.assets.drawing_count,
        drawings_after: after.assets.drawing_count,
        hyperlinks_before: before.assets.hyperlink_count,
        hyperlinks_after: after.assets.hyperlink_count,
        bookmarks_before: before.assets.bookmark_count,
        bookmarks_after: after.assets.bookmark_count,
        footnotes_before: before.assets.footnote_count,
        footnotes_after: after.assets.footnote_count,
        endnotes_before: before.assets.endnote_count,
        endnotes_after: after.assets.endnote_count,
        comments_before: before.assets.comment_count,
        comments_after: after.assets.comment_count,
        fields_before: before.assets.field_count,
        fields_after: after.assets.field_count,
        macros_before: before.assets.has_vba_macros,
        macros_after: after.assets.has_vba_macros,
        warnings,
    }
}

pub fn build_post_save_verification(
    output_exists: bool,
    output_size_bytes: u64,
    reopened_from_disk: bool,
    report_after: Option<&StdAnalysisReport>,
    preservation: &PreservationCheckSummary,
) -> PostSaveVerification {
    let remaining_critical_or_error = report_after
        .map(|r| r.severity_counts.critical + r.severity_counts.error)
        .unwrap_or(0);
    let xml_valid = reopened_from_disk && report_after.is_some();

    let summary_message = if !output_exists {
        "Tệp đầu ra không tồn tại trên ổ đĩa.".to_string()
    } else if !xml_valid {
        "Không mở lại được cấu trúc OpenXML từ tệp đã lưu.".to_string()
    } else if !preservation.passed {
        format!(
            "Cảnh báo bảo toàn thất bại: {}",
            preservation.warnings.join("; ")
        )
    } else {
        format!(
            "Đã kiểm tra lại tệp từ ổ đĩa ({} KB): OpenXML hợp lệ, bảo toàn 100% nội dung/tài sản; còn {} lỗi Nghiêm trọng/Cần sửa.",
            (output_size_bytes + 512) / 1024,
            remaining_critical_or_error
        )
    };

    PostSaveVerification {
        output_exists,
        output_size_bytes,
        reopened_from_disk,
        xml_valid,
        preservation_passed: preservation.passed,
        remaining_critical_or_error,
        summary_message,
    }
}
