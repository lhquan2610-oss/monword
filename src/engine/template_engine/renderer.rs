use crate::engine::formatter::paper_dimensions_twips;
use crate::engine::header_builder::{
    apply_header_to_package, build_empty_valid_docx_package, inspect_package_header,
};
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::page_toc::apply_page_and_toc;
use crate::engine::std_engine::ir::DocumentIr;
use crate::engine::xml_helpers::{
    cm_to_twips, ensure_wt_space_preserve, line_to_twips, pt_to_twips, set_run_font_and_size,
    XmlDocument, XmlElement, XmlNode, P_PR_ORDER, R_PR_ORDER, SECT_PR_ORDER, TBL_CELL_MAR_ORDER,
    TBL_PR_ORDER, TC_PR_ORDER, TR_PR_ORDER,
};
use crate::models::{
    DocumentTemplate, DynamicTemplateFields, FormatConfig, HeaderActionMode, HeaderBuilderConfig,
    HeaderDateMode, HeaderNumberMode, MissingFieldMarker, OrgSector, OrganizationProfile,
    OrientationMode, OutputMode, PageTocConfig, SignatureLayoutKind, TemplateApplyRequest,
    TemplateBlockKind, TemplateFormatRules, TemplatePreviewBlock, TemplateSectionBlock,
    TemplateTableBlueprint, TemplateTableColumn, TemplateTableConfig,
};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct ResolvedTemplateData {
    pub values: BTreeMap<String, String>,
    pub missing_fields: Vec<MissingFieldMarker>,
    pub effective_org: OrganizationProfile,
    pub header_cfg: HeaderBuilderConfig,
    pub so_va_ky_hieu: String,
    pub ngay_thang_line: String,
    pub dia_danh_line: String,
}

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*([a-zA-Z0-9_]+)\s*\}\}").expect("valid regex"))
}

fn slash_date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*(\d{1,2})\s*[/\-\.]\s*(\d{1,2})\s*[/\-\.]\s*(\d{4})\s*$").expect("valid date regex"))
}

fn vi_date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)ngày\s*(\d{1,2})\s*tháng\s*(\d{1,2})\s*năm\s*(\d{4})").expect("valid vi date regex"))
}

pub fn format_nd30_day(day: u32) -> String {
    if (1..=9).contains(&day) {
        format!("0{}", day)
    } else {
        day.to_string()
    }
}

pub fn format_nd30_month(month: u32) -> String {
    if month == 1 || month == 2 {
        format!("0{}", month)
    } else {
        month.to_string()
    }
}

pub fn format_nd30_date_parts(day: &str, month: &str, year: &str) -> String {
    let d_trim = day.trim();
    let m_trim = month.trim();
    let y_trim = year.trim();
    let d_out = d_trim
        .parse::<u32>()
        .map(format_nd30_day)
        .unwrap_or_else(|_| d_trim.to_string());
    let m_out = m_trim
        .parse::<u32>()
        .map(format_nd30_month)
        .unwrap_or_else(|_| m_trim.to_string());
    format!("ngày {} tháng {} năm {}", d_out, m_out, y_trim)
}

fn field_label_vi(key: &str) -> &'static str {
    match key {
        "ten_co_quan" => "Tên cơ quan / tổ chức ban hành",
        "co_quan_chu_quan" => "Cơ quan chủ quản",
        "so_van_ban" => "Số và ký hiệu văn bản",
        "dia_danh" => "Địa danh ban hành",
        "ngay_thang" => "Ngày tháng ban hành",
        "nguoi_ky" => "Họ và tên người ký",
        "chuc_vu" => "Chức vụ người ký",
        "nguoi_nhan" => "Người nhận / Kính gửi",
        "trich_yeu" => "Trích yếu nội dung",
        _ => "Trường dữ liệu động",
    }
}

fn split_number_and_symbol(
    raw_so: &str,
    tpl: &DocumentTemplate,
    viet_tat: &str,
) -> (String, String, bool) {
    let t = raw_so.trim();
    let default_symbol = || {
        let prefix = tpl.header_config.symbol_prefix.trim();
        let org_abbr = if viet_tat.trim().is_empty() {
            "[CƠ QUAN]"
        } else {
            viet_tat.trim()
        };
        if prefix.is_empty() {
            org_abbr.to_string()
        } else {
            format!("{}-{}", prefix, org_abbr)
        }
    };

    if t.is_empty() {
        return ("[CHƯA NHẬP SỐ]".to_string(), default_symbol(), true);
    }

    if let Some((left, right)) = t.split_once('/') {
        let num = left.trim().to_string();
        let sym = if right.trim().is_empty() {
            default_symbol()
        } else {
            right.trim().to_string()
        };
        (num, sym, false)
    } else {
        let num = t
            .parse::<u32>()
            .map(format_nd30_day)
            .unwrap_or_else(|_| t.to_string());
        (num, default_symbol(), false)
    }
}

pub fn resolve_template_data(
    tpl: &DocumentTemplate,
    org_opt: Option<&OrganizationProfile>,
    fields: &DynamicTemplateFields,
    doc_placeholders: &[String],
) -> ResolvedTemplateData {
    let mut values = BTreeMap::new();
    let mut missing = Vec::new();
    let mut seen_missing = BTreeSet::new();

    let mut push_missing = |key: &str, label: &str, marker: &str, required_by: &str| {
        if seen_missing.insert(key.to_string()) {
            missing.push(MissingFieldMarker {
                field_key: key.to_string(),
                label_vi: label.to_string(),
                marker_text: marker.to_string(),
                required_by: required_by.to_string(),
            });
        }
    };

    // 1. Organization Name & Abbr
    let org_issuing_raw = if !fields.ten_co_quan.trim().is_empty() {
        fields.ten_co_quan.trim().to_string()
    } else if let Some(o) = org_opt {
        o.issuing_org.trim().to_string()
    } else {
        String::new()
    };

    let ten_co_quan = if org_issuing_raw.is_empty() {
        let m = "[CHƯA NHẬP: TÊN CƠ QUAN BAN HÀNH]".to_string();
        push_missing("ten_co_quan", field_label_vi("ten_co_quan"), &m, "Phần đầu & nội dung");
        m
    } else {
        org_issuing_raw.clone()
    };

    let co_quan_chu_quan = if !fields.co_quan_chu_quan.trim().is_empty() {
        fields.co_quan_chu_quan.trim().to_string()
    } else if let Some(o) = org_opt {
        o.governing_org.trim().to_string()
    } else {
        String::new()
    };

    let viet_tat = if !fields.viet_tat_co_quan.trim().is_empty() {
        fields.viet_tat_co_quan.trim().to_string()
    } else if let Some(o) = org_opt {
        o.agency_abbreviation.trim().to_string()
    } else {
        String::new()
    };

    // 2. Number & Symbol
    let (doc_number, doc_symbol, num_is_missing) =
        split_number_and_symbol(&fields.so_van_ban, tpl, &viet_tat);
    let so_va_ky_hieu = if num_is_missing {
        let m = format!("Số: {}/{}", doc_number, doc_symbol);
        push_missing(
            "so_van_ban",
            field_label_vi("so_van_ban"),
            &m,
            if tpl.header_config.show_number_symbol {
                "Khối Số & Ký hiệu"
            } else {
                "Thông tin tham chiếu"
            },
        );
        format!("{}/{}", doc_number, doc_symbol)
    } else if fields.so_van_ban.contains('/') {
        fields.so_van_ban.trim().to_string()
    } else {
        format!("{}/{}", doc_number, doc_symbol)
    };

    // 3. Location
    let dia_danh_raw = if !fields.dia_danh.trim().is_empty() {
        fields.dia_danh.trim().to_string()
    } else if let Some(o) = org_opt {
        o.default_location.trim().to_string()
    } else {
        String::new()
    };
    let dia_danh = if dia_danh_raw.is_empty() {
        let m = "[CHƯA NHẬP: Địa danh]".to_string();
        if tpl.header_config.show_location_date
            || doc_placeholders.iter().any(|p| p == "dia_danh")
            || tpl.detected_placeholders.iter().any(|p| p == "dia_danh")
        {
            push_missing("dia_danh", field_label_vi("dia_danh"), &m, "Địa danh ban hành");
        }
        m
    } else {
        dia_danh_raw.clone()
    };

    // 4. Date
    let date_mode = HeaderDateMode::ManualDate;
    let mut manual_day = "[..]".to_string();
    let mut manual_month = "[..]".to_string();
    let mut manual_year = "[....]".to_string();

    let ngay_thang_line = if !fields.day.trim().is_empty()
        && !fields.month.trim().is_empty()
        && !fields.year.trim().is_empty()
    {
        manual_day = fields.day.trim().to_string();
        manual_month = fields.month.trim().to_string();
        manual_year = fields.year.trim().to_string();
        format_nd30_date_parts(&manual_day, &manual_month, &manual_year)
    } else if !fields.ngay_thang.trim().is_empty() {
        let raw_dt = fields.ngay_thang.trim();
        if let Some(caps) = vi_date_re().captures(raw_dt) {
            manual_day = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            manual_month = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            manual_year = caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            format_nd30_date_parts(&manual_day, &manual_month, &manual_year)
        } else if let Some(caps) = slash_date_re().captures(raw_dt) {
            manual_day = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            manual_month = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            manual_year = caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            format_nd30_date_parts(&manual_day, &manual_month, &manual_year)
        } else {
            manual_year = raw_dt.to_string();
            raw_dt.to_string()
        }
    } else if !fields.day.trim().is_empty()
        || !fields.month.trim().is_empty()
        || !fields.year.trim().is_empty()
    {
        if !fields.day.trim().is_empty() {
            manual_day = fields.day.trim().to_string();
        }
        if !fields.month.trim().is_empty() {
            manual_month = fields.month.trim().to_string();
        }
        if !fields.year.trim().is_empty() {
            manual_year = fields.year.trim().to_string();
        }
        let s = format_nd30_date_parts(&manual_day, &manual_month, &manual_year);
        push_missing(
            "ngay_thang",
            field_label_vi("ngay_thang"),
            &s,
            "Ngày tháng chưa đủ thông tin",
        );
        s
    } else {
        let s = "ngày [..] tháng [..] năm [....]".to_string();
        push_missing("ngay_thang", field_label_vi("ngay_thang"), &s, "Khối ngày tháng ban hành");
        s
    };

    // 5. Signer, Role, Recipient, Subject
    let chuc_vu = if fields.chuc_vu.trim().is_empty() {
        let m = "[CHƯA NHẬP: CHỨC VỤ NGƯỜI KÝ]".to_string();
        if tpl.signature_config.layout != SignatureLayoutKind::None {
            push_missing("chuc_vu", field_label_vi("chuc_vu"), &m, "Khối chữ ký / Thẩm quyền");
        }
        m
    } else {
        fields.chuc_vu.trim().to_string()
    };

    let nguoi_ky = if fields.nguoi_ky.trim().is_empty() {
        let m = "[CHƯA NHẬP: Họ và tên người ký]".to_string();
        if tpl.signature_config.layout != SignatureLayoutKind::None {
            push_missing("nguoi_ky", field_label_vi("nguoi_ky"), &m, "Khối chữ ký");
        }
        m
    } else {
        fields.nguoi_ky.trim().to_string()
    };

    let nguoi_nhan = if fields.nguoi_nhan.trim().is_empty() {
        let m = "[CHƯA NHẬP: Kính gửi / Nơi nhận]".to_string();
        push_missing("nguoi_nhan", field_label_vi("nguoi_nhan"), &m, "Kính gửi / Nơi nhận");
        m
    } else {
        fields.nguoi_nhan.trim().to_string()
    };

    let trich_yeu = if fields.trich_yeu.trim().is_empty() {
        let m = "[CHƯA NHẬP: Trích yếu nội dung]".to_string();
        push_missing("trich_yeu", field_label_vi("trich_yeu"), &m, "Tiêu đề / Trích yếu văn bản");
        m
    } else {
        fields.trich_yeu.trim().to_string()
    };

    values.insert("ten_co_quan".to_string(), ten_co_quan.clone());
    values.insert("co_quan_chu_quan".to_string(), co_quan_chu_quan.clone());
    values.insert("viet_tat_co_quan".to_string(), viet_tat.clone());
    values.insert("so_van_ban".to_string(), so_va_ky_hieu.clone());
    values.insert("dia_danh".to_string(), dia_danh.clone());
    values.insert("ngay_thang".to_string(), ngay_thang_line.clone());
    values.insert("nguoi_ky".to_string(), nguoi_ky);
    values.insert("chuc_vu".to_string(), chuc_vu);
    values.insert("nguoi_nhan".to_string(), nguoi_nhan);
    values.insert("trich_yeu".to_string(), trich_yeu.clone());
    values.insert("phien_ban".to_string(), tpl.version.clone());
    values.insert("ten_mau".to_string(), tpl.name.clone());

    for (k, v) in &fields.extra_fields {
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        if v.trim().is_empty() {
            let m = format!("[CHƯA NHẬP: {}]", key);
            push_missing(key, key, &m, "Trường động tùy chỉnh");
            values.insert(key.to_string(), m);
        } else {
            values.insert(key.to_string(), v.trim().to_string());
        }
    }

    // Also flag any custom placeholder declared in template/DOCX that isn't filled yet
    let mut all_placeholders = BTreeSet::new();
    for p in &tpl.detected_placeholders {
        all_placeholders.insert(p.clone());
    }
    for p in doc_placeholders {
        all_placeholders.insert(p.clone());
    }
    for p in all_placeholders {
        if !values.contains_key(&p) {
            let m = format!("[CHƯA NHẬP: {}]", p);
            push_missing(&p, &p, &m, "Trường động trong mẫu DOCX");
            values.insert(p, m);
        }
    }

    // Build effective OrganizationProfile for Phase 2 Header Builder
    let mut effective_org = org_opt.cloned().unwrap_or_else(|| OrganizationProfile {
        sector: match tpl.group {
            crate::models::TemplateGroup::Party => OrgSector::PartyOrg,
            crate::models::TemplateGroup::Enterprise => OrgSector::Enterprise,
            _ => OrgSector::StateAgency,
        },
        ..Default::default()
    });
    effective_org.issuing_org = ten_co_quan;
    if !co_quan_chu_quan.is_empty() {
        effective_org.governing_org = co_quan_chu_quan;
    }
    if !viet_tat.is_empty() {
        effective_org.agency_abbreviation = viet_tat;
    }
    effective_org.default_location = dia_danh.clone();

    let show_national_header = if !tpl.header_config.show_national_header {
        false
    } else if tpl.header_config.respect_org_enterprise_layout
        && effective_org.sector == OrgSector::Enterprise
        && !effective_org.use_nd30_layout_for_enterprise
    {
        false
    } else {
        true
    };

    let show_gov = tpl.header_config.show_governing_org
        && !effective_org.governing_org.trim().is_empty()
        && effective_org.show_governing_org_default;

    let show_aff = effective_org.show_affiliated_unit_default
        && !effective_org.affiliated_unit.trim().is_empty();
    let show_dept = effective_org.show_department_default
        && !effective_org.department_or_branch.trim().is_empty();

    let header_cfg = HeaderBuilderConfig {
        org_profile_id: org_opt.map(|o| o.id.clone()),
        doc_type: tpl.header_config.base_admin_doc_type,
        output_mode: OutputMode::NewCopy,
        show_governing_org: show_gov,
        show_issuing_org: tpl.header_config.show_issuing_org,
        show_affiliated_unit: show_aff,
        show_department: show_dept,
        show_org_underline: true,
        show_national_header,
        custom_national_header: String::new(),
        custom_motto: String::new(),
        show_motto_underline: show_national_header,
        show_number_symbol: tpl.header_config.show_number_symbol,
        number_mode: HeaderNumberMode::ManualInput,
        doc_number,
        doc_symbol,
        allow_override_existing_number: true,
        allow_override_existing_org: true,
        show_location_date: tpl.header_config.show_location_date,
        location_override: dia_danh.clone(),
        date_mode,
        manual_day,
        manual_month,
        manual_year,
        allow_override_existing_date: true,
        show_doc_type_and_subject: tpl.header_config.show_doc_type_and_subject,
        custom_doc_type_label: tpl.header_config.custom_doc_type_label.clone(),
        subject_text: trich_yeu,
        summary_line: String::new(),
        classification_mark: String::new(),
        selected_candidate_index: None,
    };

    ResolvedTemplateData {
        values,
        missing_fields: missing,
        effective_org,
        header_cfg,
        so_va_ky_hieu,
        ngay_thang_line,
        dia_danh_line: dia_danh,
    }
}

pub fn render_placeholder_string(input: &str, values: &BTreeMap<String, String>) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }
    placeholder_re()
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let key = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            values
                .get(key)
                .cloned()
                .unwrap_or_else(|| format!("[CHƯA NHẬP: {}]", key))
        })
        .into_owned()
}

pub fn resolve_sections(
    tpl: &DocumentTemplate,
    req: &TemplateApplyRequest,
) -> Vec<TemplateSectionBlock> {
    let explicit_set: Option<BTreeSet<&str>> = if req.enabled_section_ids.is_empty() {
        None
    } else {
        Some(req.enabled_section_ids.iter().map(|s| s.as_str()).collect())
    };
    tpl.content_structure
        .iter()
        .filter(|sec| {
            if sec.required {
                true
            } else if let Some(ref set) = explicit_set {
                set.contains(sec.section_id.as_str())
            } else {
                sec.enabled_by_default
            }
        })
        .cloned()
        .collect()
}

pub fn build_preview_blocks(
    tpl: &DocumentTemplate,
    req: &TemplateApplyRequest,
    resolved: &ResolvedTemplateData,
) -> Vec<TemplatePreviewBlock> {
    let mut blocks = Vec::new();
    for sec in resolve_sections(tpl, req) {
        let mut lines = Vec::new();
        for p in &sec.body_paragraphs {
            lines.push(render_placeholder_string(p, &resolved.values));
        }
        let table_preview = sec.inline_table.as_ref().map(|tb| TemplateTableBlueprint {
            caption: render_placeholder_string(&tb.caption, &resolved.values),
            columns: tb.columns.clone(),
            sample_rows: tb
                .sample_rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|c| render_placeholder_string(c, &resolved.values))
                        .collect()
                })
                .collect(),
        });
        blocks.push(TemplatePreviewBlock {
            role: format!("{:?}", sec.kind),
            title: render_placeholder_string(&sec.title, &resolved.values),
            lines,
            table_preview,
        });
    }
    blocks
}

pub fn build_signature_preview_columns(
    tpl: &DocumentTemplate,
    resolved: &ResolvedTemplateData,
) -> Vec<Vec<String>> {
    let signer_title =
        render_placeholder_string(&tpl.signature_config.signer_title_placeholder, &resolved.values)
            .to_uppercase();
    let signer_name =
        render_placeholder_string(&tpl.signature_config.signer_name_placeholder, &resolved.values);

    match tpl.signature_config.layout {
        SignatureLayoutKind::None => Vec::new(),
        SignatureLayoutKind::SingleRight => {
            let mut right = Vec::new();
            if !tpl.signature_config.authority_prefix_label.trim().is_empty() {
                right.push(tpl.signature_config.authority_prefix_label.trim().to_uppercase());
            }
            right.push(signer_title);
            right.push("(Ký, ghi rõ họ tên và đóng dấu)".to_string());
            right.push(signer_name);
            vec![right]
        }
        SignatureLayoutKind::DualSecretaryChair => {
            let left_title = if tpl.signature_config.left_column_title.trim().is_empty() {
                "THƯ KÝ".to_string()
            } else {
                tpl.signature_config.left_column_title.trim().to_uppercase()
            };
            let right_title = if signer_title.contains("CHƯA NHẬP") {
                "CHỦ TỌA / CHỦ TRÌ".to_string()
            } else {
                signer_title
            };
            vec![
                vec![
                    left_title,
                    "(Ký, ghi rõ họ tên)".to_string(),
                    "[Họ và tên thư ký]".to_string(),
                ],
                vec![right_title, "(Ký, ghi rõ họ tên)".to_string(), signer_name],
            ]
        }
        SignatureLayoutKind::TripleApproval => {
            let left_title = if tpl.signature_config.left_column_title.trim().is_empty() {
                "NGƯỜI ĐỀ XUẤT".to_string()
            } else {
                tpl.signature_config.left_column_title.trim().to_uppercase()
            };
            let mid_title = if tpl.signature_config.middle_column_title.trim().is_empty() {
                "THẨM ĐỊNH".to_string()
            } else {
                tpl.signature_config.middle_column_title.trim().to_uppercase()
            };
            vec![
                vec![left_title, "(Ký, họ tên)".to_string()],
                vec![mid_title, "(Ý kiến & ký)".to_string()],
                vec![signer_title, "(Phê duyệt & ký)".to_string(), signer_name],
            ]
        }
    }
}

pub fn build_new_docx_package_from_template(
    tpl: &DocumentTemplate,
    req: &TemplateApplyRequest,
    resolved: &ResolvedTemplateData,
    raw_master_mode: bool,
    logs: &mut Vec<String>,
) -> Result<OpenXmlPackage, String> {
    let mut pkg = build_empty_valid_docx_package();

    // Update section dimensions & styles from template formatRules before building header
    configure_new_package_section_and_styles(&mut pkg, &tpl.format_rules)?;

    // Choose values map: literal `{{key}}` if exporting editable Word master template, else resolved values
    let literal_values = BTreeMap::new();
    let val_map = if raw_master_mode {
        &literal_values
    } else {
        &resolved.values
    };

    if tpl.header_config.enabled {
        let mut h_warnings = Vec::new();
        apply_header_to_package(
            &mut pkg,
            &resolved.header_cfg,
            HeaderActionMode::CreateNew,
            Some(&resolved.effective_org),
            logs,
            &mut h_warnings,
        )?;
        for w in h_warnings {
            logs.push(format!("[Header Builder] {}", w));
        }
    }

    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Thiếu word/document.xml khi tạo tài liệu từ mẫu.".to_string())?;
    let mut doc = XmlDocument::parse(&doc_xml)?;
    let printable_twips = read_body_printable_width(&doc.root);

    {
        let body = doc
            .root
            .child_mut("w:body")
            .ok_or_else(|| "Tài liệu không có w:body.".to_string())?;

        // Detach trailing w:sectPr safely so all new blocks stay inside section 1
        let sect_pr_pos = body
            .children
            .iter()
            .rposition(|n| matches!(n, XmlNode::Element(e) if e.name == "w:sectPr"));
        let preserved_sect = sect_pr_pos.map(|pos| match body.children.remove(pos) {
            XmlNode::Element(e) => e,
            _ => unreachable!(),
        });

        // Remove the single initial empty filler paragraph seeded by build_empty_valid_docx_package
        body.children.retain(|n| match n {
            XmlNode::Element(el) if el.name == "w:p" => {
                let txt = el.extract_text();
                let is_divider = el
                    .child("w:pPr")
                    .and_then(|pp| pp.child("w:pBdr"))
                    .is_some();
                !txt.trim().is_empty() || is_divider
            }
            _ => true,
        });

        // Optional document control box (e.g., SOP Quy trình nghiệp vụ)
        if tpl.header_config.include_control_box {
            let control_tbl = build_sop_control_box_table(
                tpl,
                resolved,
                raw_master_mode,
                printable_twips,
                &tpl.table_config,
            );
            body.children.push(XmlNode::Element(control_tbl));
            body.children
                .push(XmlNode::Element(make_spacer_para(&tpl.format_rules.font_family, 100)));
        }

        // Append structured sections in exact order
        let sections = resolve_sections(tpl, req);
        for sec in &sections {
            append_section_to_body(
                body,
                sec,
                &tpl.format_rules,
                &tpl.table_config,
                printable_twips,
                val_map,
                raw_master_mode,
            );
        }

        // Ending block: Nơi nhận + Signature layout table
        let show_footer_recipients = tpl.footer_config.show_recipient_list;
        let has_signature = tpl.signature_config.layout != SignatureLayoutKind::None;
        if show_footer_recipients || has_signature {
            body.children
                .push(XmlNode::Element(make_spacer_para(&tpl.format_rules.font_family, 120)));
            let sig_tbl = build_ending_and_signature_table(
                tpl,
                resolved,
                printable_twips,
                raw_master_mode,
            );
            body.children.push(XmlNode::Element(sig_tbl));
        }

        if let Some(sect) = preserved_sect {
            body.children.push(XmlNode::Element(sect));
        }
    }

    let mut doc_xml_out = doc.to_xml_string()?;
    if raw_master_mode {
        doc_xml_out = doc_xml_out.replace("ngày [..] tháng [..] năm {{ngay_thang}}", "{{ngay_thang}}");
    }
    pkg.set_xml("word/document.xml", doc_xml_out);

    // Apply page numbers / footer note via inherited Phase 0 Page & TOC engine
    let footer_note = if raw_master_mode {
        tpl.footer_config.footer_note_template.clone()
    } else {
        render_placeholder_string(&tpl.footer_config.footer_note_template, &resolved.values)
    };
    if tpl.footer_config.include_page_numbers || !footer_note.trim().is_empty() {
        let mut pt_cfg = PageTocConfig::default();
        pt_cfg.page_numbers_enabled = tpl.footer_config.include_page_numbers;
        pt_cfg.page_num_position = tpl.footer_config.page_number_position;
        pt_cfg.skip_first_page = true;
        pt_cfg.footer_text = footer_note;
        pt_cfg.orientation_mode = OrientationMode::Keep;
        pt_cfg.auto_swap_binding_margin = false;

        let fmt_cfg = template_format_to_format_config(&tpl.format_rules);
        let _ = apply_page_and_toc(&mut pkg, &pt_cfg, &fmt_cfg, logs)?;
    }

    logs.push(format!(
        "Đã khởi tạo tài liệu DOCX hoàn chỉnh theo mẫu '{}' (loại: {}).",
        tpl.name, tpl.doc_type
    ));
    Ok(pkg)
}

pub fn apply_template_to_existing_package(
    pkg: &mut OpenXmlPackage,
    tpl: &DocumentTemplate,
    req: &TemplateApplyRequest,
    resolved: &ResolvedTemplateData,
    apply_header_and_framing: bool,
    logs: &mut Vec<String>,
) -> Result<(), String> {
    if !pkg.is_word() {
        return Err("Gói tài liệu không phải định dạng Word OpenXML hợp lệ.".to_string());
    }

    // Step A: Run-coalesced placeholder substitution across document.xml & all header/footer parts
    // Preserves w:drawing, w:tbl, bookmarks, hyperlinks, footnotes, comments, vbaProject.bin!
    let mut replaced_count = substitute_placeholders_in_package(pkg, &resolved.values)?;
    if replaced_count > 0 {
        logs.push(format!(
            "Đã điền {} cụm trường động {{{{...}}}} trực tiếp trên OpenXML (giữ nguyên định dạng, hình ảnh, bảng).",
            replaced_count
        ));
    }

    // Step B: When applying a library template onto an existing user document (rather than a pre-built master DOCX),
    // upgrade/standardize header via Phase 2 without deleting body tables/logos/sections
    if apply_header_and_framing && tpl.header_config.enabled {
        let mut h_warnings = Vec::new();
        let inspection = inspect_package_header(
            std::path::Path::new("target.docx"),
            pkg,
            &resolved.header_cfg,
            Some(&resolved.effective_org),
        )?;

        if inspection.candidates.is_empty() {
            apply_header_to_package(
                pkg,
                &resolved.header_cfg,
                HeaderActionMode::CreateNew,
                Some(&resolved.effective_org),
                logs,
                &mut h_warnings,
            )?;
            logs.push("Đã bổ sung phần đầu văn bản chuẩn cho tài liệu gốc (nội dung, bảng và ảnh gốc được bảo toàn).".to_string());
        } else {
            let idx = inspection.active_candidate_index.unwrap_or(0);
            let is_custom_logo = inspection
                .candidates
                .get(idx)
                .map(|c| {
                    c.layout_kind == crate::models::HeaderLayoutKind::CustomTemplateWithLogo
                })
                .unwrap_or(false);
            if is_custom_logo {
                logs.push(
                    "Phần đầu tài liệu gốc có logo/thiết kế tùy biến — hệ thống giữ nguyên hoàn toàn khối đầu trang gốc."
                        .to_string(),
                );
            } else {
                let mut h_cfg = resolved.header_cfg.clone();
                h_cfg.selected_candidate_index = Some(idx);
                if let Err(e) = apply_header_to_package(
                    pkg,
                    &h_cfg,
                    HeaderActionMode::StandardizeExisting,
                    Some(&resolved.effective_org),
                    logs,
                    &mut h_warnings,
                ) {
                    logs.push(format!("Giữ nguyên phần đầu tài liệu gốc ({})", e));
                }
            }
        }
        for w in h_warnings {
            logs.push(format!("[Header] {}", w));
        }

        // Optionally insert framing signature/recipients only if missing at tail
        if req.insert_framing_on_existing
            && tpl.signature_config.layout != SignatureLayoutKind::None
        {
            let inserted = maybe_append_signature_if_missing(pkg, tpl, resolved)?;
            if inserted {
                logs.push("Đã bổ sung khối Nơi nhận & Chữ ký theo mẫu vào cuối tài liệu.".to_string());
            }
        }
    }

    // Step C: Apply section margins/size & safe body formatting WITHOUT touching tables, logos, landscape, or existing headers/footers
    if tpl.format_rules.enforce_compatibility_check {
        apply_non_destructive_format_rules(pkg, &tpl.format_rules, logs)?;
    }

    // Step D: Re-run placeholder pass in case framing introduced any tokens
    if apply_header_and_framing {
        let extra = substitute_placeholders_in_package(pkg, &resolved.values)?;
        replaced_count += extra;
    }
    let _ = replaced_count;

    Ok(())
}

pub fn substitute_placeholders_in_package(
    pkg: &mut OpenXmlPackage,
    values: &BTreeMap<String, String>,
) -> Result<usize, String> {
    let target_names: Vec<String> = pkg
        .entries
        .iter()
        .map(|e| e.name.clone())
        .filter(|n| {
            let lower = n.trim_start_matches('/').replace('\\', "/").to_ascii_lowercase();
            lower == "word/document.xml"
                || ((lower.starts_with("word/header") || lower.starts_with("word/footer"))
                    && lower.ends_with(".xml"))
        })
        .collect();

    let mut total_hits = 0usize;
    for part_name in target_names {
        let Some(xml) = pkg.get_xml(&part_name) else {
            continue;
        };
        let Ok(mut doc) = XmlDocument::parse(&xml) else {
            continue;
        };
        let hits = substitute_in_xml_element(&mut doc.root, values);
        if hits > 0 {
            total_hits += hits;
            pkg.set_xml(&part_name, doc.to_xml_string()?);
        }
    }
    Ok(total_hits)
}

fn substitute_in_xml_element(el: &mut XmlElement, values: &BTreeMap<String, String>) -> usize {
    if el.name == "w:p" {
        let mut hits = substitute_in_paragraph_children(&mut el.children, values);
        // Also inspect hyperlinks or smartTags inside w:p without touching drawings/fields
        for child in &mut el.children {
            if let XmlNode::Element(sub) = child {
                if sub.name == "w:hyperlink" || sub.name == "w:sdtContent" {
                    hits += substitute_in_paragraph_children(&mut sub.children, values);
                }
            }
        }
        return hits;
    }

    let mut total = 0usize;
    for child in &mut el.children {
        if let XmlNode::Element(sub) = child {
            total += substitute_in_xml_element(sub, values);
        }
    }
    total
}

fn is_pure_text_run(node: &XmlNode) -> bool {
    let XmlNode::Element(el) = node else {
        return false;
    };
    if el.name != "w:r" {
        return false;
    }
    let mut has_t = false;
    for child in &el.children {
        let XmlNode::Element(sub) = child else {
            continue;
        };
        match sub.name.as_str() {
            "w:rPr" => {}
            "w:t" => has_t = true,
            _ => return false,
        }
    }
    has_t
}

fn is_ignorable_ws_node(node: &XmlNode) -> bool {
    matches!(node, XmlNode::Text(s) if s.chars().all(|c| c.is_whitespace()))
}

fn substitute_in_paragraph_children(
    children: &mut Vec<XmlNode>,
    values: &BTreeMap<String, String>,
) -> usize {
    let mut hits = 0usize;
    let mut remove_indices = Vec::new();
    let mut i = 0usize;

    while i < children.len() {
        if !is_pure_text_run(&children[i]) {
            i += 1;
            continue;
        }
        let mut group_run_indices = Vec::new();
        while i < children.len() {
            if is_pure_text_run(&children[i]) {
                group_run_indices.push(i);
                i += 1;
            } else if is_ignorable_ws_node(&children[i]) {
                i += 1;
            } else {
                break;
            }
        }
        if group_run_indices.is_empty() {
            continue;
        }

        let mut segments: Vec<String> = group_run_indices
            .iter()
            .map(|&idx| match &children[idx] {
                XmlNode::Element(r) => r.extract_text(),
                _ => String::new(),
            })
            .collect();

        let combined: String = segments.concat();
        if !combined.contains("{{") {
            continue;
        }

        let matches: Vec<(usize, usize, String)> = placeholder_re()
            .captures_iter(&combined)
            .filter_map(|caps| {
                let m = caps.get(0)?;
                let key = caps.get(1)?.as_str().trim().to_string();
                Some((m.start(), m.end(), key))
            })
            .collect();
        if matches.is_empty() {
            continue;
        }

        let mut starts = Vec::with_capacity(segments.len());
        let mut cursor = 0usize;
        for seg in &segments {
            starts.push(cursor);
            cursor += seg.len();
        }

        // Apply matches right-to-left so earlier byte offsets remain invariant
        for (m_start, m_end, key) in matches.into_iter().rev() {
            let repl = values
                .get(&key)
                .cloned()
                .unwrap_or_else(|| format!("[CHƯA NHẬP: {}]", key));

            let first_seg = starts
                .iter()
                .rposition(|&s| s <= m_start)
                .unwrap_or(0);
            let last_seg = starts
                .iter()
                .rposition(|&s| s < m_end)
                .unwrap_or(first_seg);

            let local_start = m_start - starts[first_seg];
            let local_end = m_end - starts[last_seg];

            if first_seg == last_seg {
                let updated = format!(
                    "{}{}{}",
                    &segments[first_seg][..local_start],
                    repl,
                    &segments[first_seg][local_end..]
                );
                segments[first_seg] = updated;
            } else {
                let suffix = segments[last_seg][local_end..].to_string();
                let prefix = segments[first_seg][..local_start].to_string();
                segments[first_seg] = format!("{}{}", prefix, repl);
                for mid in &mut segments[(first_seg + 1)..last_seg] {
                    mid.clear();
                }
                segments[last_seg] = suffix;
            }
            hits += 1;
        }

        for (offset, seg_text) in segments.into_iter().enumerate() {
            let child_idx = group_run_indices[offset];
            if seg_text.is_empty() && offset > 0 {
                remove_indices.push(child_idx);
                continue;
            }
            if let XmlNode::Element(ref mut r_el) = children[child_idx] {
                r_el.children.retain(|n| matches!(n, XmlNode::Element(e) if e.name == "w:rPr"));
                let mut t = XmlElement::new("w:t");
                ensure_wt_space_preserve(&mut t, &seg_text);
                r_el.children.push(XmlNode::Element(t));
            }
        }
    }

    remove_indices.sort_unstable();
    remove_indices.dedup();
    for idx in remove_indices.into_iter().rev() {
        children.remove(idx);
    }

    hits
}

fn configure_new_package_section_and_styles(
    pkg: &mut OpenXmlPackage,
    rules: &TemplateFormatRules,
) -> Result<(), String> {
    if let Some(doc_xml) = pkg.get_xml("word/document.xml") {
        let mut doc = XmlDocument::parse(&doc_xml)?;
        if let Some(body) = doc.root.child_mut("w:body") {
            if let Some(sect_pr) = body.child_mut("w:sectPr") {
                apply_rules_to_portrait_sect_pr(sect_pr, rules);
            }
        }
        pkg.set_xml("word/document.xml", doc.to_xml_string()?);
    }

    if let Some(styles_xml) = pkg.get_xml("word/styles.xml") {
        let mut s_doc = XmlDocument::parse(&styles_xml)?;
        if let Some(r_pr) = s_doc
            .root
            .child_mut("w:docDefaults")
            .and_then(|d| d.child_mut("w:rPrDefault"))
            .and_then(|d| d.child_mut("w:rPr"))
        {
            set_run_font_and_size(r_pr, &rules.font_family, Some(rules.body_font_size_pt));
        }
        pkg.set_xml("word/styles.xml", s_doc.to_xml_string()?);
    }

    Ok(())
}

fn apply_rules_to_portrait_sect_pr(sect_pr: &mut XmlElement, rules: &TemplateFormatRules) {
    let is_landscape = sect_pr
        .child("w:pgSz")
        .and_then(|s| s.get_attr("w:orient"))
        .map(|o| o.eq_ignore_ascii_case("landscape"))
        .unwrap_or(false);
    if is_landscape {
        // Keep landscape orientation untouched while ensuring A4 margins fit safely
        let pg_mar = sect_pr.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
        if pg_mar.get_attr("w:top").is_none() {
            pg_mar.set_attr("w:top", &cm_to_twips(rules.margin_left_mm / 10.0).to_string());
        }
        return;
    }

    let (w, h) = paper_dimensions_twips(rules.paper_size);
    let pg_sz = sect_pr.ensure_child_ordered("w:pgSz", SECT_PR_ORDER);
    pg_sz.set_attr("w:w", &w.to_string());
    pg_sz.set_attr("w:h", &h.to_string());

    let pg_mar = sect_pr.ensure_child_ordered("w:pgMar", SECT_PR_ORDER);
    pg_mar.set_attr("w:top", &cm_to_twips(rules.margin_top_mm / 10.0).to_string());
    pg_mar.set_attr("w:bottom", &cm_to_twips(rules.margin_bottom_mm / 10.0).to_string());
    pg_mar.set_attr("w:left", &cm_to_twips(rules.margin_left_mm / 10.0).to_string());
    pg_mar.set_attr("w:right", &cm_to_twips(rules.margin_right_mm / 10.0).to_string());
    if pg_mar.get_attr("w:header").is_none() {
        pg_mar.set_attr("w:header", "709");
    }
    if pg_mar.get_attr("w:footer").is_none() {
        pg_mar.set_attr("w:footer", "709");
    }
}

fn apply_non_destructive_format_rules(
    pkg: &mut OpenXmlPackage,
    rules: &TemplateFormatRules,
    logs: &mut Vec<String>,
) -> Result<(), String> {
    let Some(view) = pkg.to_selective_view().ok() else {
        return Ok(());
    };
    let Ok(doc_ir) = DocumentIr::build(std::path::Path::new("working.docx"), &view, crate::models::StdDocScope::AutoDetect, None) else {
        return Ok(());
    };

    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Thiếu word/document.xml".to_string())?;
    let mut doc = XmlDocument::parse(&doc_xml)?;

    let body_indices: BTreeSet<usize> = doc_ir
        .paragraphs
        .iter()
        .filter(|p| {
            p.in_table_idx.is_none()
                && !p.has_drawing
                && matches!(
                    p.role,
                    crate::engine::std_engine::ir::ParagraphRole::BodyProse
                )
                && p.effective_jc != "center"
        })
        .filter_map(|p| {
            p.element_path
                .strip_prefix("w:body/")
                .and_then(|s| s.split('/').next())
                .and_then(|seg| seg.strip_prefix("w:p["))
                .and_then(|seg| seg.strip_suffix(']'))
                .and_then(|num| num.parse::<usize>().ok())
        })
        .collect();

    if let Some(body) = doc.root.child_mut("w:body") {
        let mut p_ord = 0usize;
        for node in &mut body.children {
            let XmlNode::Element(el) = node else {
                continue;
            };
            if el.name == "w:sectPr" {
                apply_rules_to_portrait_sect_pr(el, rules);
                continue;
            }
            if el.name == "w:p" {
                if let Some(p_pr) = el.child_mut("w:pPr") {
                    if let Some(sect) = p_pr.child_mut("w:sectPr") {
                        apply_rules_to_portrait_sect_pr(sect, rules);
                    }
                }
                if body_indices.contains(&p_ord) {
                    apply_font_and_spacing_to_body_p(el, rules);
                }
                p_ord += 1;
            }
        }
    }

    pkg.set_xml("word/document.xml", doc.to_xml_string()?);
    logs.push(format!(
        "Đã chuẩn hóa lề khổ giấy & {} đoạn lời văn theo quy tắc mẫu (giữ nguyên toàn bộ section ngang, bảng biểu, hình ảnh).",
        body_indices.len()
    ));
    Ok(())
}

fn apply_font_and_spacing_to_body_p(p: &mut XmlElement, rules: &TemplateFormatRules) {
    let p_pr = p.ensure_child_ordered("w:pPr", P_PR_ORDER);
    let sp = p_pr.ensure_child_ordered("w:spacing", P_PR_ORDER);
    sp.set_attr("w:before", &pt_to_twips(rules.space_before_pt).to_string());
    sp.set_attr("w:after", &pt_to_twips(rules.space_after_pt).to_string());
    sp.set_attr("w:line", &line_to_twips(rules.line_spacing_multiple).to_string());
    sp.set_attr("w:lineRule", "auto");

    let jc = p_pr.ensure_child_ordered("w:jc", P_PR_ORDER);
    if rules.body_alignment == "both" || rules.body_alignment == "left" {
        jc.set_attr("w:val", &rules.body_alignment);
    }

    for child in &mut p.children {
        let XmlNode::Element(r) = child else {
            continue;
        };
        if r.name != "w:r" {
            continue;
        }
        let has_special = r.children_elements().any(|c| {
            matches!(
                c.name.as_str(),
                "w:drawing" | "w:pict" | "w:fldChar" | "w:instrText"
            )
        });
        if has_special {
            continue;
        }
        let r_pr = r.ensure_child_ordered("w:rPr", R_PR_ORDER);
        set_run_font_and_size(r_pr, &rules.font_family, Some(rules.body_font_size_pt));
    }
}

fn maybe_append_signature_if_missing(
    pkg: &mut OpenXmlPackage,
    tpl: &DocumentTemplate,
    resolved: &ResolvedTemplateData,
) -> Result<bool, String> {
    let Some(doc_xml) = pkg.get_xml("word/document.xml") else {
        return Ok(false);
    };
    let mut doc = XmlDocument::parse(&doc_xml)?;
    let body_text = doc.root.extract_text().to_lowercase();
    if body_text.contains("nơi nhận:") || body_text.contains("(ký") {
        return Ok(false);
    }
    let printable_twips = read_body_printable_width(&doc.root);
    let body = doc
        .root
        .child_mut("w:body")
        .ok_or_else(|| "Thiếu w:body".to_string())?;

    let sect_pos = body
        .children
        .iter()
        .rposition(|n| matches!(n, XmlNode::Element(e) if e.name == "w:sectPr"));
    let sect_el = sect_pos.map(|pos| match body.children.remove(pos) {
        XmlNode::Element(e) => e,
        _ => unreachable!(),
    });

    body.children
        .push(XmlNode::Element(make_spacer_para(&tpl.format_rules.font_family, 120)));
    let sig_tbl = build_ending_and_signature_table(tpl, resolved, printable_twips, false);
    body.children.push(XmlNode::Element(sig_tbl));

    if let Some(sect) = sect_el {
        body.children.push(XmlNode::Element(sect));
    }
    pkg.set_xml("word/document.xml", doc.to_xml_string()?);
    Ok(true)
}

fn read_body_printable_width(doc_root: &XmlElement) -> i32 {
    let Some(body) = doc_root.child("w:body") else {
        return 9071;
    };
    for node in &body.children {
        if let XmlNode::Element(el) = node {
            let sect = if el.name == "w:sectPr" {
                Some(el)
            } else if el.name == "w:p" {
                el.child("w:pPr").and_then(|pp| pp.child("w:sectPr"))
            } else {
                None
            };
            if let Some(s) = sect {
                let w = s
                    .child("w:pgSz")
                    .and_then(|pg| pg.get_attr("w:w"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(11906);
                let l = s
                    .child("w:pgMar")
                    .and_then(|pg| pg.get_attr("w:left"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(1701);
                let r = s
                    .child("w:pgMar")
                    .and_then(|pg| pg.get_attr("w:right"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(1134);
                return (w - l - r).max(5000);
            }
        }
    }
    9071
}

fn append_section_to_body(
    body: &mut XmlElement,
    sec: &TemplateSectionBlock,
    fmt: &TemplateFormatRules,
    tbl_cfg: &TemplateTableConfig,
    printable_twips: i32,
    val_map: &BTreeMap<String, String>,
    raw_master_mode: bool,
) {
    let render_txt = |s: &str| -> String {
        if raw_master_mode {
            s.to_string()
        } else {
            render_placeholder_string(s, val_map)
        }
    };

    if !sec.title.trim().is_empty() {
        let mut title_txt = render_txt(sec.title.trim());
        if sec.heading_uppercase {
            title_txt = title_txt.to_uppercase();
        }
        let jc = match sec.heading_alignment.as_str() {
            "center" | "right" | "both" | "left" => sec.heading_alignment.as_str(),
            _ => "left",
        };
        let p = make_formatted_para(
            &title_txt,
            &fmt.font_family,
            fmt.body_font_size_pt,
            sec.heading_bold,
            sec.heading_italic,
            jc,
            0,
            100,
            60,
            line_to_twips(fmt.line_spacing_multiple),
        );
        body.children.push(XmlNode::Element(p));
    }

    for line in &sec.body_paragraphs {
        let text = render_txt(line);
        let (jc, bold, italic, indent_twips, before_twips, after_twips) = match sec.kind {
            TemplateBlockKind::RecipientSalutation => ("center", false, false, 0, 80, 100),
            TemplateBlockKind::AuthorityHeading => ("center", true, false, 0, 80, 40),
            TemplateBlockKind::PreambleBasis => {
                ("both", false, true, cm_to_twips(fmt.first_line_indent_cm), 0, 60)
            }
            TemplateBlockKind::DecisionKeyword => ("center", true, false, 0, 80, 80),
            TemplateBlockKind::MetadataBlock => ("left", false, false, 0, 20, 60),
            TemplateBlockKind::ArticleClause => (
                "both",
                false,
                false,
                cm_to_twips(fmt.first_line_indent_cm),
                40,
                pt_to_twips(fmt.space_after_pt),
            ),
            _ => (
                fmt.body_alignment.as_str(),
                false,
                false,
                cm_to_twips(fmt.first_line_indent_cm),
                pt_to_twips(fmt.space_before_pt),
                pt_to_twips(fmt.space_after_pt),
            ),
        };
        let p = make_formatted_para(
            &text,
            &fmt.font_family,
            fmt.body_font_size_pt,
            bold,
            italic,
            jc,
            indent_twips,
            before_twips,
            after_twips,
            line_to_twips(fmt.line_spacing_multiple),
        );
        body.children.push(XmlNode::Element(p));
    }

    if let Some(ref tb) = sec.inline_table {
        if !tb.caption.trim().is_empty() && tb.caption.trim() != sec.title.trim() {
            let cap = make_formatted_para(
                &render_txt(&tb.caption),
                &fmt.font_family,
                13.0,
                true,
                true,
                "center",
                0,
                80,
                60,
                276,
            );
            body.children.push(XmlNode::Element(cap));
        }
        let tbl_el = build_data_table_xml(tb, tbl_cfg, printable_twips, &render_txt);
        body.children.push(XmlNode::Element(tbl_el));
        body.children
            .push(XmlNode::Element(make_spacer_para(&fmt.font_family, 80)));
    }
}

fn build_data_table_xml<F: Fn(&str) -> String>(
    tb: &TemplateTableBlueprint,
    tbl_cfg: &TemplateTableConfig,
    printable_twips: i32,
    render_txt: &F,
) -> XmlElement {
    let col_count = tb.columns.len().max(1);
    let col_widths = compute_col_widths_twips(&tb.columns, printable_twips);

    let mut tbl = XmlElement::new("w:tbl");
    let mut tbl_pr = XmlElement::new("w:tblPr");

    let mut tbl_w = XmlElement::new("w:tblW");
    tbl_w.set_attr("w:w", &printable_twips.to_string());
    tbl_w.set_attr("w:type", "dxa");
    tbl_pr.insert_child_ordered(tbl_w, TBL_PR_ORDER);

    let mut tbl_jc = XmlElement::new("w:jc");
    tbl_jc.set_attr("w:val", "center");
    tbl_pr.insert_child_ordered(tbl_jc, TBL_PR_ORDER);

    let mut borders = XmlElement::new("w:tblBorders");
    let b_sz = tbl_cfg.border_size_eighths_pt.max(4).to_string();
    let b_val = if tbl_cfg.border_style.is_empty() {
        "single"
    } else {
        tbl_cfg.border_style.as_str()
    };
    for side in [
        "w:top",
        "w:left",
        "w:bottom",
        "w:right",
        "w:insideH",
        "w:insideV",
    ] {
        let mut b = XmlElement::new(side);
        b.set_attr("w:val", b_val);
        b.set_attr("w:sz", &b_sz);
        b.set_attr("w:space", "0");
        b.set_attr("w:color", "000000");
        borders.children.push(XmlNode::Element(b));
    }
    tbl_pr.insert_child_ordered(borders, TBL_PR_ORDER);

    let mut tbl_layout = XmlElement::new("w:tblLayout");
    tbl_layout.set_attr("w:type", "fixed");
    tbl_pr.insert_child_ordered(tbl_layout, TBL_PR_ORDER);

    let mut cell_mar = XmlElement::new("w:tblCellMar");
    let tb_mar = tbl_cfg.cell_padding_top_bottom_twips.to_string();
    let lr_mar = tbl_cfg.cell_padding_left_right_twips.to_string();
    for (edge, val) in [
        ("w:top", tb_mar.as_str()),
        ("w:left", lr_mar.as_str()),
        ("w:bottom", tb_mar.as_str()),
        ("w:right", lr_mar.as_str()),
    ] {
        let mut m = XmlElement::new(edge);
        m.set_attr("w:w", val);
        m.set_attr("w:type", "dxa");
        cell_mar.insert_child_ordered(m, TBL_CELL_MAR_ORDER);
    }
    tbl_pr.insert_child_ordered(cell_mar, TBL_PR_ORDER);

    tbl.children.push(XmlNode::Element(tbl_pr));

    let mut grid = XmlElement::new("w:tblGrid");
    for w in &col_widths {
        let mut gc = XmlElement::new("w:gridCol");
        gc.set_attr("w:w", &w.to_string());
        grid.children.push(XmlNode::Element(gc));
    }
    tbl.children.push(XmlNode::Element(grid));

    // Header Row
    let mut header_tr = XmlElement::new("w:tr");
    let mut header_tr_pr = XmlElement::new("w:trPr");
    if tbl_cfg.prevent_row_split {
        header_tr_pr.insert_child_ordered(XmlElement::new("w:cantSplit"), TR_PR_ORDER);
    }
    if tbl_cfg.repeat_header_row {
        header_tr_pr.insert_child_ordered(XmlElement::new("w:tblHeader"), TR_PR_ORDER);
    }
    header_tr.children.push(XmlNode::Element(header_tr_pr));

    for idx in 0..col_count {
        let header_txt = tb
            .columns
            .get(idx)
            .map(|c| render_txt(&c.header))
            .unwrap_or_default();
        let mut tc = XmlElement::new("w:tc");
        let mut tc_pr = XmlElement::new("w:tcPr");
        let mut tc_w = XmlElement::new("w:tcW");
        tc_w.set_attr("w:w", &col_widths[idx].to_string());
        tc_w.set_attr("w:type", "dxa");
        tc_pr.insert_child_ordered(tc_w, TC_PR_ORDER);

        let mut shd = XmlElement::new("w:shd");
        shd.set_attr("w:val", "clear");
        shd.set_attr("w:color", "auto");
        shd.set_attr("w:fill", "F1F5F9");
        tc_pr.insert_child_ordered(shd, TC_PR_ORDER);

        let mut v_align = XmlElement::new("w:vAlign");
        v_align.set_attr("w:val", "center");
        tc_pr.insert_child_ordered(v_align, TC_PR_ORDER);
        tc.children.push(XmlNode::Element(tc_pr));

        let p = make_formatted_para(
            &header_txt,
            &tbl_cfg.default_font_name,
            tbl_cfg.default_font_size_pt,
            tbl_cfg.header_bold,
            false,
            "center",
            0,
            40,
            40,
            240,
        );
        tc.children.push(XmlNode::Element(p));
        header_tr.children.push(XmlNode::Element(tc));
    }
    tbl.children.push(XmlNode::Element(header_tr));

    // Data rows (ensure at least 1 placeholder row if sample_rows is empty)
    let rows_src: Vec<Vec<String>> = if tb.sample_rows.is_empty() {
        vec![vec![String::new(); col_count]]
    } else {
        tb.sample_rows.clone()
    };

    for row in rows_src {
        let mut tr = XmlElement::new("w:tr");
        let mut tr_pr = XmlElement::new("w:trPr");
        if tbl_cfg.prevent_row_split {
            tr_pr.insert_child_ordered(XmlElement::new("w:cantSplit"), TR_PR_ORDER);
        }
        tr.children.push(XmlNode::Element(tr_pr));

        for idx in 0..col_count {
            let cell_val = row.get(idx).map(|s| render_txt(s)).unwrap_or_default();
            let jc = tb
                .columns
                .get(idx)
                .map(|c| match c.align.as_str() {
                    "center" | "right" | "both" => c.align.as_str(),
                    _ => "left",
                })
                .unwrap_or("left");

            let mut tc = XmlElement::new("w:tc");
            let mut tc_pr = XmlElement::new("w:tcPr");
            let mut tc_w = XmlElement::new("w:tcW");
            tc_w.set_attr("w:w", &col_widths[idx].to_string());
            tc_w.set_attr("w:type", "dxa");
            tc_pr.insert_child_ordered(tc_w, TC_PR_ORDER);
            tc.children.push(XmlNode::Element(tc_pr));

            let p = make_formatted_para(
                &cell_val,
                &tbl_cfg.default_font_name,
                tbl_cfg.default_font_size_pt,
                false,
                false,
                jc,
                0,
                40,
                40,
                240,
            );
            tc.children.push(XmlNode::Element(p));
            tr.children.push(XmlNode::Element(tc));
        }
        tbl.children.push(XmlNode::Element(tr));
    }

    tbl
}

fn build_sop_control_box_table(
    tpl: &DocumentTemplate,
    resolved: &ResolvedTemplateData,
    raw_master_mode: bool,
    printable_twips: i32,
    tbl_cfg: &TemplateTableConfig,
) -> XmlElement {
    let so_txt = if raw_master_mode {
        "Mã tài liệu: {{so_van_ban}}".to_string()
    } else {
        format!("Mã tài liệu: {}", resolved.so_va_ky_hieu)
    };
    let ver_txt = format!("Phiên bản: v{}", tpl.version);
    let org_txt = if raw_master_mode {
        "Đơn vị ban hành: {{ten_co_quan}}".to_string()
    } else {
        format!(
            "Đơn vị ban hành: {}",
            resolved
                .values
                .get("ten_co_quan")
                .map(|s| s.as_str())
                .unwrap_or("")
        )
    };
    let date_txt = if raw_master_mode {
        "Hiệu lực từ: {{ngay_thang}}".to_string()
    } else {
        format!("Hiệu lực từ: {}", resolved.ngay_thang_line)
    };

    let bp = TemplateTableBlueprint {
        caption: String::new(),
        columns: vec![
            TemplateTableColumn {
                header: "KIỂM SOÁT TÀI LIỆU".to_string(),
                width_pct: 50,
                align: "left".to_string(),
            },
            TemplateTableColumn {
                header: "THÔNG TIN HIỆU LỰC".to_string(),
                width_pct: 50,
                align: "left".to_string(),
            },
        ],
        sample_rows: vec![vec![so_txt, ver_txt], vec![org_txt, date_txt]],
    };
    let render_identity = |s: &str| s.to_string();
    build_data_table_xml(&bp, tbl_cfg, printable_twips, &render_identity)
}

fn build_ending_and_signature_table(
    tpl: &DocumentTemplate,
    resolved: &ResolvedTemplateData,
    printable_twips: i32,
    raw_master_mode: bool,
) -> XmlElement {
    let font = &tpl.format_rules.font_family;
    let render_txt = |s: &str| -> String {
        if raw_master_mode {
            s.to_string()
        } else {
            render_placeholder_string(s, &resolved.values)
        }
    };

    let recipient_lines: Vec<String> = if tpl.footer_config.show_recipient_list {
        tpl.footer_config
            .default_recipients
            .iter()
            .map(|r| render_txt(r))
            .collect()
    } else {
        Vec::new()
    };

    let signer_title = render_txt(&tpl.signature_config.signer_title_placeholder).to_uppercase();
    let signer_name = render_txt(&tpl.signature_config.signer_name_placeholder);

    let columns: Vec<(i32, Vec<(String, f64, bool, bool, &str)>)> = match tpl
        .signature_config
        .layout
    {
        SignatureLayoutKind::DualSecretaryChair => {
            let left_w = printable_twips / 2;
            let right_w = printable_twips - left_w;
            let left_hdr = if tpl.signature_config.left_column_title.trim().is_empty() {
                "THƯ KÝ".to_string()
            } else {
                tpl.signature_config.left_column_title.trim().to_uppercase()
            };
            let right_hdr = if signer_title.contains("CHƯA NHẬP") {
                "CHỦ TỌA / CHỦ TRÌ".to_string()
            } else {
                signer_title
            };
            let mut left_lines = vec![
                (left_hdr, 13.5, true, false, "center"),
                ("(Ký, ghi rõ họ tên)".to_string(), 12.5, false, true, "center"),
            ];
            let mut right_lines = vec![
                (right_hdr, 13.5, true, false, "center"),
                ("(Ký, ghi rõ họ tên)".to_string(), 12.5, false, true, "center"),
            ];
            for _ in 0..tpl.signature_config.blank_lines_for_seal.clamp(2, 5) {
                left_lines.push((String::new(), 13.0, false, false, "center"));
                right_lines.push((String::new(), 13.0, false, false, "center"));
            }
            left_lines.push(("[Họ và tên thư ký]".to_string(), 13.5, true, false, "center"));
            right_lines.push((signer_name, 13.5, true, false, "center"));
            vec![(left_w, left_lines), (right_w, right_lines)]
        }
        SignatureLayoutKind::TripleApproval => {
            let col_w = printable_twips / 3;
            let last_w = printable_twips - col_w * 2;
            let left_hdr = if tpl.signature_config.left_column_title.trim().is_empty() {
                "NGƯỜI ĐỀ XUẤT".to_string()
            } else {
                tpl.signature_config.left_column_title.trim().to_uppercase()
            };
            let mid_hdr = if tpl.signature_config.middle_column_title.trim().is_empty() {
                "THẨM ĐỊNH".to_string()
            } else {
                tpl.signature_config.middle_column_title.trim().to_uppercase()
            };
            let mut c1 = vec![
                (left_hdr, 13.0, true, false, "center"),
                ("(Ký, ghi rõ họ tên)".to_string(), 12.0, false, true, "center"),
            ];
            let mut c2 = vec![
                (mid_hdr, 13.0, true, false, "center"),
                ("(Ý kiến & ký)".to_string(), 12.0, false, true, "center"),
            ];
            let mut c3 = vec![
                (signer_title, 13.0, true, false, "center"),
                ("(Ký duyệt, đóng dấu)".to_string(), 12.0, false, true, "center"),
            ];
            for _ in 0..tpl.signature_config.blank_lines_for_seal.clamp(2, 5) {
                c1.push((String::new(), 13.0, false, false, "center"));
                c2.push((String::new(), 13.0, false, false, "center"));
                c3.push((String::new(), 13.0, false, false, "center"));
            }
            c3.push((signer_name, 13.0, true, false, "center"));
            vec![(col_w, c1), (col_w, c2), (last_w, c3)]
        }
        SignatureLayoutKind::SingleRight | SignatureLayoutKind::None => {
            let left_w = (printable_twips as f64 * 0.45).round() as i32;
            let right_w = printable_twips - left_w;
            let mut left_lines = Vec::new();
            if tpl.footer_config.show_recipient_list {
                let lbl = if tpl.footer_config.recipient_header_label.trim().is_empty() {
                    "Nơi nhận:"
                } else {
                    tpl.footer_config.recipient_header_label.trim()
                };
                left_lines.push((lbl.to_string(), 12.0, true, true, "left"));
                for rec in recipient_lines {
                    left_lines.push((rec, 11.0, false, false, "left"));
                }
            }
            if left_lines.is_empty() {
                left_lines.push((String::new(), 12.0, false, false, "left"));
            }

            let mut right_lines = Vec::new();
            if tpl.signature_config.layout == SignatureLayoutKind::SingleRight {
                if !tpl.signature_config.authority_prefix_label.trim().is_empty() {
                    right_lines.push((
                        tpl.signature_config.authority_prefix_label.trim().to_uppercase(),
                        13.5,
                        true,
                        false,
                        "center",
                    ));
                }
                right_lines.push((signer_title, 13.5, true, false, "center"));
                right_lines.push((
                    "(Ký, ghi rõ họ tên và đóng dấu)".to_string(),
                    12.5,
                    false,
                    true,
                    "center",
                ));
                for _ in 0..tpl.signature_config.blank_lines_for_seal.clamp(2, 5) {
                    right_lines.push((String::new(), 13.0, false, false, "center"));
                }
                right_lines.push((signer_name, 13.5, true, false, "center"));
            } else {
                right_lines.push((String::new(), 12.0, false, false, "center"));
            }
            vec![(left_w, left_lines), (right_w, right_lines)]
        }
    };

    let mut tbl = XmlElement::new("w:tbl");
    let mut tbl_pr = XmlElement::new("w:tblPr");
    let mut tbl_w = XmlElement::new("w:tblW");
    tbl_w.set_attr("w:w", &printable_twips.to_string());
    tbl_w.set_attr("w:type", "dxa");
    tbl_pr.insert_child_ordered(tbl_w, TBL_PR_ORDER);

    let mut borders = XmlElement::new("w:tblBorders");
    for side in [
        "w:top",
        "w:left",
        "w:bottom",
        "w:right",
        "w:insideH",
        "w:insideV",
    ] {
        let mut b = XmlElement::new(side);
        b.set_attr("w:val", "none");
        b.set_attr("w:sz", "0");
        b.set_attr("w:space", "0");
        b.set_attr("w:color", "auto");
        borders.children.push(XmlNode::Element(b));
    }
    tbl_pr.insert_child_ordered(borders, TBL_PR_ORDER);

    let mut tbl_layout = XmlElement::new("w:tblLayout");
    tbl_layout.set_attr("w:type", "fixed");
    tbl_pr.insert_child_ordered(tbl_layout, TBL_PR_ORDER);
    tbl.children.push(XmlNode::Element(tbl_pr));

    let mut grid = XmlElement::new("w:tblGrid");
    for (w, _) in &columns {
        let mut gc = XmlElement::new("w:gridCol");
        gc.set_attr("w:w", &w.to_string());
        grid.children.push(XmlNode::Element(gc));
    }
    tbl.children.push(XmlNode::Element(grid));

    let mut tr = XmlElement::new("w:tr");
    let mut tr_pr = XmlElement::new("w:trPr");
    tr_pr.insert_child_ordered(XmlElement::new("w:cantSplit"), TR_PR_ORDER);
    tr.children.push(XmlNode::Element(tr_pr));

    for (col_w, lines) in columns {
        let mut tc = XmlElement::new("w:tc");
        let mut tc_pr = XmlElement::new("w:tcPr");
        let mut tc_w = XmlElement::new("w:tcW");
        tc_w.set_attr("w:w", &col_w.to_string());
        tc_w.set_attr("w:type", "dxa");
        tc_pr.insert_child_ordered(tc_w, TC_PR_ORDER);
        tc.children.push(XmlNode::Element(tc_pr));

        for (txt, pt, bold, italic, jc) in lines {
            let p = make_formatted_para(&txt, font, pt, bold, italic, jc, 0, 20, 40, 240);
            tc.children.push(XmlNode::Element(p));
        }
        tr.children.push(XmlNode::Element(tc));
    }

    tbl.children.push(XmlNode::Element(tr));
    tbl
}

fn compute_col_widths_twips(cols: &[TemplateTableColumn], printable_twips: i32) -> Vec<i32> {
    let n = cols.len().max(1);
    let total_pct: u32 = cols.iter().map(|c| c.width_pct.max(1)).sum();
    let mut widths = Vec::with_capacity(n);
    let mut acc = 0i32;
    for (i, c) in cols.iter().enumerate() {
        if i + 1 == n {
            widths.push((printable_twips - acc).max(400));
        } else {
            let w = ((printable_twips as f64) * (c.width_pct.max(1) as f64) / (total_pct as f64))
                .round() as i32;
            let clamped = w.max(400);
            acc += clamped;
            widths.push(clamped);
        }
    }
    if widths.is_empty() {
        widths.push(printable_twips);
    }
    widths
}

fn make_spacer_para(font_name: &str, after_twips: u32) -> XmlElement {
    make_formatted_para("", font_name, 13.0, false, false, "left", 0, 0, after_twips, 240)
}

#[allow(clippy::too_many_arguments)]
fn make_formatted_para(
    text: &str,
    font_name: &str,
    pt: f64,
    bold: bool,
    italic: bool,
    jc_val: &str,
    first_line_twips: i32,
    before_twips: u32,
    after_twips: u32,
    line_twips: u32,
) -> XmlElement {
    let mut p = XmlElement::new("w:p");
    let mut p_pr = XmlElement::new("w:pPr");

    let mut sp = XmlElement::new("w:spacing");
    sp.set_attr("w:before", &before_twips.to_string());
    sp.set_attr("w:after", &after_twips.to_string());
    sp.set_attr("w:line", &line_twips.to_string());
    sp.set_attr("w:lineRule", "auto");
    p_pr.insert_child_ordered(sp, P_PR_ORDER);

    let mut ind = XmlElement::new("w:ind");
    ind.set_attr("w:left", "0");
    ind.set_attr("w:right", "0");
    ind.set_attr("w:firstLine", &first_line_twips.max(0).to_string());
    p_pr.insert_child_ordered(ind, P_PR_ORDER);

    let mut jc = XmlElement::new("w:jc");
    jc.set_attr("w:val", jc_val);
    p_pr.insert_child_ordered(jc, P_PR_ORDER);

    p.children.push(XmlNode::Element(p_pr));

    let mut r = XmlElement::new("w:r");
    let mut r_pr = XmlElement::new("w:rPr");
    set_run_font_and_size(&mut r_pr, font_name, Some(pt));
    if bold {
        r_pr.insert_child_ordered(XmlElement::new("w:b"), R_PR_ORDER);
        r_pr.insert_child_ordered(XmlElement::new("w:bCs"), R_PR_ORDER);
    }
    if italic {
        r_pr.insert_child_ordered(XmlElement::new("w:i"), R_PR_ORDER);
        r_pr.insert_child_ordered(XmlElement::new("w:iCs"), R_PR_ORDER);
    }
    let mut color = XmlElement::new("w:color");
    color.set_attr("w:val", "000000");
    r_pr.insert_child_ordered(color, R_PR_ORDER);
    r.children.push(XmlNode::Element(r_pr));

    let mut t = XmlElement::new("w:t");
    ensure_wt_space_preserve(&mut t, text);
    r.children.push(XmlNode::Element(t));
    p.children.push(XmlNode::Element(r));
    p
}

pub fn template_format_to_format_config(rules: &TemplateFormatRules) -> FormatConfig {
    FormatConfig {
        enabled: true,
        font_name: rules.font_family.clone(),
        font_size_pt: rules.body_font_size_pt,
        line_spacing: rules.line_spacing_multiple,
        space_before_pt: rules.space_before_pt,
        space_after_pt: rules.space_after_pt,
        first_line_indent_cm: rules.first_line_indent_cm,
        paper_size: rules.paper_size,
        margin_top_cm: rules.margin_top_mm / 10.0,
        margin_bottom_cm: rules.margin_bottom_mm / 10.0,
        margin_left_cm: rules.margin_left_mm / 10.0,
        margin_right_cm: rules.margin_right_mm / 10.0,
        protect_admin_blocks: true,
        smart_tables: true,
        table_font_size_pt: 13.0,
        repeat_table_headers: true,
        sync_styles_default: true,
    }
}
