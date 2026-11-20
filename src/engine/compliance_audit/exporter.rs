use crate::engine::reliability::error_code::{classify_error_message, AppErrorCode};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::preservation::PreservationPolicy;
use crate::engine::reliability::safe_save::{save_bytes_atomic, save_openxml_verified};
use crate::engine::{header_builder, system_office};
use crate::engine::xml_helpers::{
    XmlDocument, XmlElement, XmlNode, P_PR_ORDER, R_PR_ORDER, SECT_PR_ORDER, TBL_PR_ORDER,
    TC_PR_ORDER,
};
use crate::models::{
    AuditExportFormat, AuditExportResult, ComplianceAuditReport, ComplianceBasisKind,
    ComplianceObligation, StdSeverity,
};
use std::env;
use std::fs;
use std::path::Path;

pub const NON_LEGAL_DISCLAIMER_VI: &str = "Lưu ý giới hạn phạm vi: Báo cáo chỉ phản ánh kết quả kiểm tra kỹ thuật trình bày, bố cục và thể thức OpenXML theo bộ quy tắc cấu hình tại thời điểm kiểm tra; KHÔNG phải là kết luận thẩm định tính hợp pháp, thẩm quyền ban hành hay giá trị pháp lý của văn bản.";

pub fn export_report(
    report: &ComplianceAuditReport,
    format: AuditExportFormat,
    dest_path: &Path,
) -> AuditExportResult {
    let timer = OpTimer::start("compliance_audit", "export_report", Some(dest_path));
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                let err_msg = format!("Không tạo được thư mục xuất báo cáo: {}", e);
                let classified = classify_error_message(&err_msg, AppErrorCode::ErrFileWrite);
                let code = classified.code.as_str().to_string();
                let formatted = classified.format_user_message();
                timer.finish_err(&code, &formatted);
                return AuditExportResult {
                    success: false,
                    output_path: String::new(),
                    format,
                    size_bytes: 0,
                    error: Some(formatted),
                    error_code: Some(code),
                };
            }
        }
    }

    let res = match format {
        AuditExportFormat::Docx => export_docx(report, dest_path),
        AuditExportFormat::Pdf => export_pdf(report, dest_path),
        AuditExportFormat::Csv => export_csv(report, dest_path),
        AuditExportFormat::Json => export_json(report, dest_path),
    };

    match res {
        Ok(size) => {
            timer.finish_ok(&format!("Xuất báo cáo {:?} thành công ({} bytes)", format, size));
            AuditExportResult {
                success: true,
                output_path: dest_path.to_string_lossy().to_string(),
                format,
                size_bytes: size,
                error: None,
                error_code: None,
            }
        }
        Err(e) => {
            let fallback = match format {
                AuditExportFormat::Pdf => AppErrorCode::ErrDocConvert,
                _ => AppErrorCode::ErrFileWrite,
            };
            let classified = classify_error_message(&e, fallback);
            let code = classified.code.as_str().to_string();
            let formatted = classified.format_user_message();
            timer.finish_err(&code, &formatted);
            AuditExportResult {
                success: false,
                output_path: dest_path.to_string_lossy().to_string(),
                format,
                size_bytes: 0,
                error: Some(formatted),
                error_code: Some(code),
            }
        }
    }
}

fn obligation_vi(ob: ComplianceObligation) -> &'static str {
    match ob {
        ComplianceObligation::Mandatory => "Bắt buộc",
        ComplianceObligation::Conditional => "Có điều kiện",
        ComplianceObligation::Recommended => "Khuyến nghị",
        ComplianceObligation::NotApplicable => "Không áp dụng",
    }
}

fn basis_vi(bk: ComplianceBasisKind) -> &'static str {
    match bk {
        ComplianceBasisKind::LegalRegulation => "Quy định thể thức",
        ComplianceBasisKind::TechnicalRecommendation => "Khuyến nghị kỹ thuật",
        ComplianceBasisKind::OrganizationPolicy => "Quy định tổ chức",
    }
}

fn severity_vi(s: StdSeverity) -> &'static str {
    match s {
        StdSeverity::Critical => "Nghiêm trọng",
        StdSeverity::Error => "Lỗi cần sửa",
        StdSeverity::Warning => "Cảnh báo",
        StdSeverity::Information => "Thông tin",
    }
}

fn export_docx(report: &ComplianceAuditReport, dest_path: &Path) -> Result<u64, String> {
    let mut pkg = header_builder::build_empty_valid_docx_package();
    let doc_xml = build_report_document_xml(report)?;
    pkg.set_xml("word/document.xml", doc_xml);
    save_openxml_verified(&pkg, dest_path, None, &PreservationPolicy::default())
}

fn export_pdf(report: &ComplianceAuditReport, dest_pdf: &Path) -> Result<u64, String> {
    let tmp_docx = env::temp_dir().join(format!(
        "monword_audit_pdf_tmp_{}_{}.docx",
        std::process::id(),
        chrono::Local::now().timestamp_millis()
    ));
    export_docx(report, &tmp_docx)?;
    let conv_res = system_office::word_com_refresh_and_export_pdf(&tmp_docx, false, Some(dest_pdf));
    let _ = fs::remove_file(&tmp_docx);
    conv_res?;
    let meta = fs::metadata(dest_pdf)
        .map_err(|e| format!("Không tìm thấy tệp PDF vừa xuất: {}", e))?;
    if meta.len() < 100 {
        return Err("Tệp PDF xuất ra không hợp lệ hoặc rỗng.".to_string());
    }
    Ok(meta.len())
}

fn export_csv(report: &ComplianceAuditReport, dest_csv: &Path) -> Result<u64, String> {
    let mut csv = String::from("\u{FEFF}");
    csv.push_str(&format!("# {}\r\n", NON_LEGAL_DISCLAIMER_VI));
    csv.push_str(&format!(
        "# Tệp: {} | Thời điểm: {} | Bộ quy tắc: {} v{} | SHA256: {}\r\n",
        report.file_info.file_name,
        report.checked_at,
        report.rule_profile.pack_id,
        report.rule_profile.pack_version,
        report.file_info.doc_sha256
    ));
    csv.push_str("ID,MaQuyTac,PhienBanQT,TenQuyTac,NghiaVu,NguonCanCu,CanCu,MucDo,ViTri,DuongDanXML,HienTai,ChuanDeXuat,PhuongAnXuLy,TrangThai\r\n");

    for f in &report.traceable_findings {
        let fields = [
            f.finding_id.as_str(),
            f.rule_id.as_str(),
            f.rule_version.as_str(),
            f.rule_name.as_str(),
            obligation_vi(f.obligation),
            basis_vi(f.basis_kind),
            f.legal_basis.as_str(),
            severity_vi(f.severity),
            f.location.as_str(),
            f.element_path.as_str(),
            f.current_value.as_str(),
            f.expected_value.as_str(),
            f.remediation_method.as_str(),
            &format!("{:?}", f.disposition),
        ];
        let line: Vec<String> = fields.iter().map(|v| csv_escape(v)).collect();
        csv.push_str(&line.join(","));
        csv.push_str("\r\n");
    }

    save_bytes_atomic(dest_csv, csv.as_bytes(), false)?;
    Ok(csv.len() as u64)
}

fn export_json(report: &ComplianceAuditReport, dest_json: &Path) -> Result<u64, String> {
    let raw = serde_json::to_string_pretty(report)
        .map_err(|e| format!("Lỗi tuần tự hóa báo cáo JSON: {}", e))?;
    save_bytes_atomic(dest_json, raw.as_bytes(), false)?;
    Ok(raw.len() as u64)
}

fn csv_escape(s: &str) -> String {
    let cleaned = s.replace(['\r', '\n'], " ");
    format!("\"{}\"", cleaned.replace('"', "\"\""))
}

fn make_para(text: &str, size_half_pt: u32, bold: bool, italic: bool, align: &str, after_tw: u32) -> XmlElement {
    let mut p = XmlElement::new("w:p");
    let p_pr = p.ensure_child_ordered("w:pPr", P_PR_ORDER);
    let sp = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
    sp.set_attr("w:before", "40");
    sp.set_attr("w:after", after_tw.to_string());
    sp.set_attr("w:line", "260");
    sp.set_attr("w:lineRule", "auto");
    let jc = p_pr.ensure_child_ordered("w:jc", P_PR_ORDER);
    jc.set_attr("w:val", align);

    let mut r = XmlElement::new("w:r");
    let r_pr = r.ensure_child_ordered("w:rPr", R_PR_ORDER);
    let rf = r_pr.ensure_child_ordered("w:rFonts", R_PR_ORDER);
    rf.set_attr("w:ascii", "Times New Roman");
    rf.set_attr("w:hAnsi", "Times New Roman");
    rf.set_attr("w:cs", "Times New Roman");
    if bold {
        r_pr.ensure_child_ordered("w:b", R_PR_ORDER);
    }
    if italic {
        r_pr.ensure_child_ordered("w:i", R_PR_ORDER);
    }
    let sz = r_pr.ensure_child_ordered("w:sz", R_PR_ORDER);
    sz.set_attr("w:val", size_half_pt.to_string());
    let sz_cs = r_pr.ensure_child_ordered("w:szCs", R_PR_ORDER);
    sz_cs.set_attr("w:val", size_half_pt.to_string());

    let mut t = XmlElement::new("w:t");
    t.set_attr("xml:space", "preserve");
    t.set_text(text);
    r.children.push(XmlNode::Element(t));
    p.children.push(XmlNode::Element(r));
    p
}

fn build_bordered_table(col_widths: &[i32], rows: &[Vec<(&str, bool)>]) -> XmlElement {
    let total_w: i32 = col_widths.iter().sum();
    let mut tbl = XmlElement::new("w:tbl");
    let tbl_pr = tbl.ensure_child_ordered("w:tblPr", TBL_PR_ORDER);
    let tbl_w = tbl_pr.ensure_child_ordered("w:tblW", TBL_PR_ORDER);
    tbl_w.set_attr("w:w", total_w.to_string());
    tbl_w.set_attr("w:type", "dxa");
    let borders = tbl_pr.ensure_child_ordered("w:tblBorders", TBL_PR_ORDER);
    for side in ["w:top", "w:left", "w:bottom", "w:right", "w:insideH", "w:insideV"] {
        let b = borders.ensure_child_ordered(side, &["w:top", "w:left", "w:bottom", "w:right", "w:insideH", "w:insideV"]);
        b.set_attr("w:val", "single");
        b.set_attr("w:sz", "4");
        b.set_attr("w:space", "0");
        b.set_attr("w:color", "94A3B8");
    }

    let mut grid = XmlElement::new("w:tblGrid");
    for &w in col_widths {
        let mut gc = XmlElement::new("w:gridCol");
        gc.set_attr("w:w", w.to_string());
        grid.children.push(XmlNode::Element(gc));
    }
    tbl.children.push(XmlNode::Element(grid));

    for row in rows {
        let mut tr = XmlElement::new("w:tr");
        for (c_idx, &(txt, bold)) in row.iter().enumerate() {
            let mut tc = XmlElement::new("w:tc");
            let tc_pr = tc.ensure_child_ordered("w:tcPr", TC_PR_ORDER);
            let tc_w = tc_pr.ensure_child_ordered("w:tcW", TC_PR_ORDER);
            tc_w.set_attr(
                "w:w",
                col_widths.get(c_idx).copied().unwrap_or(2000).to_string(),
            );
            tc_w.set_attr("w:type", "dxa");
            tc.children
                .push(XmlNode::Element(make_para(txt, 22, bold, false, "left", 40)));
            tr.children.push(XmlNode::Element(tc));
        }
        tbl.children.push(XmlNode::Element(tr));
    }
    tbl
}

fn build_report_document_xml(report: &ComplianceAuditReport) -> Result<String, String> {
    let mut doc = XmlElement::new("w:document");
    doc.set_attr("xmlns:w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main");
    doc.set_attr("xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");

    let mut body = XmlElement::new("w:body");

    body.children.push(XmlNode::Element(make_para(
        "BÁO CÁO KIỂM TOÁN TUÂN THỦ THỂ THỨC & CHẤT LƯỢNG KỸ THUẬT VĂN BẢN",
        30,
        true,
        false,
        "center",
        80,
    )));
    body.children.push(XmlNode::Element(make_para(
        &format!("Mã phiên kiểm tra: {} · Thời điểm: {}", report.audit_id, report.checked_at),
        22,
        false,
        true,
        "center",
        120,
    )));
    body.children.push(XmlNode::Element(make_para(
        &report.legal_disclaimer,
        22,
        false,
        true,
        "both",
        140,
    )));

    // Section 1: Metadata & Rule Profile Table
    body.children.push(XmlNode::Element(make_para(
        "1. THÔNG TIN TỆP & HỒ SƠ QUY TẮC ÁP DỤNG",
        26,
        true,
        false,
        "left",
        80,
    )));
    let file_size_str = format!("{} bytes", report.file_info.file_size_bytes);
    let scope_str = format!("{:?}", report.file_info.effective_scope);
    let pack_str = format!(
        "{} v{} ({})",
        report.rule_profile.pack_id, report.rule_profile.pack_version, report.rule_profile.effective_date
    );
    let sec_page_str = format!(
        "{} section · {} trang ước tính · {} đoạn · {} bảng",
        report.file_info.section_count,
        report.file_info.estimated_page_count,
        report.file_info.paragraph_count,
        report.file_info.table_count
    );
    let matrix_counts_str = format!(
        "Bắt buộc: {} | Có điều kiện: {} | Khuyến nghị: {} | Không áp dụng: {}",
        report.rule_profile.mandatory_count,
        report.rule_profile.conditional_count,
        report.rule_profile.recommended_count,
        report.rule_profile.not_applicable_count
    );
    let qual_str = format!(
        "Điểm kỹ thuật: {}/100 (Phông {:.1}%, Cỡ {:.1}%, Dãn đoạn {:.1}%)",
        report.consistency.technical_quality_index,
        report.consistency.font_uniformity_pct,
        report.consistency.size_uniformity_pct,
        report.consistency.spacing_uniformity_pct
    );

    let meta_rows: Vec<Vec<(&str, bool)>> = vec![
        vec![("Thuộc tính", true), ("Giá trị ghi nhận", true)],
        vec![("Tên tệp & Loại văn bản", false), (&report.file_info.file_name, false)],
        vec![("Phạm vi & Dung lượng", false), (&file_size_str, false)],
        vec![("Phạm vi kiểm tra", false), (&scope_str, false)],
        vec![("Mã băm tệp (SHA-256)", false), (&report.file_info.doc_sha256, false)],
        vec![("Hồ sơ bộ quy tắc", false), (&pack_str, false)],
        vec![("Thống kê cấu trúc", false), (&sec_page_str, false)],
        vec![("Phân bổ ma trận quy tắc", false), (&matrix_counts_str, false)],
        vec![("Chỉ số nhất quán kỹ thuật", false), (&qual_str, false)],
    ];
    body.children
        .push(XmlNode::Element(build_bordered_table(&[3200, 6155], &meta_rows)));

    // Section 2: Findings Traceability Table
    body.children.push(XmlNode::Element(make_para(
        &format!(
            "2. DANH SÁCH LỖI & TRUY XUẤT CĂN CỨ ({} mục)",
            report.traceable_findings.len()
        ),
        26,
        true,
        false,
        "left",
        80,
    )));
    if report.traceable_findings.is_empty() {
        body.children.push(XmlNode::Element(make_para(
            "Không phát hiện lỗi nào trong các quy tắc áp dụng ở lần kiểm tra này.",
            24,
            false,
            false,
            "left",
            100,
        )));
    } else {
        let mut f_rows: Vec<Vec<(String, bool)>> = Vec::new();
        f_rows.push(vec![
            ("Mã & Phiên bản".to_string(), true),
            ("Nghĩa vụ & Mức".to_string(), true),
            ("Vị trí & Căn cứ".to_string(), true),
            ("Phương án & Trạng thái".to_string(), true),
        ]);
        for f in report.traceable_findings.iter().take(120) {
            f_rows.push(vec![
                (format!("{} (v{})", f.rule_id, f.rule_version), false),
                (
                    format!("{} · {}", obligation_vi(f.obligation), severity_vi(f.severity)),
                    false,
                ),
                (
                    format!("{} [{}]\nCăn cứ: {}", f.location, f.element_path, f.legal_basis),
                    false,
                ),
                (
                    format!("{}\nTrạng thái: {:?}", f.remediation_method, f.disposition),
                    false,
                ),
            ]);
        }
        let f_refs: Vec<Vec<(&str, bool)>> = f_rows
            .iter()
            .map(|r| r.iter().map(|(s, b)| (s.as_str(), *b)).collect())
            .collect();
        body.children.push(XmlNode::Element(build_bordered_table(
            &[1500, 1755, 3100, 3000],
            &f_refs,
        )));
    }

    // Section 3: Advanced Checks + Recheck
    if !report.advanced_issues.is_empty() || !report.repeated_clusters.is_empty() {
        body.children.push(XmlNode::Element(make_para(
            "3. KIỂM TRA CHÉO, MÂU THUẪN CẤU HÌNH & CỤM LỖI LẶP",
            26,
            true,
            false,
            "left",
            80,
        )));
        for adv in &report.advanced_issues {
            body.children.push(XmlNode::Element(make_para(
                &format!(
                    "- [{}] {}: {} (Căn cứ: {} | Đề xuất: {})",
                    adv.issue_id, adv.title, adv.explanation, adv.legal_basis, adv.remediation
                ),
                22,
                false,
                false,
                "both",
                60,
            )));
        }
        for clu in &report.repeated_clusters {
            body.children.push(XmlNode::Element(make_para(
                &format!(
                    "- Cụm lỗi lặp {} ({} lần): {} — {}",
                    clu.rule_id, clu.occurrence_count, clu.rule_name, clu.recommended_batch_action
                ),
                22,
                false,
                false,
                "both",
                60,
            )));
        }
    }

    if let Some(ref rc) = report.recheck {
        body.children.push(XmlNode::Element(make_para(
            "4. KẾT QUẢ KIỂM TRA LẠI SAU TỰ ĐỘNG SỬA (AUTO-FIX RECHECK)",
            26,
            true,
            false,
            "left",
            80,
        )));
        body.children.push(XmlNode::Element(make_para(
            &format!(
                "Thời điểm hậu kiểm: {} · Trước sửa: {} lỗi -> Sau sửa: {} lỗi · Đã sửa: {} · Bỏ qua: {} · Bảo toàn dữ liệu: {}",
                rc.rechecked_at,
                rc.before_findings,
                rc.after_findings,
                rc.fixed_count,
                rc.skipped_count,
                if rc.preservation_passed { "Đạt 100%" } else { "Cần rà soát" }
            ),
            24,
            false,
            false,
            "both",
            100,
        )));
    }

    body.children.push(XmlNode::Element(make_para(
        &format!("Ghi chú cuối báo cáo: {}", report.legal_disclaimer),
        20,
        false,
        true,
        "left",
        60,
    )));

    let mut sect_pr = XmlElement::new("w:sectPr");
    let pg_sz = sect_pr.ensure_child_ordered("w:pgSz", SECT_PR_ORDER);
    pg_sz.set_attr("w:w", "11906");
    pg_sz.set_attr("w:h", "16838");
    let pg_mar = sect_pr.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
    pg_mar.set_attr("w:top", "1134");
    pg_mar.set_attr("w:right", "850");
    pg_mar.set_attr("w:bottom", "1134");
    pg_mar.set_attr("w:left", "1701");
    pg_mar.set_attr("w:header", "708");
    pg_mar.set_attr("w:footer", "708");
    pg_mar.set_attr("w:gutter", "0");
    body.children.push(XmlNode::Element(sect_pr));

    doc.children.push(XmlNode::Element(body));

    let xml_doc = XmlDocument {
        decl: Some((
            "1.0".to_string(),
            Some("UTF-8".to_string()),
            Some("yes".to_string()),
        )),
        prelude: Vec::new(),
        root: doc,
    };
    xml_doc.to_xml_string()
}
