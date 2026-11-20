use crate::engine::formatter::{collapse_spaces_upper, estimate_text_line_twips};
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::backup_manager;
use crate::engine::reliability::error_code::{classify_error_message, AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::preservation::{PreservationPolicy, PreservationSnapshot};
use crate::engine::reliability::safe_save;
use crate::engine::{org_profile, system_office};
use crate::engine::xml_helpers::{XmlDocument, XmlElement, XmlNode, P_PR_ORDER, TBL_PR_ORDER};
use crate::models::{
    AdminDocType, AuditSeverity, DetectedHeaderFields, HeaderActionMode, HeaderApplyResult,
    HeaderBuilderConfig, HeaderCandidateSummary, HeaderClassification, HeaderFinding,
    HeaderInspectionResult, HeaderLayoutKind, HeaderNumberMode, HeaderDateMode, HeaderRuleMeta,
    OrgSector, OrganizationProfile, OutputMode, RuleObligation,
};
use chrono::{Datelike, Local};
use regex::Regex;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const STD_NATIONAL_HEADER: &str = "CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM";
const STD_PARTY_HEADER: &str = "ĐẢNG CỘNG SẢN VIỆT NAM";
const STD_MOTTO: &str = "Độc lập - Tự do - Hạnh phúc";

#[derive(Debug, Clone)]
struct CandidateInternal {
    summary: HeaderCandidateSummary,
    preserved_logo_paras: Vec<XmlElement>,
    preserved_sect_pr: Option<XmlElement>,
    typos_or_format_issues: Vec<(String, AuditSeverity, String)>,
}

/// Returns the full catalog of legal & configurable header rules referenced by MonWord.
pub fn get_header_rules_catalog() -> Vec<HeaderRuleMeta> {
    vec![
        HeaderRuleMeta {
            rule_id: "ND30-PLI-01".to_string(),
            title: "Quốc hiệu".to_string(),
            description: "Chữ in hoa \"CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM\", cỡ 12–13pt, kiểu đứng, đậm.".to_string(),
            obligation: RuleObligation::Mandatory,
            scope: "Văn bản hành chính Nhà nước (doanh nghiệp tự chọn mẫu tương thích)".to_string(),
            legal_basis: "Khoản 1 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLI-01B".to_string(),
            title: "Tiêu ngữ & Đường kẻ dưới Tiêu ngữ".to_string(),
            description: "Cụm từ \"Độc lập - Tự do - Hạnh phúc\" in thường, viết hoa chữ cái đầu cụm từ, cỡ 13–14pt đứng đậm; phía dưới có đường kẻ nét liền bằng độ dài dòng chữ.".to_string(),
            obligation: RuleObligation::Mandatory,
            scope: "Văn bản hành chính Nhà nước".to_string(),
            legal_basis: "Khoản 1 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLI-02A".to_string(),
            title: "Tên cơ quan, tổ chức chủ quản trực tiếp".to_string(),
            description: "Chữ in hoa, cỡ 12–13pt, kiểu đứng, KHÔNG in đậm; chỉ trình bày khi cơ quan có cấp chủ quản trực tiếp.".to_string(),
            obligation: RuleObligation::CaseSpecific,
            scope: "Khi cơ quan ban hành trực thuộc cơ quan chủ quản".to_string(),
            legal_basis: "Khoản 2 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLI-02B".to_string(),
            title: "Tên cơ quan, tổ chức ban hành & Đường kẻ phân cách".to_string(),
            description: "Chữ in hoa, cỡ 12–13pt, kiểu đứng, đậm; phía dưới có đường kẻ nét liền dài từ 1/3 đến 1/2 độ dài dòng chữ.".to_string(),
            obligation: RuleObligation::Mandatory,
            scope: "Mọi văn bản chính thức".to_string(),
            legal_basis: "Khoản 2 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ORG-UNIT-02C".to_string(),
            title: "Đơn vị trực thuộc / Phòng, ban, chi nhánh".to_string(),
            description: "Thành phần tùy cấu hình theo hồ sơ tổ chức khi văn bản ban hành ở cấp đơn vị/bộ phận.".to_string(),
            obligation: RuleObligation::OrgConfigurable,
            scope: "Do tổ chức cấu hình trong Organization Profile".to_string(),
            legal_basis: "Khoản 2 Điều 8 & Khoản 2 Mục II Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLI-03".to_string(),
            title: "Số và ký hiệu văn bản".to_string(),
            description: "Từ \"Số:\" in thường 13pt đứng; số nhỏ hơn 10 thêm số 0; ký hiệu chữ in hoa 13pt đứng, không cách quanh dấu / và -.".to_string(),
            obligation: RuleObligation::Mandatory,
            scope: "Văn bản có số phát hành hoặc dự thảo để trống số".to_string(),
            legal_basis: "Khoản 3 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLI-04".to_string(),
            title: "Địa danh và ngày, tháng, năm ban hành".to_string(),
            description: "Chữ in thường, cỡ 13–14pt, kiểu nghiêng; ngày 01–09 và tháng 01–02 thêm số 0; sau địa danh có dấu phẩy.".to_string(),
            obligation: RuleObligation::Mandatory,
            scope: "Mọi văn bản ban hành".to_string(),
            legal_basis: "Khoản 4 Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLI-05A".to_string(),
            title: "Tên loại văn bản & Trích yếu (văn bản có tên loại)".to_string(),
            description: "Tên loại IN HOA 13–14pt đứng đậm canh giữa; trích yếu in thường 13–14pt đứng đậm + đường kẻ nét liền 1/3–1/2 bên dưới.".to_string(),
            obligation: RuleObligation::DocTypeConditional,
            scope: "Quyết định, Tờ trình, Báo cáo, Thông báo, Kế hoạch, Chỉ thị...".to_string(),
            legal_basis: "Khoản 5a Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLI-05B".to_string(),
            title: "Trích yếu Công văn (V/v...)".to_string(),
            description: "Công văn không ghi tên loại; dòng trích yếu bắt đầu bằng \"V/v\" cỡ 12–13pt đứng đặt dưới Số, ký hiệu tại ô bên trái.".to_string(),
            obligation: RuleObligation::DocTypeConditional,
            scope: "Riêng cho loại Công văn".to_string(),
            legal_basis: "Khoản 5b Mục II Phần I Phụ lục I NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "ND30-PLIII-TBL".to_string(),
            title: "Bảng khung 2 cột vô hình giữ tương quan ngang".to_string(),
            description: "Dùng bảng 2 cột không viền cố định độ rộng vừa khổ in để khối Cơ quan – Quốc hiệu và Số – Ngày tháng không lệch nhau.".to_string(),
            obligation: RuleObligation::Recommended,
            scope: "Khuyến nghị chuẩn trình bày Word".to_string(),
            legal_basis: "Phụ lục II & Phụ lục III NĐ 30/2020/NĐ-CP".to_string(),
        },
        HeaderRuleMeta {
            rule_id: "DANG-36HD-01".to_string(),
            title: "Thể thức phần đầu Văn bản Đảng".to_string(),
            description: "Dòng tiêu đề \"ĐẢNG CỘNG SẢN VIỆT NAM\" bên phải; dấu hoa thị (*) dưới tên cấp ủy/tổ chức Đảng.".to_string(),
            obligation: RuleObligation::CaseSpecific,
            scope: "Chỉ áp dụng cho hồ sơ Tổ chức Đảng".to_string(),
            legal_basis: "Hướng dẫn số 36-HD/VPTW".to_string(),
        },
    ]
}

pub fn applicable_rules_for(
    org: Option<&OrganizationProfile>,
    doc_type: AdminDocType,
) -> Vec<HeaderRuleMeta> {
    let sector = org.map(|o| o.sector).unwrap_or(OrgSector::StateAgency);
    let mut rules = get_header_rules_catalog();

    rules.retain(|r| match r.rule_id.as_str() {
        "ND30-PLI-05A" => !doc_type.uses_left_cell_subject(),
        "ND30-PLI-05B" => doc_type.uses_left_cell_subject(),
        "DANG-36HD-01" => sector == OrgSector::PartyOrg,
        _ => true,
    });

    if sector == OrgSector::Enterprise {
        for r in &mut rules {
            if matches!(
                r.rule_id.as_str(),
                "ND30-PLI-01" | "ND30-PLI-01B" | "ND30-PLI-02B" | "ND30-PLI-05A" | "ND30-PLI-05B"
            ) {
                r.obligation = if org.map(|o| o.use_nd30_layout_for_enterprise).unwrap_or(false) {
                    RuleObligation::Recommended
                } else {
                    RuleObligation::OrgConfigurable
                };
            }
        }
    }

    rules
}

fn find_rule_legal_basis(rule_id: &str) -> String {
    get_header_rules_catalog()
        .into_iter()
        .find(|r| r.rule_id == rule_id)
        .map(|r| r.legal_basis)
        .unwrap_or_else(|| "Nghị định 30/2020/NĐ-CP".to_string())
}

fn resolve_org_profile(
    explicit_profile: Option<&OrganizationProfile>,
    cfg: &HeaderBuilderConfig,
) -> Option<OrganizationProfile> {
    if let Some(p) = explicit_profile {
        return Some(p.clone());
    }
    cfg.org_profile_id
        .as_deref()
        .and_then(org_profile::get_org_profile)
}

pub fn classification_label_vi(c: HeaderClassification) -> &'static str {
    match c {
        HeaderClassification::Valid => "Đã hợp lệ",
        HeaderClassification::NeedsStandardization => "Cần chuẩn hóa",
        HeaderClassification::MissingComponents => "Thiếu thành phần",
        HeaderClassification::DataConflict => "Có xung đột dữ liệu",
        HeaderClassification::Uncertain => "Không thể xác định chắc chắn",
    }
}

pub fn inspect_file_header(
    path: &Path,
    cfg: &HeaderBuilderConfig,
    explicit_profile: Option<&OrganizationProfile>,
) -> Result<HeaderInspectionResult, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut temp_docx: Option<PathBuf> = None;
    let working_path = if ext == "doc" {
        let tmp = env::temp_dir().join(format!(
            "monword_hdr_inspect_{}_{}.docx",
            std::process::id(),
            chrono::Local::now().timestamp_millis()
        ));
        system_office::word_com_convert_doc_to_docx(path, &tmp)?;
        temp_docx = Some(tmp.clone());
        tmp
    } else if ext == "docx" || ext == "docm" {
        path.to_path_buf()
    } else {
        return Err(format!(
            "Định dạng .{} không hỗ trợ trình dựng phần đầu (chỉ hỗ trợ .docx, .docm, .doc).",
            ext
        ));
    };

    let pkg_res = OpenXmlPackage::open(&working_path);
    if let Some(tmp) = temp_docx {
        let _ = fs::remove_file(tmp);
    }
    let pkg = pkg_res?;
    if !pkg.is_word() {
        return Err("Tệp không có cấu trúc word/document.xml hợp lệ.".to_string());
    }
    let org = resolve_org_profile(explicit_profile, cfg);
    inspect_package_header(path, &pkg, cfg, org.as_ref())
}

pub fn inspect_package_header(
    path: &Path,
    pkg: &OpenXmlPackage,
    cfg: &HeaderBuilderConfig,
    org: Option<&OrganizationProfile>,
) -> Result<HeaderInspectionResult, String> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document.docx")
        .to_string();

    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Không tìm thấy word/document.xml trong tệp Word.".to_string())?;
    let doc = XmlDocument::parse(&doc_xml)?;
    let body = doc
        .root
        .child("w:body")
        .ok_or_else(|| "Tài liệu không có phần tử w:body.".to_string())?;

    let styles_xml = pkg.get_xml("word/styles.xml");
    let has_header_footer = pkg.entries.iter().any(|e| {
        let l = e.name.to_ascii_lowercase();
        (l.starts_with("word/header") || l.starts_with("word/footer")) && l.ends_with(".xml")
    });
    let has_macros = pkg.has_entry("word/vbaProject.bin") || pkg.has_entry("word/vbaData.xml");
    let section_count = count_sections(body);

    let candidates = detect_header_candidates(body, styles_xml.as_deref(), cfg.doc_type);
    let applicable_rules = applicable_rules_for(org, cfg.doc_type);

    let mut findings: Vec<HeaderFinding> = Vec::new();
    let classification;
    let mut requires_user_choice = false;
    let mut active_candidate_index: Option<usize> = None;

    if candidates.len() > 1 && cfg.selected_candidate_index.is_none() {
        classification = HeaderClassification::Uncertain;
        requires_user_choice = true;
        findings.push(HeaderFinding {
            rule_id: "HDR-MULTI-01".to_string(),
            severity: AuditSeverity::Warning,
            obligation: RuleObligation::CaseSpecific,
            message: format!(
                "Phát hiện {} vị trí có thể là phần đầu văn bản trong cùng tệp. Vui lòng chọn đúng vị trí cần chuẩn hóa để tránh ghi đè nhầm.",
                candidates.len()
            ),
            legal_basis: "Nguyên tắc bảo toàn cấu trúc đa văn bản".to_string(),
        });
    } else if candidates.is_empty() {
        let is_optional_enterprise = org
            .map(|o| o.sector == OrgSector::Enterprise && !o.use_nd30_layout_for_enterprise)
            .unwrap_or(false)
            && !cfg.show_national_header;

        if is_optional_enterprise {
            classification = HeaderClassification::Valid;
            findings.push(HeaderFinding {
                rule_id: "ND30-PLI-01".to_string(),
                severity: AuditSeverity::Info,
                obligation: RuleObligation::OrgConfigurable,
                message: "Văn bản doanh nghiệp không bật Quốc hiệu hành chính; được giữ tự do theo cấu hình tổ chức.".to_string(),
                legal_basis: "Cấu hình nội bộ doanh nghiệp".to_string(),
            });
        } else {
            classification = HeaderClassification::MissingComponents;
            findings.push(HeaderFinding {
                rule_id: "ND30-PLI-01".to_string(),
                severity: AuditSeverity::Warning,
                obligation: if org.map(|o| o.sector == OrgSector::Enterprise).unwrap_or(false) {
                    RuleObligation::OrgConfigurable
                } else {
                    RuleObligation::Mandatory
                },
                message: "Chưa phát hiện khối phần đầu văn bản (Cơ quan ban hành / Quốc hiệu – Tiêu ngữ). Có thể dùng \"Tạo phần đầu mới\".".to_string(),
                legal_basis: find_rule_legal_basis("ND30-PLI-01"),
            });
        }
    } else {
        let chosen_idx = cfg
            .selected_candidate_index
            .unwrap_or(0)
            .min(candidates.len() - 1);
        active_candidate_index = Some(chosen_idx);
        let cand = &candidates[chosen_idx];
        let fields = &cand.summary.fields;

        if cand.summary.layout_kind == HeaderLayoutKind::CustomTemplateWithLogo {
            classification = HeaderClassification::Uncertain;
            requires_user_choice = true;
            findings.push(HeaderFinding {
                rule_id: "HDR-CUSTOM-01".to_string(),
                severity: AuditSeverity::Warning,
                obligation: RuleObligation::CaseSpecific,
                message: "Phần đầu dùng bố cục bảng/banner nhiều cột tùy biến kèm hình ảnh; hệ thống cảnh báo không tự ý sửa khi chưa chắc chắn.".to_string(),
                legal_basis: "Bảo toàn mẫu trình bày riêng".to_string(),
            });
        } else {
            let mut has_conflict = false;
            let mut has_missing = false;
            let mut has_needs_std = false;

            let sector = org.map(|o| o.sector).unwrap_or(OrgSector::StateAgency);
            let enterprise_loose = org
                .map(|o| o.sector == OrgSector::Enterprise && !o.use_nd30_layout_for_enterprise)
                .unwrap_or(false);

            // Check Organization conflict or missing issuing org
            let effective_issuing = org
                .map(|o| o.issuing_org.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| fields.issuing_org.clone());

            if !fields.issuing_org.is_empty() {
                if let Some(o) = org {
                    let doc_org_norm = collapse_spaces_upper(&fields.issuing_org);
                    let prof_org_norm = collapse_spaces_upper(&o.issuing_org);
                    if !prof_org_norm.is_empty()
                        && doc_org_norm != prof_org_norm
                        && !doc_org_norm.contains(&prof_org_norm)
                        && !prof_org_norm.contains(&doc_org_norm)
                        && !cfg.allow_override_existing_org
                    {
                        has_conflict = true;
                        findings.push(HeaderFinding {
                            rule_id: "ORG-CONFLICT".to_string(),
                            severity: AuditSeverity::Error,
                            obligation: RuleObligation::Mandatory,
                            message: format!(
                                "Xung đột tên cơ quan: văn bản gốc ghi \"{}\" khác với hồ sơ \"{}\". Bật xác nhận ghi đè hoặc giữ tên cơ quan gốc.",
                                fields.issuing_org, o.issuing_org
                            ),
                            legal_basis: "Bảo toàn dữ liệu cơ quan ban hành gốc".to_string(),
                        });
                    }
                }
            } else if cfg.show_issuing_org && effective_issuing.is_empty() {
                has_missing = true;
                findings.push(HeaderFinding {
                    rule_id: "ND30-PLI-02B".to_string(),
                    severity: AuditSeverity::Warning,
                    obligation: if enterprise_loose {
                        RuleObligation::OrgConfigurable
                    } else {
                        RuleObligation::Mandatory
                    },
                    message: "Thiếu Tên cơ quan, tổ chức ban hành văn bản và chưa chọn Hồ sơ Tổ chức.".to_string(),
                    legal_basis: find_rule_legal_basis("ND30-PLI-02B"),
                });
            } else if cfg.show_issuing_org && fields.issuing_org.is_empty() && !effective_issuing.is_empty() {
                has_missing = true;
                findings.push(HeaderFinding {
                    rule_id: "ND30-PLI-02B".to_string(),
                    severity: AuditSeverity::Warning,
                    obligation: RuleObligation::Mandatory,
                    message: format!(
                        "Văn bản thiếu tên cơ quan ban hành; sẽ bổ sung \"{}\" từ Hồ sơ Tổ chức.",
                        effective_issuing
                    ),
                    legal_basis: find_rule_legal_basis("ND30-PLI-02B"),
                });
            }

            // Check National Header & Motto
            if cfg.show_national_header && !enterprise_loose {
                let expected_qh = expected_national_header(cfg, sector);
                if fields.national_header.trim().is_empty() {
                    has_missing = true;
                    findings.push(HeaderFinding {
                        rule_id: "ND30-PLI-01".to_string(),
                        severity: AuditSeverity::Warning,
                        obligation: RuleObligation::Mandatory,
                        message: format!("Thiếu dòng Quốc hiệu \"{}\".", expected_qh),
                        legal_basis: find_rule_legal_basis("ND30-PLI-01"),
                    });
                } else if cfg.custom_national_header.trim().is_empty()
                    && fields.national_header.trim() != expected_qh
                {
                    has_needs_std = true;
                    findings.push(HeaderFinding {
                        rule_id: "ND30-PLI-01".to_string(),
                        severity: AuditSeverity::Warning,
                        obligation: RuleObligation::Mandatory,
                        message: format!(
                            "Quốc hiệu \"{}\" chưa đúng viết hoa/chính tả chuẩn \"{}\".",
                            fields.national_header, expected_qh
                        ),
                        legal_basis: find_rule_legal_basis("ND30-PLI-01"),
                    });
                }

                if sector != OrgSector::PartyOrg {
                    let expected_motto = if cfg.custom_motto.trim().is_empty() {
                        STD_MOTTO.to_string()
                    } else {
                        cfg.custom_motto.trim().to_string()
                    };
                    if fields.motto.trim().is_empty() {
                        has_missing = true;
                        findings.push(HeaderFinding {
                            rule_id: "ND30-PLI-01B".to_string(),
                            severity: AuditSeverity::Warning,
                            obligation: RuleObligation::Mandatory,
                            message: format!("Thiếu Tiêu ngữ \"{}\".", expected_motto),
                            legal_basis: find_rule_legal_basis("ND30-PLI-01B"),
                        });
                    } else if cfg.custom_motto.trim().is_empty()
                        && normalize_motto_display(&fields.motto) != STD_MOTTO
                    {
                        has_needs_std = true;
                        findings.push(HeaderFinding {
                            rule_id: "ND30-PLI-01B".to_string(),
                            severity: AuditSeverity::Warning,
                            obligation: RuleObligation::Mandatory,
                            message: format!(
                                "Tiêu ngữ \"{}\" sai quy cách (chuẩn: \"Độc lập - Tự do - Hạnh phúc\").",
                                fields.motto
                            ),
                            legal_basis: find_rule_legal_basis("ND30-PLI-01B"),
                        });
                    } else if cfg.custom_motto.trim().is_empty() && fields.motto.trim() != STD_MOTTO {
                        has_needs_std = true;
                        findings.push(HeaderFinding {
                            rule_id: "ND30-PLI-01B".to_string(),
                            severity: AuditSeverity::Warning,
                            obligation: RuleObligation::Mandatory,
                            message: format!(
                                "Tiêu ngữ \"{}\" chưa đúng chữ hoa/gạch nối chuẩn \"Độc lập - Tự do - Hạnh phúc\".",
                                fields.motto
                            ),
                            legal_basis: find_rule_legal_basis("ND30-PLI-01B"),
                        });
                    }
                }
            }

            // Check conflicts on manual Number / Symbol
            if cfg.number_mode == HeaderNumberMode::ManualInput
                && !cfg.doc_number.trim().is_empty()
                && !fields.number_value.trim().is_empty()
                && fields.number_value.trim() != cfg.doc_number.trim()
                && !cfg.allow_override_existing_number
            {
                has_conflict = true;
                findings.push(HeaderFinding {
                    rule_id: "ND30-PLI-03".to_string(),
                    severity: AuditSeverity::Error,
                    obligation: RuleObligation::Mandatory,
                    message: format!(
                        "Xung đột Số văn bản: tài liệu gốc đang ghi \"{}\" khác số nhập tay \"{}\".",
                        fields.number_raw, cfg.doc_number
                    ),
                    legal_basis: find_rule_legal_basis("ND30-PLI-03"),
                });
            }

            // Check conflicts on Date override
            let has_concrete_doc_date = !fields.day.is_empty() || !fields.month.is_empty();
            if matches!(cfg.date_mode, HeaderDateMode::UseToday | HeaderDateMode::ManualDate | HeaderDateMode::BlankDraft)
                && has_concrete_doc_date
                && !cfg.allow_override_existing_date
            {
                has_conflict = true;
                findings.push(HeaderFinding {
                    rule_id: "ND30-PLI-04".to_string(),
                    severity: AuditSeverity::Error,
                    obligation: RuleObligation::Mandatory,
                    message: format!(
                        "Xung đột ngày ban hành: văn bản gốc đang ghi \"{}\". Cần xác nhận nếu muốn thay bằng ngày mới.",
                        fields.date_raw
                    ),
                    legal_basis: find_rule_legal_basis("ND30-PLI-04"),
                });
            }

            // Missing number / date / underline / typography checks
            if cfg.show_number_symbol && fields.number_raw.trim().is_empty() && !enterprise_loose {
                has_missing = true;
                findings.push(HeaderFinding {
                    rule_id: "ND30-PLI-03".to_string(),
                    severity: AuditSeverity::Info,
                    obligation: RuleObligation::Mandatory,
                    message: "Chưa thấy dòng Số, ký hiệu văn bản (có thể chèn khung Số/ký hiệu).".to_string(),
                    legal_basis: find_rule_legal_basis("ND30-PLI-03"),
                });
            }
            if cfg.show_location_date && fields.date_raw.trim().is_empty() && !enterprise_loose {
                has_missing = true;
                findings.push(HeaderFinding {
                    rule_id: "ND30-PLI-04".to_string(),
                    severity: AuditSeverity::Info,
                    obligation: RuleObligation::Mandatory,
                    message: "Chưa thấy dòng Địa danh và ngày tháng năm.".to_string(),
                    legal_basis: find_rule_legal_basis("ND30-PLI-04"),
                });
            }

            if cand.summary.layout_kind != HeaderLayoutKind::TableLayout {
                has_needs_std = true;
                findings.push(HeaderFinding {
                    rule_id: "ND30-PLIII-TBL".to_string(),
                    severity: AuditSeverity::Info,
                    obligation: RuleObligation::Recommended,
                    message: "Phần đầu đang dùng đoạn văn rời; nên chuyển vào bảng 2 cột vô hình để căn lề thẳng hàng.".to_string(),
                    legal_basis: find_rule_legal_basis("ND30-PLIII-TBL"),
                });
            }

            for (rid, sev, msg) in &cand.typos_or_format_issues {
                has_needs_std = true;
                findings.push(HeaderFinding {
                    rule_id: rid.clone(),
                    severity: *sev,
                    obligation: RuleObligation::Mandatory,
                    message: msg.clone(),
                    legal_basis: find_rule_legal_basis(rid),
                });
            }

            if has_conflict {
                classification = HeaderClassification::DataConflict;
            } else if has_missing {
                classification = HeaderClassification::MissingComponents;
            } else if has_needs_std {
                classification = HeaderClassification::NeedsStandardization;
            } else {
                classification = HeaderClassification::Valid;
            }
        }
    }

    let can_create_new = candidates.is_empty();
    let can_standardize = !candidates.is_empty() && !requires_user_choice;

    Ok(HeaderInspectionResult {
        file_path: path.to_string_lossy().to_string(),
        file_name,
        classification,
        classification_label: classification_label_vi(classification).to_string(),
        section_count,
        has_header_footer,
        has_macros,
        candidates: candidates.into_iter().map(|c| c.summary).collect(),
        active_candidate_index,
        findings,
        applicable_rules,
        can_create_new,
        can_standardize,
        requires_user_choice,
    })
}

pub fn apply_header_builder_to_file(
    input_path: &Path,
    cfg: &HeaderBuilderConfig,
    mode: HeaderActionMode,
    explicit_profile: Option<&OrganizationProfile>,
) -> HeaderApplyResult {
    let timer = OpTimer::start("header_builder", "apply_header_builder_to_file", Some(input_path));
    let input_str = input_path.to_string_lossy().to_string();
    let org = resolve_org_profile(explicit_profile, cfg);

    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut temp_docx: Option<PathBuf> = None;
    let working_path = if ext == "doc" {
        let tmp = env::temp_dir().join(format!(
            "monword_hdr_work_{}_{}.docx",
            std::process::id(),
            chrono::Local::now().timestamp_millis()
        ));
        if let Err(e) = system_office::word_com_convert_doc_to_docx(input_path, &tmp) {
            let classified = classify_error_message(&e, AppErrorCode::ErrDocConvert);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return HeaderApplyResult {
                input_path: input_str,
                output_path: String::new(),
                initial_backup_path: None,
                success: false,
                classification_before: HeaderClassification::Uncertain,
                classification_after: HeaderClassification::Uncertain,
                logs: vec![],
                warnings: vec![],
                error: Some(msg),
                error_code: Some(code),
                inspection_after: None,
            };
        }
        temp_docx = Some(tmp.clone());
        tmp
    } else if ext == "docx" || ext == "docm" {
        input_path.to_path_buf()
    } else {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            format!("Không hỗ trợ định dạng .{} trong Header Builder.", ext),
        )
        .format_user_message();
        timer.finish_err("ERR_OPENXML_CORRUPT", &msg);
        return HeaderApplyResult {
            input_path: input_str,
            output_path: String::new(),
            initial_backup_path: None,
            success: false,
            classification_before: HeaderClassification::Uncertain,
            classification_after: HeaderClassification::Uncertain,
            logs: vec![],
            warnings: vec![],
            error: Some(msg),
            error_code: Some("ERR_OPENXML_CORRUPT".to_string()),
            inspection_after: None,
        };
    };

    let mut pkg = match OpenXmlPackage::open(&working_path) {
        Ok(p) => p,
        Err(e) => {
            if let Some(t) = temp_docx {
                let _ = fs::remove_file(t);
            }
            let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return HeaderApplyResult {
                input_path: input_str,
                output_path: String::new(),
                initial_backup_path: None,
                success: false,
                classification_before: HeaderClassification::Uncertain,
                classification_after: HeaderClassification::Uncertain,
                logs: vec![],
                warnings: vec![],
                error: Some(msg),
                error_code: Some(code),
                inspection_after: None,
            };
        }
    };

    let snapshot = match PreservationSnapshot::capture(&pkg) {
        Ok(s) => s,
        Err(e) => {
            if let Some(t) = temp_docx {
                let _ = fs::remove_file(t);
            }
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return HeaderApplyResult {
                input_path: input_str,
                output_path: String::new(),
                initial_backup_path: None,
                success: false,
                classification_before: HeaderClassification::Uncertain,
                classification_after: HeaderClassification::Uncertain,
                logs: vec![],
                warnings: vec![],
                error: Some(msg),
                error_code: Some(code),
                inspection_after: None,
            };
        }
    };

    let mut logs = Vec::new();
    let mut warnings = Vec::new();
    let mut initial_backup_path: Option<String> = None;

    let inspection_before = match inspect_package_header(input_path, &pkg, cfg, org.as_ref()) {
        Ok(ins) => ins,
        Err(e) => {
            if let Some(t) = temp_docx {
                let _ = fs::remove_file(t);
            }
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return HeaderApplyResult {
                input_path: input_str,
                output_path: String::new(),
                initial_backup_path: None,
                success: false,
                classification_before: HeaderClassification::Uncertain,
                classification_after: HeaderClassification::Uncertain,
                logs,
                warnings,
                error: Some(msg),
                error_code: Some(code),
                inspection_after: None,
            };
        }
    };

    let output_path = resolve_header_output_path(input_path, cfg.output_mode);
    let mut latest_backup_path: Option<PathBuf> = None;

    // Create versioned backup (plus immutable InitialBaseline) whenever overwriting OR requested
    if cfg.output_mode == OutputMode::OverwriteWithBackup || output_path == input_path {
        match ensure_initial_and_latest_backup(input_path) {
            Ok((init_bak, latest_bak)) => {
                initial_backup_path = Some(init_bak.to_string_lossy().to_string());
                latest_backup_path = Some(latest_bak);
            }
            Err(e) => {
                if let Some(t) = temp_docx {
                    let _ = fs::remove_file(t);
                }
                let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
                let code = classified.code.as_str().to_string();
                let msg = classified.format_user_message();
                timer.finish_err(&code, &msg);
                return HeaderApplyResult {
                    input_path: input_str,
                    output_path: String::new(),
                    initial_backup_path: None,
                    success: false,
                    classification_before: inspection_before.classification,
                    classification_after: inspection_before.classification,
                    logs,
                    warnings,
                    error: Some(msg),
                    error_code: Some(code),
                    inspection_after: Some(inspection_before),
                };
            }
        }
    }

    match apply_header_to_package(&mut pkg, cfg, mode, org.as_ref(), &mut logs, &mut warnings) {
        Ok(()) => {}
        Err(e) => {
            if let Some(t) = temp_docx {
                let _ = fs::remove_file(t);
            }
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return HeaderApplyResult {
                input_path: input_str,
                output_path: String::new(),
                initial_backup_path,
                success: false,
                classification_before: inspection_before.classification,
                classification_after: inspection_before.classification,
                logs,
                warnings,
                error: Some(msg),
                error_code: Some(code),
                inspection_after: Some(inspection_before),
            };
        }
    }

    if let Err(e) = safe_save::save_openxml_verified(
        &pkg,
        &output_path,
        Some(&snapshot),
        &PreservationPolicy::default(),
    ) {
        if let Some(t) = temp_docx {
            let _ = fs::remove_file(t);
        }
        let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
        let code = classified.code.as_str().to_string();
        let msg = classified.format_user_message();
        timer.finish_err(&code, &msg);
        return HeaderApplyResult {
            input_path: input_str,
            output_path: String::new(),
            initial_backup_path,
            success: false,
            classification_before: inspection_before.classification,
            classification_after: inspection_before.classification,
            logs,
            warnings,
            error: Some(msg),
            error_code: Some(code),
            inspection_after: None,
        };
    }

    if let Some(t) = temp_docx {
        let _ = fs::remove_file(t);
    }

    let mut verify_cfg = cfg.clone();
    if inspection_before.candidates.len() > 1 {
        verify_cfg.selected_candidate_index = inspection_before.active_candidate_index.or(Some(0));
    }
    verify_cfg.allow_override_existing_org = true;
    verify_cfg.allow_override_existing_number = true;
    verify_cfg.allow_override_existing_date = true;

    let disk_inspect_res = OpenXmlPackage::open(&output_path)
        .and_then(|disk_pkg| inspect_package_header(&output_path, &disk_pkg, &verify_cfg, org.as_ref()));

    let inspection_after = match disk_inspect_res {
        Ok(ins) => ins,
        Err(e) => {
            if let Some(ref bak) = latest_backup_path {
                if cfg.output_mode == OutputMode::OverwriteWithBackup && output_path == input_path {
                    let _ = backup_manager::restore_backup_checked(input_path, bak);
                }
            }
            let classified = classify_error_message(&e, AppErrorCode::ErrPostSaveVerify);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return HeaderApplyResult {
                input_path: input_str,
                output_path: output_path.to_string_lossy().to_string(),
                initial_backup_path,
                success: false,
                classification_before: inspection_before.classification,
                classification_after: inspection_before.classification,
                logs,
                warnings,
                error: Some(msg),
                error_code: Some(code),
                inspection_after: None,
            };
        }
    };

    let classification_after = inspection_after.classification;
    logs.push(format!("Đã lưu & hậu kiểm tệp kết quả tại: {}", output_path.display()));
    timer.finish_ok(&format!(
        "Chuẩn hóa phần đầu thành công ({:?} -> {:?})",
        inspection_before.classification, classification_after
    ));

    HeaderApplyResult {
        input_path: input_str,
        output_path: output_path.to_string_lossy().to_string(),
        initial_backup_path,
        success: true,
        classification_before: inspection_before.classification,
        classification_after,
        logs,
        warnings,
        error: None,
        error_code: None,
        inspection_after: Some(inspection_after),
    }
}

pub fn create_standalone_header_docx(
    dest_path: &Path,
    cfg: &HeaderBuilderConfig,
    explicit_profile: Option<&OrganizationProfile>,
) -> HeaderApplyResult {
    let timer = OpTimer::start("header_builder", "create_standalone_header_docx", Some(dest_path));
    let org = resolve_org_profile(explicit_profile, cfg);
    let mut pkg = build_empty_valid_docx_package();
    let mut logs = Vec::new();
    let mut warnings = Vec::new();

    if let Err(e) = apply_header_to_package(
        &mut pkg,
        cfg,
        HeaderActionMode::CreateNew,
        org.as_ref(),
        &mut logs,
        &mut warnings,
    ) {
        let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
        let code = classified.code.as_str().to_string();
        let msg = classified.format_user_message();
        timer.finish_err(&code, &msg);
        return HeaderApplyResult {
            input_path: String::new(),
            output_path: String::new(),
            initial_backup_path: None,
            success: false,
            classification_before: HeaderClassification::MissingComponents,
            classification_after: HeaderClassification::MissingComponents,
            logs,
            warnings,
            error: Some(msg),
            error_code: Some(code),
            inspection_after: None,
        };
    }

    if let Err(e) = safe_save::save_openxml_verified(
        &pkg,
        dest_path,
        None,
        &PreservationPolicy::default(),
    ) {
        let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
        let code = classified.code.as_str().to_string();
        let msg = classified.format_user_message();
        timer.finish_err(&code, &msg);
        return HeaderApplyResult {
            input_path: String::new(),
            output_path: String::new(),
            initial_backup_path: None,
            success: false,
            classification_before: HeaderClassification::MissingComponents,
            classification_after: HeaderClassification::MissingComponents,
            logs,
            warnings,
            error: Some(msg),
            error_code: Some(code),
            inspection_after: None,
        };
    }

    let mut verify_cfg = cfg.clone();
    verify_cfg.allow_override_existing_org = true;
    verify_cfg.allow_override_existing_number = true;
    verify_cfg.allow_override_existing_date = true;
    let disk_inspect_res = OpenXmlPackage::open(dest_path)
        .and_then(|disk_pkg| inspect_package_header(dest_path, &disk_pkg, &verify_cfg, org.as_ref()));

    let inspection_after = match disk_inspect_res {
        Ok(ins) => ins,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrPostSaveVerify);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return HeaderApplyResult {
                input_path: dest_path.to_string_lossy().to_string(),
                output_path: dest_path.to_string_lossy().to_string(),
                initial_backup_path: None,
                success: false,
                classification_before: HeaderClassification::MissingComponents,
                classification_after: HeaderClassification::MissingComponents,
                logs,
                warnings,
                error: Some(msg),
                error_code: Some(code),
                inspection_after: None,
            };
        }
    };

    let classification_after = inspection_after.classification;
    timer.finish_ok("Tạo tệp header độc lập thành công");

    HeaderApplyResult {
        input_path: dest_path.to_string_lossy().to_string(),
        output_path: dest_path.to_string_lossy().to_string(),
        initial_backup_path: None,
        success: true,
        classification_before: HeaderClassification::MissingComponents,
        classification_after,
        logs,
        warnings,
        error: None,
        error_code: None,
        inspection_after: Some(inspection_after),
    }
}

pub fn restore_initial_header_backup(original_path: &Path, backup_path: &Path) -> Result<String, String> {
    if backup_path.as_os_str().is_empty() {
        backup_manager::restore_initial_baseline(original_path)
    } else {
        backup_manager::restore_backup_checked(original_path, backup_path)
    }
}

fn ensure_initial_and_latest_backup(original_path: &Path) -> Result<(PathBuf, PathBuf), String> {
    let created = backup_manager::create_versioned_backup(original_path, "header_builder")?;
    let all = backup_manager::list_backups_for_file(original_path);
    let init_path = all
        .iter()
        .find(|r| r.kind == crate::models::BackupKind::InitialBaseline && r.integrity_ok)
        .map(|r| PathBuf::from(&r.backup_path))
        .unwrap_or_else(|| PathBuf::from(&created.backup_path));
    Ok((init_path, PathBuf::from(&created.backup_path)))
}

fn resolve_header_output_path(input_path: &Path, output_mode: OutputMode) -> PathBuf {
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("docx")
        .to_ascii_lowercase();
    let out_ext = if ext == "doc" { "docx" } else { ext.as_str() };
    match output_mode {
        OutputMode::NewCopy => {
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document");
            let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}_chuanhoa.{}", stem, out_ext))
        }
        OutputMode::OverwriteWithBackup => {
            if ext == "doc" {
                input_path.with_extension("docx")
            } else {
                input_path.to_path_buf()
            }
        }
    }
}

pub fn apply_header_to_package(
    pkg: &mut OpenXmlPackage,
    cfg: &HeaderBuilderConfig,
    mode: HeaderActionMode,
    org: Option<&OrganizationProfile>,
    logs: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let doc_xml = pkg
        .get_xml("word/document.xml")
        .ok_or_else(|| "Không tìm thấy word/document.xml.".to_string())?;
    let mut doc = XmlDocument::parse(&doc_xml)?;
    let styles_xml = pkg.get_xml("word/styles.xml");

    let printable_width = {
        let body = doc
            .root
            .child("w:body")
            .ok_or_else(|| "Tài liệu không có w:body.".to_string())?;
        read_first_section_printable_width(body)
    };

    let body = doc
        .root
        .child_mut("w:body")
        .ok_or_else(|| "Tài liệu không có w:body.".to_string())?;

    let candidates = detect_header_candidates(body, styles_xml.as_deref(), cfg.doc_type);

    let (replace_range, preserved_logos, preserved_sect_pr, existing_fields) = match mode {
        HeaderActionMode::CreateNew => {
            if !candidates.is_empty() {
                return Err(
                    "Văn bản đã có phần đầu (Quốc hiệu/Tiêu ngữ hoặc Cơ quan). Hệ thống từ chối chèn trùng lặp — hãy dùng \"Chuẩn hóa phần đầu hiện có\"."
                        .to_string(),
                );
            }
            (None, Vec::new(), None, DetectedHeaderFields::default())
        }
        HeaderActionMode::StandardizeExisting => {
            if candidates.is_empty() {
                return Err(
                    "Chưa phát hiện phần đầu hiện có trong tài liệu. Hãy chọn \"Tạo phần đầu mới\" nếu muốn chèn mới."
                        .to_string(),
                );
            }
            if candidates.len() > 1 && cfg.selected_candidate_index.is_none() {
                return Err(format!(
                    "Tìm thấy {} vùng có thể là phần đầu văn bản. Vui lòng chọn đúng vùng mục tiêu trước khi chuẩn hóa.",
                    candidates.len()
                ));
            }
            let idx = cfg.selected_candidate_index.unwrap_or(0).min(candidates.len() - 1);
            let cand = &candidates[idx];
            if cand.summary.layout_kind == HeaderLayoutKind::CustomTemplateWithLogo {
                return Err(
                    "Phần đầu thuộc mẫu bảng/banner nhiều cột tùy biến; hệ thống giữ nguyên để tránh làm hỏng thiết kế."
                        .to_string(),
                );
            }
            (
                Some((cand.summary.body_start_index, cand.summary.body_end_index)),
                cand.preserved_logo_paras.clone(),
                cand.preserved_sect_pr.clone(),
                cand.summary.fields.clone(),
            )
        }
    };

    let resolved = resolve_header_render_values(cfg, org, &existing_fields, warnings)?;

    let mut nodes = build_header_ooxml_nodes(
        &resolved,
        cfg,
        printable_width,
        &preserved_logos,
        preserved_sect_pr.as_ref(),
    );

    if let Some((start_idx, end_idx)) = replace_range {
        if end_idx >= start_idx && end_idx < body.children.len() {
            body.children.splice(start_idx..=end_idx, nodes);
            logs.push(format!(
                "Đã chuẩn hóa phần đầu tại vị trí body[{}..={}] theo quy chuẩn {} (bảo toàn {} ảnh/logo & {} section gốc).",
                start_idx,
                end_idx,
                cfg.doc_type.default_label(),
                preserved_logos.len(),
                count_sections(body)
            ));
        }
    } else {
        // Insert before existing body content (ensuring word/document.xml sectPr stays at end)
        let insert_pos = body
            .children
            .iter()
            .position(|n| matches!(n, XmlNode::Element(e) if e.name != "w:sectPr"))
            .unwrap_or(0);
        for (offset, n) in nodes.drain(..).enumerate() {
            body.children.insert(insert_pos + offset, n);
        }
        logs.push(format!(
            "Đã tạo mới phần đầu văn bản ({}) và giữ nguyên toàn bộ nội dung gốc bên dưới.",
            cfg.doc_type.default_label()
        ));
    }

    pkg.set_xml("word/document.xml", doc.to_xml_string()?);
    Ok(())
}

#[derive(Debug, Clone)]
struct ResolvedHeaderValues {
    sector: OrgSector,
    governing_org: String,
    issuing_org: String,
    affiliated_unit: String,
    department: String,
    show_org_underline: bool,
    show_national_header: bool,
    national_header: String,
    motto: String,
    show_motto_underline: bool,
    show_number_symbol: bool,
    number_str: String,
    symbol_str: String,
    classification_mark: String,
    show_location_date: bool,
    location_date_line: String,
    show_doc_type_and_subject: bool,
    doc_type_label: String,
    subject_text: String,
    summary_line: String,
}

fn resolve_header_render_values(
    cfg: &HeaderBuilderConfig,
    org: Option<&OrganizationProfile>,
    existing: &DetectedHeaderFields,
    warnings: &mut Vec<String>,
) -> Result<ResolvedHeaderValues, String> {
    let sector = org.map(|o| o.sector).unwrap_or(OrgSector::StateAgency);

    // 1. Organization block: never invent names!
    let (governing_org, issuing_org, affiliated_unit, department) = if let Some(o) = org {
        let existing_iss = existing.issuing_org.trim();
        let prof_iss = o.issuing_org.trim();
        if !existing_iss.is_empty()
            && !prof_iss.is_empty()
            && collapse_spaces_upper(existing_iss) != collapse_spaces_upper(prof_iss)
            && !cfg.allow_override_existing_org
        {
            return Err(format!(
                "Phát hiện xung đột tên cơ quan: văn bản ghi \"{}\" khác với hồ sơ \"{}\". Hãy tích chọn xác nhận thay thế tên cơ quan hoặc giữ tên trong văn bản.",
                existing_iss, prof_iss
            ));
        }

        let gov = if cfg.show_governing_org {
            if !o.governing_org.trim().is_empty() {
                o.governing_org.trim().to_uppercase()
            } else {
                existing.governing_org.trim().to_uppercase()
            }
        } else {
            String::new()
        };
        let iss = if cfg.show_issuing_org {
            if !prof_iss.is_empty() {
                prof_iss.to_uppercase()
            } else {
                existing_iss.to_uppercase()
            }
        } else {
            String::new()
        };
        let aff = if cfg.show_affiliated_unit {
            if !o.affiliated_unit.trim().is_empty() {
                o.affiliated_unit.trim().to_uppercase()
            } else {
                existing.affiliated_unit.trim().to_uppercase()
            }
        } else {
            String::new()
        };
        let dept = if cfg.show_department {
            if !o.department_or_branch.trim().is_empty() {
                o.department_or_branch.trim().to_uppercase()
            } else {
                existing.department.trim().to_uppercase()
            }
        } else {
            String::new()
        };
        (gov, iss, aff, dept)
    } else {
        (
            if cfg.show_governing_org {
                existing.governing_org.trim().to_uppercase()
            } else {
                String::new()
            },
            if cfg.show_issuing_org {
                existing.issuing_org.trim().to_uppercase()
            } else {
                String::new()
            },
            if cfg.show_affiliated_unit {
                existing.affiliated_unit.trim().to_uppercase()
            } else {
                String::new()
            },
            if cfg.show_department {
                existing.department.trim().to_uppercase()
            } else {
                String::new()
            },
        )
    };

    if cfg.show_issuing_org && issuing_org.is_empty() {
        return Err(
            "Thiếu Tên cơ quan, tổ chức ban hành: vui lòng chọn Hồ sơ Tổ chức hoặc kiểm tra văn bản gốc (hệ thống không tự suy đoán tên cơ quan)."
                .to_string(),
        );
    }

    // 2. National Header & Motto
    let national_header = if !cfg.custom_national_header.trim().is_empty() {
        cfg.custom_national_header.trim().to_string()
    } else if sector == OrgSector::PartyOrg {
        STD_PARTY_HEADER.to_string()
    } else {
        STD_NATIONAL_HEADER.to_string()
    };

    let motto = if !cfg.custom_motto.trim().is_empty() {
        cfg.custom_motto.trim().to_string()
    } else if sector == OrgSector::PartyOrg {
        String::new()
    } else {
        STD_MOTTO.to_string()
    };

    // 3. Number & Symbol (strictly no auto-generation of fake numbers!)
    let (number_str, symbol_str) = match cfg.number_mode {
        HeaderNumberMode::KeepExisting => {
            let num = format_nd30_doc_number(&existing.number_value);
            let sym = if !existing.symbol_value.trim().is_empty() {
                existing.symbol_value.trim().replace(' ', "")
            } else {
                cfg.doc_symbol.trim().to_string()
            };
            (num, sym)
        }
        HeaderNumberMode::ManualInput => {
            let has_existing_num = !existing.number_value.trim().is_empty();
            if has_existing_num
                && !cfg.doc_number.trim().is_empty()
                && existing.number_value.trim() != cfg.doc_number.trim()
                && !cfg.allow_override_existing_number
            {
                return Err(format!(
                    "Văn bản gốc đang có Số \"{}\". Hệ thống không tự ý thay số văn bản khi chưa được xác nhận.",
                    existing.number_raw
                ));
            }
            let num = if !cfg.doc_number.trim().is_empty() {
                format_nd30_doc_number(cfg.doc_number.trim())
            } else if has_existing_num && !cfg.allow_override_existing_number {
                format_nd30_doc_number(&existing.number_value)
            } else {
                String::new()
            };
            let sym = if !cfg.doc_symbol.trim().is_empty() {
                cfg.doc_symbol.trim().to_uppercase()
            } else {
                existing.symbol_value.trim().to_uppercase()
            };
            (num, sym)
        }
    };

    // 4. Location & Date
    let location = if !cfg.location_override.trim().is_empty() {
        cfg.location_override.trim().to_string()
    } else if !existing.location.trim().is_empty() {
        existing.location.trim().to_string()
    } else if let Some(o) = org {
        o.default_location.trim().to_string()
    } else {
        String::new()
    };

    if cfg.show_location_date && location.is_empty() {
        warnings.push("Chưa có Địa danh ban hành (có thể nhập hoặc cấu hình trong Hồ sơ Tổ chức).".to_string());
    }

    let has_concrete_existing_date = !existing.day.trim().is_empty() || !existing.month.trim().is_empty();
    let date_part = match cfg.date_mode {
        HeaderDateMode::KeepExisting => {
            if !existing.date_raw.trim().is_empty() {
                normalize_nd30_date_fragment(&existing.date_raw)
            } else {
                format_blank_date(existing.year.trim())
            }
        }
        HeaderDateMode::BlankDraft => {
            if has_concrete_existing_date && !cfg.allow_override_existing_date {
                return Err(format!(
                    "Văn bản gốc đã có ngày ban hành \"{}\". Không tự ý xóa ngày gốc khi chưa xác nhận.",
                    existing.date_raw
                ));
            }
            format_blank_date(cfg.manual_year.trim())
        }
        HeaderDateMode::UseToday => {
            if has_concrete_existing_date && !cfg.allow_override_existing_date {
                return Err(format!(
                    "Văn bản gốc đã có ngày ban hành \"{}\". Tích chọn xác nhận nếu muốn thay bằng ngày hôm nay.",
                    existing.date_raw
                ));
            }
            let now = Local::now();
            format_concrete_nd30_date(now.day(), now.month(), now.year())
        }
        HeaderDateMode::ManualDate => {
            if has_concrete_existing_date && !cfg.allow_override_existing_date {
                return Err(format!(
                    "Văn bản gốc đã có ngày ban hành \"{}\". Tích chọn xác nhận nếu muốn thay bằng ngày nhập thủ công.",
                    existing.date_raw
                ));
            }
            let d = cfg.manual_day.trim().parse::<u32>().ok();
            let m = cfg.manual_month.trim().parse::<u32>().ok();
            let y = cfg.manual_year.trim();
            match (d, m, y.is_empty()) {
                (Some(dd), Some(mm), false) => format_concrete_nd30_date(dd, mm, y.parse::<i32>().unwrap_or_else(|_| Local::now().year())),
                _ => format_manual_partial_date(cfg.manual_day.trim(), cfg.manual_month.trim(), y),
            }
        }
    };

    let location_date_line = if location.is_empty() {
        date_part
    } else {
        format!("{}, {}", location.trim_end_matches(','), date_part)
    };

    // 5. Doc Type & Subject
    let doc_type_label = if !cfg.custom_doc_type_label.trim().is_empty() {
        cfg.custom_doc_type_label.trim().to_uppercase()
    } else if cfg.doc_type == AdminDocType::VanBanNoiBo && !existing.doc_type_label.trim().is_empty() {
        existing.doc_type_label.trim().to_uppercase()
    } else {
        cfg.doc_type.default_label().to_string()
    };

    let subject_text = if !cfg.subject_text.trim().is_empty() {
        cfg.subject_text.trim().to_string()
    } else {
        existing.subject.trim().to_string()
    };

    let summary_line = if !cfg.summary_line.trim().is_empty() {
        cfg.summary_line.trim().to_string()
    } else {
        existing.summary_line.trim().to_string()
    };

    let classification_mark = if !cfg.classification_mark.trim().is_empty() {
        cfg.classification_mark.trim().to_uppercase()
    } else {
        existing.classification_mark.trim().to_uppercase()
    };

    Ok(ResolvedHeaderValues {
        sector,
        governing_org,
        issuing_org,
        affiliated_unit,
        department,
        show_org_underline: cfg.show_org_underline,
        show_national_header: cfg.show_national_header,
        national_header,
        motto,
        show_motto_underline: cfg.show_motto_underline && sector != OrgSector::PartyOrg,
        show_number_symbol: cfg.show_number_symbol,
        number_str,
        symbol_str,
        classification_mark,
        show_location_date: cfg.show_location_date,
        location_date_line,
        show_doc_type_and_subject: cfg.show_doc_type_and_subject,
        doc_type_label,
        subject_text,
        summary_line,
    })
}

fn format_nd30_doc_number(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    if let Ok(n) = t.parse::<u32>() {
        if (1..=9).contains(&n) {
            return format!("0{}", n);
        }
    }
    t.to_string()
}

fn format_concrete_nd30_date(day: u32, month: u32, year: i32) -> String {
    let d_str = if (1..=9).contains(&day) {
        format!("0{}", day)
    } else {
        day.to_string()
    };
    let m_str = if month == 1 || month == 2 {
        format!("0{}", month)
    } else {
        month.to_string()
    };
    format!("ngày {} tháng {} năm {}", d_str, m_str, year)
}

fn format_blank_date(year_hint: &str) -> String {
    let y = if year_hint.is_empty() {
        Local::now().year().to_string()
    } else {
        year_hint.to_string()
    };
    format!("ngày     tháng     năm {}", y)
}

fn format_manual_partial_date(d: &str, m: &str, y: &str) -> String {
    let d_out = if d.is_empty() {
        "    ".to_string()
    } else {
        format_nd30_doc_number(d)
    };
    let m_out = if m.is_empty() {
        "    ".to_string()
    } else if let Ok(mn) = m.parse::<u32>() {
        if mn == 1 || mn == 2 {
            format!("0{}", mn)
        } else {
            mn.to_string()
        }
    } else {
        m.to_string()
    };
    let y_out = if y.is_empty() {
        Local::now().year().to_string()
    } else {
        y.to_string()
    };
    format!("ngày {} tháng {} năm {}", d_out, m_out, y_out)
}

fn normalize_nd30_date_fragment(raw: &str) -> String {
    let t = raw.trim();
    if let Some(caps) = re_date_parts().captures(t) {
        let day_s = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        let mon_s = caps.get(2).map(|m| m.as_str()).unwrap_or("").trim();
        let yr_s = caps.get(3).map(|m| m.as_str()).unwrap_or("").trim();
        let d_out = if day_s.is_empty() {
            "     ".to_string()
        } else if let Ok(d) = day_s.parse::<u32>() {
            if (1..=9).contains(&d) {
                format!("0{}", d)
            } else {
                d.to_string()
            }
        } else {
            day_s.to_string()
        };
        let m_out = if mon_s.is_empty() {
            "     ".to_string()
        } else if let Ok(m) = mon_s.parse::<u32>() {
            if m == 1 || m == 2 {
                format!("0{}", m)
            } else {
                m.to_string()
            }
        } else {
            mon_s.to_string()
        };
        let y_out = if yr_s.is_empty() {
            Local::now().year().to_string()
        } else {
            yr_s.to_string()
        };
        return format!("ngày {} tháng {} năm {}", d_out, m_out, y_out);
    }
    t.to_string()
}

fn build_header_ooxml_nodes(
    val: &ResolvedHeaderValues,
    cfg: &HeaderBuilderConfig,
    printable_width_twips: i32,
    preserved_logos: &[XmlElement],
    preserved_sect_pr: Option<&XmlElement>,
) -> Vec<XmlNode> {
    let mut left_lines: Vec<(&str, f64, bool, bool)> = Vec::new();
    if !val.governing_org.is_empty() {
        left_lines.push((&val.governing_org, 12.5, false, true));
    }
    if !val.issuing_org.is_empty() {
        left_lines.push((&val.issuing_org, 12.5, true, true));
    }
    if !val.affiliated_unit.is_empty() {
        left_lines.push((&val.affiliated_unit, 12.0, false, true));
    }
    if !val.department.is_empty() {
        left_lines.push((&val.department, 12.0, true, true));
    }

    let number_text = if val.show_number_symbol {
        match (val.number_str.is_empty(), val.symbol_str.is_empty()) {
            (false, false) => format!("Số: {}/{}", val.number_str, val.symbol_str),
            (true, false) => format!("Số:        /{}", val.symbol_str),
            (false, true) => format!("Số: {}", val.number_str),
            (true, true) => "Số:        /".to_string(),
        }
    } else {
        String::new()
    };

    let subj_left_text = if cfg.doc_type.uses_left_cell_subject()
        && val.show_doc_type_and_subject
        && !val.subject_text.is_empty()
    {
        if val.subject_text.to_lowercase().starts_with("v/v") {
            val.subject_text.clone()
        } else {
            format!("V/v {}", val.subject_text)
        }
    } else {
        String::new()
    };

    // Measure text widths in twips to size the 2 columns accurately
    let mut max_left_text_w = 1800i32;
    for (txt, pt, bold, caps) in &left_lines {
        max_left_text_w = max_left_text_w.max(estimate_text_line_twips(txt, *pt, *bold, *caps));
    }
    if !number_text.is_empty() {
        max_left_text_w = max_left_text_w.max(estimate_text_line_twips(&number_text, 13.0, false, false));
    }
    if !subj_left_text.is_empty() {
        max_left_text_w = max_left_text_w.max(estimate_text_line_twips(&subj_left_text, 12.5, false, false).min(3800));
    }

    let mut max_right_text_w = 2400i32;
    let mut qh_pt = 12.5f64;
    let mut motto_pt = 13.5f64;
    if val.show_national_header {
        let qh_w = estimate_text_line_twips(&val.national_header, qh_pt, true, true);
        let max_right_budget = (printable_width_twips as f64 * 0.60).round() as i32;
        if qh_w > max_right_budget {
            qh_pt = 12.0;
            motto_pt = 13.0;
        }
        max_right_text_w = max_right_text_w.max(estimate_text_line_twips(&val.national_header, qh_pt, true, true));
        if !val.motto.is_empty() {
            max_right_text_w = max_right_text_w.max(estimate_text_line_twips(&val.motto, motto_pt, true, false));
        }
    }
    if val.show_location_date && !val.location_date_line.is_empty() {
        max_right_text_w = max_right_text_w.max(estimate_text_line_twips(&val.location_date_line, 13.0, false, false));
    }

    let req_left = max_left_text_w + 80;
    let req_right = max_right_text_w + 80;

    // Fit left/right columns dynamically within printable_width_twips
    let max_right_allowed = (printable_width_twips as f64 * 0.62).round() as i32;
    let min_left_allowed = (printable_width_twips as f64 * 0.36).round() as i32;
    let ideal_right = (printable_width_twips as f64 * 0.56).round() as i32;
    let col_right = if req_left + req_right <= printable_width_twips {
        ideal_right.clamp(req_right, printable_width_twips - req_left)
    } else {
        req_right.clamp((printable_width_twips - min_left_allowed).min(max_right_allowed) / 2, max_right_allowed)
            .min(printable_width_twips - min_left_allowed)
    };
    let col_left = (printable_width_twips - col_right).max(1600);

    // If single agency line still exceeds col_left, nudge agency pt down safely from 12.5 to 11.5
    let agency_pt = if max_left_text_w + 60 > col_left { 11.5 } else { 12.5 };

    // Build borderless 2-col table
    let mut tbl = XmlElement::new("w:tbl");

    let mut tbl_pr = XmlElement::new("w:tblPr");
    let mut tbl_w = XmlElement::new("w:tblW");
    tbl_w.set_attr("w:w", &printable_width_twips.to_string());
    tbl_w.set_attr("w:type", "dxa");
    tbl_pr.insert_child_ordered(tbl_w, TBL_PR_ORDER);

    let mut tbl_ind = XmlElement::new("w:tblInd");
    tbl_ind.set_attr("w:w", "0");
    tbl_ind.set_attr("w:type", "dxa");
    tbl_pr.insert_child_ordered(tbl_ind, TBL_PR_ORDER);

    tbl_pr.insert_child_ordered(build_no_borders_el("w:tblBorders"), TBL_PR_ORDER);

    let mut tbl_layout = XmlElement::new("w:tblLayout");
    tbl_layout.set_attr("w:type", "fixed");
    tbl_pr.insert_child_ordered(tbl_layout, TBL_PR_ORDER);

    let mut cell_mar = XmlElement::new("w:tblCellMar");
    for edge in ["w:top", "w:left", "w:bottom", "w:right"] {
        let mut e = XmlElement::new(edge);
        e.set_attr("w:w", "0");
        e.set_attr("w:type", "dxa");
        cell_mar.children.push(XmlNode::Element(e));
    }
    tbl_pr.insert_child_ordered(cell_mar, TBL_PR_ORDER);
    tbl.children.push(XmlNode::Element(tbl_pr));

    let mut tbl_grid = XmlElement::new("w:tblGrid");
    for w in [col_left, col_right] {
        let mut gc = XmlElement::new("w:gridCol");
        gc.set_attr("w:w", &w.to_string());
        tbl_grid.children.push(XmlNode::Element(gc));
    }
    tbl.children.push(XmlNode::Element(tbl_grid));

    // ROW 1: Left Org | Right National Header + Motto
    let mut tr1 = XmlElement::new("w:tr");
    let mut tr1_pr = XmlElement::new("w:trPr");
    tr1_pr.children.push(XmlNode::Element(XmlElement::new("w:cantSplit")));
    tr1.children.push(XmlNode::Element(tr1_pr));

    let mut tc1_left = build_cell(col_left);
    for logo_p in preserved_logos {
        tc1_left.children.push(XmlNode::Element(logo_p.clone()));
    }
    let mut bottom_org_text_w = 0i32;
    for (idx, (txt, base_pt, bold, caps)) in left_lines.iter().enumerate() {
        let pt = if *base_pt > agency_pt { agency_pt } else { *base_pt };
        let w = estimate_text_line_twips(txt, pt, *bold, *caps);
        if idx + 1 == left_lines.len() {
            bottom_org_text_w = w;
        } else if bottom_org_text_w == 0 {
            bottom_org_text_w = w;
        }
        tc1_left.children.push(XmlNode::Element(build_run_para(
            txt,
            "center",
            pt,
            *bold,
            false,
            *caps,
            0,
            20,
        )));
    }
    if val.show_org_underline && !left_lines.is_empty() {
        if val.sector == OrgSector::PartyOrg {
            tc1_left.children.push(XmlNode::Element(build_run_para(
                "*",
                "center",
                12.0,
                false,
                false,
                false,
                0,
                40,
            )));
        } else {
            // Solid line 1/3..1/2 (42%) of issuing org text width
            let line_w = ((bottom_org_text_w.max(1800) as f64) * 0.42)
                .round()
                .clamp(900.0, (col_left - 400).max(900) as f64) as i32;
            let side_ind = ((col_left - line_w) / 2).max(200);
            tc1_left.children.push(XmlNode::Element(build_bottom_rule_para(side_ind, 40)));
        }
    }
    if tc1_left.children_elements().filter(|e| e.name == "w:p").count() == 0 {
        tc1_left.children.push(XmlNode::Element(XmlElement::new("w:p")));
    }
    tr1.children.push(XmlNode::Element(tc1_left));

    let mut tc1_right = build_cell(col_right);
    if val.show_national_header {
        tc1_right.children.push(XmlNode::Element(build_run_para(
            &val.national_header,
            "center",
            qh_pt,
            true,
            false,
            true,
            0,
            20,
        )));
        if !val.motto.is_empty() {
            let motto_w = estimate_text_line_twips(&val.motto, motto_pt, true, false);
            tc1_right.children.push(XmlNode::Element(build_run_para(
                &val.motto,
                "center",
                motto_pt,
                true,
                false,
                false,
                0,
                20,
            )));
            if val.show_motto_underline {
                let line_w = motto_w.min(col_right - 120).max(1200);
                let side_ind = ((col_right - line_w) / 2).max(60);
                tc1_right.children.push(XmlNode::Element(build_bottom_rule_para(side_ind, 40)));
            }
        }
    } else {
        tc1_right.children.push(XmlNode::Element(XmlElement::new("w:p")));
    }
    tr1.children.push(XmlNode::Element(tc1_right));
    tbl.children.push(XmlNode::Element(tr1));

    // ROW 2: Left Số/Ký hiệu (+ V/v for Công văn) | Right Địa danh, ngày tháng
    let mut tr2 = XmlElement::new("w:tr");
    let mut tr2_pr = XmlElement::new("w:trPr");
    tr2_pr.children.push(XmlNode::Element(XmlElement::new("w:cantSplit")));
    tr2.children.push(XmlNode::Element(tr2_pr));

    let mut tc2_left = build_cell(col_left);
    if !val.classification_mark.is_empty() {
        tc2_left.children.push(XmlNode::Element(build_run_para(
            &val.classification_mark,
            "center",
            13.0,
            true,
            false,
            true,
            20,
            20,
        )));
    }
    if !number_text.is_empty() {
        tc2_left.children.push(XmlNode::Element(build_run_para(
            &number_text,
            "center",
            13.0,
            false,
            false,
            false,
            40,
            20,
        )));
    }
    if !subj_left_text.is_empty() {
        tc2_left.children.push(XmlNode::Element(build_run_para(
            &subj_left_text,
            "center",
            12.5,
            false,
            false,
            false,
            60,
            40,
        )));
    }
    if tc2_left.children_elements().filter(|e| e.name == "w:p").count() == 0 {
        tc2_left.children.push(XmlNode::Element(XmlElement::new("w:p")));
    }
    tr2.children.push(XmlNode::Element(tc2_left));

    let mut tc2_right = build_cell(col_right);
    if val.show_location_date && !val.location_date_line.is_empty() {
        tc2_right.children.push(XmlNode::Element(build_run_para(
            &val.location_date_line,
            "center",
            13.0,
            false,
            true,
            false,
            40,
            40,
        )));
    } else {
        tc2_right.children.push(XmlNode::Element(XmlElement::new("w:p")));
    }
    tr2.children.push(XmlNode::Element(tc2_right));
    tbl.children.push(XmlNode::Element(tr2));

    let mut out_nodes: Vec<XmlNode> = vec![XmlNode::Element(tbl)];

    // Below table: Centered Tên loại + Trích yếu + Đường kẻ (when doc_type is NOT Công văn)
    if val.show_doc_type_and_subject && !cfg.doc_type.uses_left_cell_subject() {
        out_nodes.push(XmlNode::Element(build_run_para(
            &val.doc_type_label,
            "center",
            14.0,
            true,
            false,
            true,
            160,
            40,
        )));
        if !val.subject_text.is_empty() {
            let subj_w = estimate_text_line_twips(&val.subject_text, 14.0, true, false);
            out_nodes.push(XmlNode::Element(build_run_para(
                &val.subject_text,
                "center",
                14.0,
                true,
                false,
                false,
                0,
                40,
            )));
            if !val.summary_line.is_empty() {
                out_nodes.push(XmlNode::Element(build_run_para(
                    &val.summary_line,
                    "center",
                    13.0,
                    false,
                    true,
                    false,
                    0,
                    40,
                )));
            }
            let rule_w = ((subj_w.max(2400).min(printable_width_twips) as f64) * 0.42)
                .round()
                .clamp(1200.0, 3200.0) as i32;
            let side_ind = ((printable_width_twips - rule_w) / 2).max(600);
            out_nodes.push(XmlNode::Element(build_bottom_rule_para(side_ind, 140)));
        } else if !val.summary_line.is_empty() {
            out_nodes.push(XmlNode::Element(build_run_para(
                &val.summary_line,
                "center",
                13.0,
                false,
                true,
                false,
                0,
                100,
            )));
        }
    } else if cfg.doc_type.uses_left_cell_subject() && !val.summary_line.is_empty() {
        out_nodes.push(XmlNode::Element(build_run_para(
            &val.summary_line,
            "center",
            13.0,
            false,
            true,
            false,
            80,
            80,
        )));
    }

    // Re-attach preserved section break (<w:sectPr>) onto an anchor paragraph if replaced header had one
    if let Some(sect_pr) = preserved_sect_pr {
        let need_new_anchor = !matches!(out_nodes.last(), Some(XmlNode::Element(e)) if e.name == "w:p");
        if need_new_anchor {
            out_nodes.push(XmlNode::Element(XmlElement::new("w:p")));
        }
        if let Some(XmlNode::Element(last_p)) = out_nodes.last_mut() {
            let p_pr = last_p.ensure_child_first("w:pPr");
            p_pr.insert_child_ordered(sect_pr.clone(), P_PR_ORDER);
        }
    }

    out_nodes
}

fn build_cell(width_twips: i32) -> XmlElement {
    let mut tc = XmlElement::new("w:tc");
    let mut tc_pr = XmlElement::new("w:tcPr");
    let mut tc_w = XmlElement::new("w:tcW");
    tc_w.set_attr("w:w", &width_twips.to_string());
    tc_w.set_attr("w:type", "dxa");
    tc_pr.children.push(XmlNode::Element(tc_w));
    tc_pr.children.push(XmlNode::Element(build_no_borders_el("w:tcBorders")));
    tc.children.push(XmlNode::Element(tc_pr));
    tc
}

fn build_no_borders_el(tag: &str) -> XmlElement {
    let mut b = XmlElement::new(tag);
    for side in ["w:top", "w:left", "w:bottom", "w:right", "w:insideH", "w:insideV"] {
        if tag == "w:tcBorders" && (side == "w:insideH" || side == "w:insideV") {
            continue;
        }
        let mut s = XmlElement::new(side);
        s.set_attr("w:val", "none");
        s.set_attr("w:sz", "0");
        s.set_attr("w:space", "0");
        s.set_attr("w:color", "auto");
        b.children.push(XmlNode::Element(s));
    }
    b
}

fn build_run_para(
    text: &str,
    jc_val: &str,
    pt: f64,
    bold: bool,
    italic: bool,
    all_caps: bool,
    before_twips: u32,
    after_twips: u32,
) -> XmlElement {
    let half = (pt * 2.0).round() as u32;
    let mut p = XmlElement::new("w:p");
    let mut p_pr = XmlElement::new("w:pPr");

    let mut sp = XmlElement::new("w:spacing");
    sp.set_attr("w:before", &before_twips.to_string());
    sp.set_attr("w:after", &after_twips.to_string());
    sp.set_attr("w:line", "240");
    sp.set_attr("w:lineRule", "auto");
    p_pr.insert_child_ordered(sp, P_PR_ORDER);

    let mut ind = XmlElement::new("w:ind");
    ind.set_attr("w:left", "0");
    ind.set_attr("w:right", "0");
    ind.set_attr("w:firstLine", "0");
    p_pr.insert_child_ordered(ind, P_PR_ORDER);

    let mut jc = XmlElement::new("w:jc");
    jc.set_attr("w:val", jc_val);
    p_pr.insert_child_ordered(jc, P_PR_ORDER);

    p.children.push(XmlNode::Element(p_pr));

    let mut r = XmlElement::new("w:r");
    let mut r_pr = XmlElement::new("w:rPr");
    let mut r_fonts = XmlElement::new("w:rFonts");
    r_fonts.set_attr("w:ascii", "Times New Roman");
    r_fonts.set_attr("w:hAnsi", "Times New Roman");
    r_fonts.set_attr("w:cs", "Times New Roman");
    r_fonts.set_attr("w:eastAsia", "Times New Roman");
    r_pr.children.push(XmlNode::Element(r_fonts));
    if bold {
        r_pr.children.push(XmlNode::Element(XmlElement::new("w:b")));
        r_pr.children.push(XmlNode::Element(XmlElement::new("w:bCs")));
    }
    if italic {
        r_pr.children.push(XmlNode::Element(XmlElement::new("w:i")));
        r_pr.children.push(XmlNode::Element(XmlElement::new("w:iCs")));
    }
    if all_caps {
        r_pr.children.push(XmlNode::Element(XmlElement::new("w:caps")));
    }
    let mut color = XmlElement::new("w:color");
    color.set_attr("w:val", "000000");
    r_pr.children.push(XmlNode::Element(color));
    let mut sz = XmlElement::new("w:sz");
    sz.set_attr("w:val", &half.to_string());
    r_pr.children.push(XmlNode::Element(sz));
    let mut sz_cs = XmlElement::new("w:szCs");
    sz_cs.set_attr("w:val", &half.to_string());
    r_pr.children.push(XmlNode::Element(sz_cs));
    r.children.push(XmlNode::Element(r_pr));

    let mut t = XmlElement::new("w:t");
    if text.starts_with(' ') || text.ends_with(' ') || text.contains("  ") {
        t.set_attr("xml:space", "preserve");
    }
    t.set_text(text);
    r.children.push(XmlNode::Element(t));
    p.children.push(XmlNode::Element(r));
    p
}

fn build_bottom_rule_para(side_ind_twips: i32, after_twips: u32) -> XmlElement {
    let mut p = XmlElement::new("w:p");
    let mut p_pr = XmlElement::new("w:pPr");

    let mut p_bdr = XmlElement::new("w:pBdr");
    let mut bottom = XmlElement::new("w:bottom");
    bottom.set_attr("w:val", "single");
    bottom.set_attr("w:sz", "6");
    bottom.set_attr("w:space", "1");
    bottom.set_attr("w:color", "000000");
    p_bdr.children.push(XmlNode::Element(bottom));
    p_pr.insert_child_ordered(p_bdr, P_PR_ORDER);

    let mut sp = XmlElement::new("w:spacing");
    sp.set_attr("w:before", "0");
    sp.set_attr("w:after", &after_twips.to_string());
    sp.set_attr("w:line", "80");
    sp.set_attr("w:lineRule", "exact");
    p_pr.insert_child_ordered(sp, P_PR_ORDER);

    let mut ind = XmlElement::new("w:ind");
    ind.set_attr("w:left", &side_ind_twips.to_string());
    ind.set_attr("w:right", &side_ind_twips.to_string());
    p_pr.insert_child_ordered(ind, P_PR_ORDER);

    p.children.push(XmlNode::Element(p_pr));
    p
}

fn read_first_section_printable_width(body: &XmlElement) -> i32 {
    for node in &body.children {
        if let XmlNode::Element(el) = node {
            let sect_opt = if el.name == "w:sectPr" {
                Some(el)
            } else if el.name == "w:p" {
                el.child("w:pPr").and_then(|pp| pp.child("w:sectPr"))
            } else {
                None
            };
            if let Some(sect) = sect_opt {
                let w = sect
                    .child("w:pgSz")
                    .and_then(|s| s.get_attr("w:w"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(11906);
                let l = sect
                    .child("w:pgMar")
                    .and_then(|m| m.get_attr("w:left"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(1701);
                let r = sect
                    .child("w:pgMar")
                    .and_then(|m| m.get_attr("w:right"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(1134);
                return (w - l - r).max(5000);
            }
        }
    }
    9071
}

fn count_sections(body: &XmlElement) -> usize {
    let mut cnt = 0usize;
    for node in &body.children {
        if let XmlNode::Element(el) = node {
            if el.name == "w:sectPr" {
                cnt += 1;
            } else if el.name == "w:p"
                && el.child("w:pPr").and_then(|pp| pp.child("w:sectPr")).is_some()
            {
                cnt += 1;
            }
        }
    }
    cnt.max(1)
}

fn detect_header_candidates(
    body: &XmlElement,
    styles_xml: Option<&str>,
    default_doc_type: AdminDocType,
) -> Vec<CandidateInternal> {
    let _ = styles_xml;
    let mut candidates = Vec::new();
    let children = &body.children;
    let mut i = 0usize;

    while i < children.len() {
        let XmlNode::Element(el) = &children[i] else {
            i += 1;
            continue;
        };

        if el.name == "w:tbl" {
            if let Some(cand) = try_parse_table_header_candidate(
                children,
                i,
                candidates.len(),
                default_doc_type,
            ) {
                let next_i = cand.summary.body_end_index + 1;
                candidates.push(cand);
                i = next_i;
                continue;
            }
        } else if el.name == "w:p" {
            // Only start a paragraph header candidate near section start (top of body or just after break)
            let is_top_or_after_break = i <= 4
                || children[i.saturating_sub(2)..i].iter().any(|n| match n {
                    XmlNode::Element(prev) => has_page_or_section_break(prev),
                    _ => false,
                });
            if is_top_or_after_break {
                if let Some(cand) = try_parse_paragraph_header_candidate(
                    children,
                    i,
                    candidates.len(),
                    default_doc_type,
                ) {
                    let next_i = cand.summary.body_end_index + 1;
                    candidates.push(cand);
                    i = next_i;
                    continue;
                }
            }
        }
        i += 1;
    }

    candidates
}

fn has_page_or_section_break(el: &XmlElement) -> bool {
    if el.name == "w:sectPr" {
        return true;
    }
    if el.name == "w:br" && el.get_attr("w:type") == Some("page") {
        return true;
    }
    el.children.iter().any(|n| match n {
        XmlNode::Element(sub) => has_page_or_section_break(sub),
        _ => false,
    })
}

fn element_has_logo_drawing(el: &XmlElement) -> bool {
    if matches!(
        el.name.as_str(),
        "w:drawing" | "w:pict" | "mc:AlternateContent"
    ) {
        return true;
    }
    el.children.iter().any(|n| match n {
        XmlNode::Element(sub) => element_has_logo_drawing(sub),
        _ => false,
    })
}

fn is_header_keyword_table(tbl: &XmlElement) -> (bool, usize, usize) {
    let mut row_count = 0usize;
    let mut max_cols = 0usize;
    for node in &tbl.children {
        if let XmlNode::Element(tr) = node {
            if tr.name == "w:tr" {
                row_count += 1;
                let mut cols = 0usize;
                for c in &tr.children {
                    if let XmlNode::Element(tc) = c {
                        if tc.name == "w:tc" {
                            let span = tc
                                .child("w:tcPr")
                                .and_then(|tp| tp.child("w:gridSpan"))
                                .and_then(|gs| gs.get_attr("w:val"))
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1)
                                .max(1);
                            cols += span;
                        }
                    }
                }
                max_cols = max_cols.max(cols);
            }
        }
    }
    if row_count == 0 || row_count > 6 || max_cols == 0 || max_cols > 5 {
        return (false, row_count, max_cols);
    }
    let upper = collapse_spaces_upper(&tbl.extract_text());
    let has_qh = has_national_or_motto_cue(&upper);
    let has_num_date =
        upper.contains("SỐ:") || upper.contains("SỐ :") || (upper.contains("NGÀY ") && upper.contains("THÁNG "));
    let has_org = upper.contains("ỦY BAN")
        || upper.contains("UBND")
        || upper.contains("CÔNG TY")
        || upper.contains("BỘ ")
        || upper.contains("SỞ ")
        || upper.contains("BAN ")
        || upper.contains("CHI BỘ")
        || upper.contains("ĐẢNG ỦY")
        || upper.contains("TRƯỜNG ")
        || upper.contains("TẬP ĐOÀN");

    let is_hdr = has_qh || (row_count <= 4 && has_num_date && (has_org || max_cols <= 3));
    (is_hdr, row_count, max_cols)
}

fn try_parse_table_header_candidate(
    children: &[XmlNode],
    tbl_idx: usize,
    candidate_index: usize,
    default_doc_type: AdminDocType,
) -> Option<CandidateInternal> {
    let XmlNode::Element(tbl) = &children[tbl_idx] else {
        return None;
    };
    let (is_hdr, _rows, max_cols) = is_header_keyword_table(tbl);
    if !is_hdr {
        return None;
    }

    let mut has_logo = element_has_logo_drawing(tbl);
    let mut preserved_logo_paras: Vec<XmlElement> = Vec::new();
    let mut body_start = tbl_idx;

    // Check immediately preceding 1-2 paragraphs for standalone top logo right above table
    if tbl_idx >= 1 {
        if let XmlNode::Element(prev_p) = &children[tbl_idx - 1] {
            if prev_p.name == "w:p" && prev_p.extract_text().trim().is_empty() && element_has_logo_drawing(prev_p) {
                has_logo = true;
                preserved_logo_paras.push(prev_p.clone());
                body_start = tbl_idx - 1;
            }
        }
    }

    // Collect drawings inside left cells of header table
    collect_logo_paras_in_table(tbl, &mut preserved_logo_paras);

    // Check if complex multi-column custom banner (>= 4 cols without clear 2/3-col split)
    let is_custom_complex = max_cols >= 4 && has_logo;
    if is_custom_complex {
        let snippet = tbl.extract_text().trim().chars().take(90).collect();
        return Some(CandidateInternal {
            summary: HeaderCandidateSummary {
                candidate_index,
                layout_kind: HeaderLayoutKind::CustomTemplateWithLogo,
                body_start_index: body_start,
                body_end_index: tbl_idx,
                has_logo_or_image: true,
                preview_snippet: snippet,
                fields: DetectedHeaderFields::default(),
            },
            preserved_logo_paras,
            preserved_sect_pr: None,
            typos_or_format_issues: Vec::new(),
        });
    }

    let mut fields = DetectedHeaderFields::default();
    let mut issues: Vec<(String, AuditSeverity, String)> = Vec::new();
    let mut preserved_sect_pr: Option<XmlElement> = None;

    parse_table_rows_into_fields(tbl, &mut fields, &mut issues);

    // Scan trailing docType / subject / divider paragraphs right below table
    let mut body_end = tbl_idx;
    let mut j = tbl_idx + 1;
    let max_look = (tbl_idx + 10).min(children.len());
    while j < max_look {
        let XmlNode::Element(el) = &children[j] else {
            j += 1;
            continue;
        };
        if el.name != "w:p" {
            break;
        }
        let txt = el.extract_text();
        let trimmed = txt.trim();
        let upper = collapse_spaces_upper(trimmed);
        if is_body_boundary_paragraph(el, &upper) {
            break;
        }
        let is_part = is_post_table_header_para(el, trimmed, &upper, &fields);
        if !is_part {
            break;
        }
        if let Some(sect) = el.child("w:pPr").and_then(|pp| pp.child("w:sectPr")) {
            preserved_sect_pr = Some(sect.clone());
        }
        assign_post_table_para(el, trimmed, &upper, &mut fields, &mut issues);
        body_end = j;
        if trimmed.is_empty() && el.child("w:pPr").and_then(|pp| pp.child("w:pBdr")).is_some() {
            break;
        }
        j += 1;
    }

    if fields.doc_type_label.is_empty() && !fields.subject.is_empty() && fields.subject.to_lowercase().starts_with("v/v") {
        fields.doc_type_label = "CÔNG VĂN".to_string();
    } else if fields.doc_type_label.is_empty() {
        let _ = default_doc_type;
    }

    let layout_kind = if body_end > tbl_idx && !fields.doc_type_label.is_empty() {
        HeaderLayoutKind::MixedTableAndParagraphs
    } else {
        HeaderLayoutKind::TableLayout
    };

    let snippet = build_candidate_snippet(&fields, &tbl.extract_text());

    Some(CandidateInternal {
        summary: HeaderCandidateSummary {
            candidate_index,
            layout_kind,
            body_start_index: body_start,
            body_end_index: body_end,
            has_logo_or_image: has_logo,
            preview_snippet: snippet,
            fields,
        },
        preserved_logo_paras,
        preserved_sect_pr,
        typos_or_format_issues: issues,
    })
}

fn try_parse_paragraph_header_candidate(
    children: &[XmlNode],
    start_idx: usize,
    candidate_index: usize,
    _default_doc_type: AdminDocType,
) -> Option<CandidateInternal> {
    let mut end_idx = start_idx;
    let mut lines: Vec<&XmlElement> = Vec::new();
    let max_scan = (start_idx + 18).min(children.len());
    let mut seen_qh_or_motto = false;
    let mut seen_date_or_num = false;
    let mut seen_doc_type = false;

    let mut idx = start_idx;
    while idx < max_scan {
        let XmlNode::Element(el) = &children[idx] else {
            break;
        };
        if el.name != "w:p" {
            break;
        }
        let raw = el.extract_text();
        let trimmed = raw.trim();
        let upper = collapse_spaces_upper(trimmed);
        if is_body_boundary_paragraph(el, &upper) {
            break;
        }
        if trimmed.chars().count() > 140 && !is_doc_type_cue(&upper) && !upper.starts_with("VỀ VIỆC") && !upper.starts_with("V/V") {
            break;
        }
        let is_qh = is_national_header_line(&upper) || is_motto_line(&upper);
        let is_nd = is_number_symbol_line(&upper) || is_location_date_line(&upper);
        let is_dt = is_doc_type_cue(&upper);
        let is_header_cue = is_qh
            || is_nd
            || is_dt
            || is_divider_text(trimmed)
            || upper.starts_with("VỀ VIỆC")
            || upper.starts_with("V/V");

        if (seen_qh_or_motto || seen_date_or_num) && !trimmed.is_empty() && !is_header_cue && !raw.contains('\t') {
            let jc = el
                .child("w:pPr")
                .and_then(|pp| pp.child("w:jc"))
                .and_then(|j| j.get_attr("w:val"))
                .unwrap_or("");
            let allow_subject_line = seen_doc_type
                && (is_para_bold(el) || jc == "center")
                && !trimmed.ends_with('.');
            if !allow_subject_line {
                break;
            }
        }

        if is_qh {
            seen_qh_or_motto = true;
        }
        if is_nd {
            seen_date_or_num = true;
        }
        if is_dt {
            seen_doc_type = true;
        }

        lines.push(el);
        end_idx = idx;
        if el.child("w:pPr").and_then(|pp| pp.child("w:sectPr")).is_some() {
            break;
        }
        idx += 1;
    }

    let combined_upper: String = lines
        .iter()
        .map(|p| collapse_spaces_upper(&p.extract_text()))
        .collect::<Vec<_>>()
        .join("\n");

    let has_qh = has_national_or_motto_cue(&combined_upper);
    let has_num = combined_upper.contains("SỐ:") || combined_upper.contains("SỐ :");
    let has_date = combined_upper.contains("NGÀY ") && combined_upper.contains("THÁNG ");
    if !has_qh && !(has_num && has_date) {
        return None;
    }

    let mut fields = DetectedHeaderFields::default();
    let mut issues = Vec::new();
    let mut preserved_logos = Vec::new();
    let mut preserved_sect_pr = None;
    let mut has_logo = false;
    let mut org_lines_buf: Vec<(String, bool)> = Vec::new();

    for p in &lines {
        if let Some(sect) = p.child("w:pPr").and_then(|pp| pp.child("w:sectPr")) {
            preserved_sect_pr = Some(sect.clone());
        }
        if element_has_logo_drawing(p) {
            has_logo = true;
            if p.extract_text().trim().is_empty() {
                preserved_logos.push((*p).clone());
                continue;
            }
        }
        let raw = p.extract_text();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Check if a paragraph contains tab-separated Left \t Right columns
        if raw.contains('\t') {
            let parts: Vec<&str> = raw.split('\t').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if parts.len() >= 2 {
                classify_left_fragment(parts[0], p, &mut org_lines_buf, &mut fields, &mut issues);
                classify_right_fragment(parts[parts.len() - 1], p, &mut fields, &mut issues);
                continue;
            }
        }
        let upper = collapse_spaces_upper(trimmed);
        if is_motto_line(&upper) {
            fields.motto = trimmed.to_string();
        } else if is_national_header_line(&upper) {
            fields.national_header = trimmed.to_string();
        } else if is_number_symbol_line(&upper) {
            parse_number_symbol_str(trimmed, &mut fields, &mut issues);
        } else if is_location_date_line(&upper) {
            parse_location_date_str(trimmed, &mut fields, &mut issues);
        } else if is_doc_type_cue(&upper) && fields.doc_type_label.is_empty() {
            fields.doc_type_label = trimmed.to_uppercase();
        } else if upper.starts_with("V/V") || upper.starts_with("VỀ VIỆC") || !fields.doc_type_label.is_empty() {
            if trimmed.chars().all(|c| c == '-' || c == '_' || c == 'o' || c == '0' || c == '*') {
                issues.push((
                    "ND30-PLI-01B".to_string(),
                    AuditSeverity::Warning,
                    format!("Đang dùng ký tự \"{}\" làm đường kẻ; nên thay bằng đường kẻ nét liền chuẩn.", trimmed),
                ));
            } else if fields.subject.is_empty() {
                fields.subject = trimmed.to_string();
            } else if fields.summary_line.is_empty() {
                fields.summary_line = trimmed.to_string();
            }
        } else {
            classify_left_fragment(trimmed, p, &mut org_lines_buf, &mut fields, &mut issues);
        }
    }

    finalize_org_lines(&org_lines_buf, &mut fields);
    let snippet = build_candidate_snippet(&fields, &combined_upper);

    Some(CandidateInternal {
        summary: HeaderCandidateSummary {
            candidate_index,
            layout_kind: HeaderLayoutKind::ParagraphLayout,
            body_start_index: start_idx,
            body_end_index: end_idx,
            has_logo_or_image: has_logo,
            preview_snippet: snippet,
            fields,
        },
        preserved_logo_paras: preserved_logos,
        preserved_sect_pr,
        typos_or_format_issues: issues,
    })
}

fn collect_logo_paras_in_table(tbl: &XmlElement, out: &mut Vec<XmlElement>) {
    for tr_n in &tbl.children {
        if let XmlNode::Element(tr) = tr_n {
            if tr.name == "w:tr" {
                for tc_n in &tr.children {
                    if let XmlNode::Element(tc) = tc_n {
                        if tc.name == "w:tc" {
                            for p_n in &tc.children {
                                if let XmlNode::Element(p) = p_n {
                                    if p.name == "w:p" && element_has_logo_drawing(p) {
                                        let mut clean_logo = p.clone();
                                        // Strip any stray text runs inside the logo wrapper if text is empty
                                        if clean_logo.extract_text().trim().is_empty() {
                                            out.push(clean_logo);
                                        } else {
                                            clean_logo.children.retain(|cn| {
                                                matches!(cn, XmlNode::Element(e) if e.name == "w:pPr" || (e.name == "w:r" && element_has_logo_drawing(e)))
                                            });
                                            out.push(clean_logo);
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

fn parse_table_rows_into_fields(
    tbl: &XmlElement,
    fields: &mut DetectedHeaderFields,
    issues: &mut Vec<(String, AuditSeverity, String)>,
) {
    let mut org_lines: Vec<(String, bool)> = Vec::new();
    for tr_n in &tbl.children {
        let XmlNode::Element(tr) = tr_n else {
            continue;
        };
        if tr.name != "w:tr" {
            continue;
        }
        let cells: Vec<&XmlElement> = tr
            .children_elements()
            .filter(|e| e.name == "w:tc")
            .collect();
        if cells.is_empty() {
            continue;
        }
        let left_cell = cells[0];
        let right_cell = if cells.len() >= 2 {
            Some(*cells.last().unwrap())
        } else {
            None
        };

        for p in left_cell.children_elements().filter(|e| e.name == "w:p") {
            let txt = p.extract_text();
            let trimmed = txt.trim();
            if p.child("w:pPr").and_then(|pp| pp.child("w:pBdr")).is_some() && trimmed.is_empty() {
                fields.has_org_line = true;
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            let upper = collapse_spaces_upper(trimmed);
            if is_divider_text(trimmed) {
                issues.push((
                    "ND30-PLI-02B".to_string(),
                    AuditSeverity::Info,
                    format!("Đang dùng ký tự \"{}\" làm gạch chân cơ quan; sẽ chuẩn hóa sang đường kẻ nét liền 1/3–1/2.", trimmed),
                ));
                continue;
            }
            if is_number_symbol_line(&upper) {
                parse_number_symbol_str(trimmed, fields, issues);
            } else if upper.starts_with("V/V") || upper.starts_with("VỀ VIỆC") {
                fields.subject = trimmed.to_string();
                if fields.doc_type_label.is_empty() {
                    fields.doc_type_label = "CÔNG VĂN".to_string();
                }
            } else if matches!(upper.as_str(), "HỎA TỐC" | "KHẨN" | "THƯỢNG KHẨN" | "MẬT" | "TỐI MẬT" | "TUYỆT MẬT") {
                fields.classification_mark = trimmed.to_string();
            } else {
                let bold = is_para_bold(p);
                org_lines.push((trimmed.to_string(), bold));
            }
        }

        if let Some(rc) = right_cell {
            for p in rc.children_elements().filter(|e| e.name == "w:p") {
                let txt = p.extract_text();
                let trimmed = txt.trim();
                if p.child("w:pPr").and_then(|pp| pp.child("w:pBdr")).is_some() && trimmed.is_empty() {
                    fields.has_motto_line = true;
                    continue;
                }
                if trimmed.is_empty() {
                    continue;
                }
                if is_divider_text(trimmed) {
                    issues.push((
                        "ND30-PLI-01B".to_string(),
                        AuditSeverity::Warning,
                        format!("Đang dùng ký tự \"{}\" làm đường kẻ Tiêu ngữ; sẽ chuẩn hóa sang đường kẻ nét liền chuẩn.", trimmed),
                    ));
                    continue;
                }
                classify_right_fragment(trimmed, p, fields, issues);
            }
        }
    }

    finalize_org_lines(&org_lines, fields);
}

fn classify_left_fragment(
    trimmed: &str,
    p: &XmlElement,
    org_lines: &mut Vec<(String, bool)>,
    fields: &mut DetectedHeaderFields,
    issues: &mut Vec<(String, AuditSeverity, String)>,
) {
    let upper = collapse_spaces_upper(trimmed);
    if is_divider_text(trimmed) {
        issues.push((
            "ND30-PLI-02B".to_string(),
            AuditSeverity::Info,
            "Ký tự gạch ngang thủ công sẽ được thay bằng đường kẻ nét liền chuẩn.".to_string(),
        ));
        return;
    }
    if is_number_symbol_line(&upper) {
        parse_number_symbol_str(trimmed, fields, issues);
    } else if upper.starts_with("V/V") {
        fields.subject = trimmed.to_string();
    } else {
        org_lines.push((trimmed.to_string(), is_para_bold(p)));
    }
}

fn classify_right_fragment(
    trimmed: &str,
    p: &XmlElement,
    fields: &mut DetectedHeaderFields,
    issues: &mut Vec<(String, AuditSeverity, String)>,
) {
    let upper = collapse_spaces_upper(trimmed);
    if is_national_header_line(&upper) {
        fields.national_header = trimmed.to_string();
    } else if is_motto_line(&upper) {
        fields.motto = trimmed.to_string();
    } else if is_location_date_line(&upper) {
        parse_location_date_str(trimmed, fields, issues);
        if !is_para_italic(p) {
            issues.push((
                "ND30-PLI-04".to_string(),
                AuditSeverity::Info,
                "Dòng Địa danh và ngày tháng chưa bật kiểu chữ nghiêng theo Phụ lục I NĐ30.".to_string(),
            ));
        }
    }
}

fn finalize_org_lines(lines: &[(String, bool)], fields: &mut DetectedHeaderFields) {
    match lines.len() {
        0 => {}
        1 => {
            fields.issuing_org = lines[0].0.clone();
        }
        2 => {
            fields.governing_org = lines[0].0.clone();
            fields.issuing_org = lines[1].0.clone();
        }
        3 => {
            fields.governing_org = lines[0].0.clone();
            fields.issuing_org = lines[1].0.clone();
            fields.affiliated_unit = lines[2].0.clone();
        }
        _ => {
            fields.governing_org = lines[0].0.clone();
            fields.issuing_org = lines[1].0.clone();
            fields.affiliated_unit = lines[2].0.clone();
            fields.department = lines[3].0.clone();
        }
    }
}

fn parse_number_symbol_str(
    trimmed: &str,
    fields: &mut DetectedHeaderFields,
    issues: &mut Vec<(String, AuditSeverity, String)>,
) {
    fields.number_raw = trimmed.to_string();
    let after_colon = trimmed
        .split_once(':')
        .map(|(_, rhs)| rhs)
        .unwrap_or(trimmed.strip_prefix("Số").or_else(|| trimmed.strip_prefix("SỐ")).unwrap_or(trimmed));
    if let Some((num_part, sym_part)) = after_colon.split_once('/') {
        let num = num_part.trim().to_string();
        let sym = sym_part.trim().to_string();
        if let Ok(n) = num.parse::<u32>() {
            if (1..=9).contains(&n) && !num.starts_with('0') {
                issues.push((
                    "ND30-PLI-03".to_string(),
                    AuditSeverity::Info,
                    format!("Số văn bản nhỏ hơn 10 (\"{}\") cần viết thêm số 0 thành \"0{}\".", num, n),
                ));
            }
        }
        if !sym.is_empty() && sym.contains(' ') {
            issues.push((
                "ND30-PLI-03".to_string(),
                AuditSeverity::Info,
                format!("Ký hiệu văn bản \"{}\" chứa khoảng trắng không đúng quy định.", sym),
            ));
        }
        fields.number_value = num;
        fields.symbol_value = sym;
    } else {
        fields.number_value = after_colon.trim().to_string();
    }
}

fn parse_location_date_str(
    trimmed: &str,
    fields: &mut DetectedHeaderFields,
    issues: &mut Vec<(String, AuditSeverity, String)>,
) {
    fields.date_raw = trimmed.to_string();
    let lower = trimmed.to_lowercase();
    if let Some(pos) = lower.find("ngày") {
        let before = trimmed[..pos].trim();
        let loc = before.trim_end_matches(',').trim();
        if !loc.is_empty() {
            if !before.ends_with(',') {
                issues.push((
                    "ND30-PLI-04".to_string(),
                    AuditSeverity::Info,
                    format!("Sau địa danh \"{}\" thiếu dấu phẩy theo Khoản 4 Mục II Phụ lục I NĐ30.", loc),
                ));
            }
            fields.location = loc.to_string();
        }
        let date_part = trimmed[pos..].trim();
        fields.date_raw = date_part.to_string();
        if let Some(caps) = re_date_parts().captures(date_part) {
            let d = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim().to_string();
            let m = caps.get(2).map(|m| m.as_str()).unwrap_or("").trim().to_string();
            let y = caps.get(3).map(|m| m.as_str()).unwrap_or("").trim().to_string();
            if let Ok(dv) = d.parse::<u32>() {
                if (1..=9).contains(&dv) && !d.starts_with('0') {
                    issues.push((
                        "ND30-PLI-04".to_string(),
                        AuditSeverity::Info,
                        format!("Ngày \"{}\" cần viết 2 chữ số (\"0{}\") theo NĐ30.", dv, dv),
                    ));
                }
            }
            if let Ok(mv) = m.parse::<u32>() {
                if (mv == 1 || mv == 2) && !m.starts_with('0') {
                    issues.push((
                        "ND30-PLI-04".to_string(),
                        AuditSeverity::Info,
                        format!("Tháng \"{}\" cần viết \"0{}\" theo NĐ30.", mv, mv),
                    ));
                }
            }
            fields.day = d;
            fields.month = m;
            fields.year = y;
        }
    }
}

fn is_post_table_header_para(
    p: &XmlElement,
    trimmed: &str,
    upper: &str,
    fields: &DetectedHeaderFields,
) -> bool {
    if p.child("w:pPr").and_then(|pp| pp.child("w:pBdr")).is_some() && trimmed.is_empty() {
        return true;
    }
    if trimmed.is_empty() {
        return !fields.doc_type_label.is_empty() && fields.subject.is_empty();
    }
    if is_divider_text(trimmed) {
        return true;
    }
    if is_doc_type_cue(upper) {
        return true;
    }
    if upper.starts_with("VỀ VIỆC") || upper.starts_with("V/V") || upper.starts_with("(BAN HÀNH") {
        return true;
    }
    // Continuation of centered bold subject or quoted sub-title right below docType
    if !fields.doc_type_label.is_empty() && trimmed.chars().count() <= 140 {
        let jc = p
            .child("w:pPr")
            .and_then(|pp| pp.child("w:jc"))
            .and_then(|j| j.get_attr("w:val"))
            .unwrap_or("");
        return jc == "center" || is_para_bold(p) || trimmed.starts_with('“') || trimmed.starts_with('"');
    }
    false
}

fn assign_post_table_para(
    _p: &XmlElement,
    trimmed: &str,
    upper: &str,
    fields: &mut DetectedHeaderFields,
    issues: &mut Vec<(String, AuditSeverity, String)>,
) {
    if trimmed.is_empty() {
        return;
    }
    if is_divider_text(trimmed) {
        issues.push((
            "ND30-PLI-05A".to_string(),
            AuditSeverity::Info,
            "Đường gạch dưới trích yếu bằng ký tự sẽ được chuẩn hóa sang đường kẻ nét liền.".to_string(),
        ));
        return;
    }
    if fields.doc_type_label.is_empty() && is_doc_type_cue(upper) {
        fields.doc_type_label = trimmed.to_uppercase();
    } else if fields.subject.is_empty() {
        fields.subject = trimmed.to_string();
    } else if fields.summary_line.is_empty() {
        fields.summary_line = trimmed.to_string();
    } else {
        fields.summary_line = format!("{} {}", fields.summary_line, trimmed);
    }
}

fn is_body_boundary_paragraph(p: &XmlElement, upper: &str) -> bool {
    if upper.starts_with("KÍNH GỬI")
        || upper.starts_with("CĂN CỨ")
        || upper.starts_with("XÉT ĐỀ NGHỊ")
        || upper.starts_with("THEO ĐỀ NGHỊ")
        || upper.starts_with("PHẦN ")
        || upper.starts_with("CHƯƠNG ")
        || upper.starts_with("ĐIỀU ")
        || upper.starts_with("NƠI NHẬN")
    {
        return true;
    }
    let style = p
        .child("w:pPr")
        .and_then(|pp| pp.child("w:pStyle"))
        .and_then(|s| s.get_attr("w:val"))
        .unwrap_or("");
    style.starts_with("Heading")
}

fn is_para_bold(p: &XmlElement) -> bool {
    p.children_elements().filter(|e| e.name == "w:r").any(|r| {
        r.child("w:rPr")
            .and_then(|rp| rp.child("w:b"))
            .map(|b| b.get_attr("w:val") != Some("0"))
            .unwrap_or(false)
    })
}

fn is_para_italic(p: &XmlElement) -> bool {
    p.children_elements().filter(|e| e.name == "w:r").any(|r| {
        r.child("w:rPr")
            .and_then(|rp| rp.child("w:i"))
            .map(|b| b.get_attr("w:val") != Some("0"))
            .unwrap_or(false)
    })
}

fn is_national_header_line(upper: &str) -> bool {
    upper.contains("CỘNG HÒA XÃ HỘI")
        || upper.contains("CỘNG HOÀ XÃ HỘI")
        || upper.contains("ĐẢNG CỘNG SẢN VIỆT NAM")
}

fn is_motto_line(upper: &str) -> bool {
    upper.contains("ĐỘC LẬP") && (upper.contains("TỰ DO") || upper.contains("HẠNH PHÚC"))
}

fn has_national_or_motto_cue(upper: &str) -> bool {
    is_national_header_line(upper) || is_motto_line(upper)
}

fn is_number_symbol_line(upper: &str) -> bool {
    upper.starts_with("SỐ:") || upper.starts_with("SỐ :") || (upper.starts_with("SỐ ") && upper.contains('/'))
}

fn is_location_date_line(upper: &str) -> bool {
    upper.contains("NGÀY ") && upper.contains("THÁNG ")
}

fn is_divider_text(trimmed: &str) -> bool {
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| matches!(c, '-' | '_' | '–' | '—' | '*' | 'o' | 'O' | '0' | '=' | ' '))
        && trimmed.chars().any(|c| matches!(c, '-' | '_' | '–' | '—' | '*'))
}

fn is_doc_type_cue(upper: &str) -> bool {
    const TYPES: &[&str] = &[
        "QUYẾT ĐỊNH",
        "TỜ TRÌNH",
        "BÁO CÁO",
        "THÔNG BÁO",
        "KẾ HOẠCH",
        "BIÊN BẢN",
        "CHỈ THỊ",
        "NGHỊ QUYẾT",
        "ĐỀ ÁN",
        "QUY CHẾ",
        "HỢP ĐỒNG",
        "QUY ĐỊNH",
        "CHƯƠNG TRÌNH",
        "PHƯƠNG ÁN",
        "GIẤY MỜI",
        "CÔNG ĐIỆN",
        "HƯỚNG DẪN",
    ];
    TYPES.iter().any(|t| upper.starts_with(t))
}

fn normalize_motto_display(raw: &str) -> String {
    let up = collapse_spaces_upper(raw);
    if up.contains("ĐỘC LẬP") && up.contains("TỰ DO") && up.contains("HẠNH PHÚC") {
        STD_MOTTO.to_string()
    } else {
        raw.trim().to_string()
    }
}

fn expected_national_header(cfg: &HeaderBuilderConfig, sector: OrgSector) -> String {
    if !cfg.custom_national_header.trim().is_empty() {
        cfg.custom_national_header.trim().to_string()
    } else if sector == OrgSector::PartyOrg {
        STD_PARTY_HEADER.to_string()
    } else {
        STD_NATIONAL_HEADER.to_string()
    }
}

fn build_candidate_snippet(fields: &DetectedHeaderFields, fallback: &str) -> String {
    let mut parts = Vec::new();
    if !fields.issuing_org.is_empty() {
        parts.push(fields.issuing_org.as_str());
    }
    if !fields.national_header.is_empty() {
        parts.push(fields.national_header.as_str());
    }
    if !fields.number_raw.is_empty() {
        parts.push(fields.number_raw.as_str());
    }
    if !fields.doc_type_label.is_empty() {
        parts.push(fields.doc_type_label.as_str());
    }
    if parts.is_empty() {
        fallback.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(90).collect()
    } else {
        parts.join(" · ")
    }
}

fn re_date_parts() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)ngày\s*(\d{0,2})\s*tháng\s*(\d{0,2})\s*năm\s*(\d{0,4})")
            .expect("valid date regex")
    })
}

pub(crate) fn build_empty_valid_docx_package() -> OpenXmlPackage {
    let mut pkg = OpenXmlPackage { entries: Vec::new() };
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
</Types>"#;
    pkg.set_xml("[Content_Types].xml", content_types.to_string());

    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    pkg.set_xml("_rels/.rels", root_rels.to_string());

    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
</Relationships>"#;
    pkg.set_xml("word/_rels/document.xml.rels", doc_rels.to_string());

    let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p><w:pPr><w:spacing w:before="0" w:after="120" w:line="276" w:lineRule="auto"/><w:ind w:firstLine="720"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr><w:t></w:t></w:r></w:p>
    <w:sectPr>
      <w:pgSz w:w="11906" w:h="16838"/>
      <w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1701" w:header="709" w:footer="709"/>
    </w:sectPr>
  </w:body>
</w:document>"#;
    pkg.set_xml("word/document.xml", doc_xml.to_string());

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
</w:styles>"#;
    pkg.set_xml("word/styles.xml", styles_xml.to_string());

    let settings_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;
    pkg.set_xml("word/settings.xml", settings_xml.to_string());

    pkg
}
