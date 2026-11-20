use crate::engine::formatter::collapse_spaces_upper;
use crate::engine::std_engine::ir::{is_non_replaceable_font, DocumentIr, ParagraphInfo, ParagraphRole};
use crate::models::{
    FixSafetyTier, OrgSector, OrganizationProfile, RuleCategory, RuleCheckStatus, StdDocScope,
    StdFinding, StdRuleDescriptor, StdSeverity,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct RuleEvalContext<'a> {
    pub org_profile: Option<&'a OrganizationProfile>,
    pub disabled_rule_ids: &'a [String],
}

pub struct RuleCheckResult {
    pub status: RuleCheckStatus,
    pub findings: Vec<StdFinding>,
}

pub struct StdRuleDefinition {
    pub descriptor: StdRuleDescriptor,
    pub check_fn: fn(&DocumentIr, &RuleEvalContext) -> RuleCheckResult,
}

/// Phụ lục III NĐ 30/2020/NĐ-CP: 27 chữ viết tắt tên loại văn bản hành chính chính thức.
pub const PHU_LUC_III_ABBR: &[(&str, &str)] = &[
    ("NQ", "Nghị quyết (cá biệt)"),
    ("QĐ", "Quyết định (cá biệt)"),
    ("CT", "Chỉ thị"),
    ("QC", "Quy chế"),
    ("QyĐ", "Quy định"),
    ("TC", "Thông cáo"),
    ("TB", "Thông báo"),
    ("HD", "Hướng dẫn"),
    ("CTr", "Chương trình"),
    ("KH", "Kế hoạch"),
    ("PA", "Phương án"),
    ("ĐA", "Đề án"),
    ("DA", "Dự án"),
    ("BC", "Báo cáo"),
    ("BB", "Biên bản"),
    ("TTr", "Tờ trình"),
    ("HĐ", "Hợp đồng"),
    ("CĐ", "Công điện"),
    ("BGN", "Bản ghi nhớ"),
    ("BTT", "Bản thỏa thuận"),
    ("GUQ", "Giấy ủy quyền"),
    ("GM", "Giấy mời"),
    ("GGT", "Giấy giới thiệu"),
    ("GNP", "Giấy nghỉ phép"),
    ("PG", "Phiếu gửi"),
    ("PC", "Phiếu chuyển"),
    ("PB", "Phiếu báo"),
];

pub fn build_rule_registry() -> Vec<StdRuleDefinition> {
    use RuleCategory::*;
    use FixSafetyTier::*;
    use StdDocScope::*;
    use StdSeverity::*;

    let all_core = vec![
        AdminNd30,
        PartyDocument,
        EnterpriseInternal,
        SpecializedLegal,
        AcademicThesis,
        Freeform,
    ];
    let admin_and_party = vec![AdminNd30, PartyDocument, EnterpriseInternal];
    let admin_strict = vec![AdminNd30, PartyDocument];

    let mut defs = vec![
        // 4.1 Page & Section
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PAGE-01".into(),
                rule_name: "Khổ giấy chuẩn A4 (210 × 297 mm)".into(),
                category: PageAndSection,
                description: "Kiểm tra kích thước khổ giấy của từng Section theo chuẩn A4 (dọc hoặc ngang).".into(),
                legal_basis: "Khoản 1 Điều 9 & Mục I Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: vec![AdminNd30, PartyDocument, EnterpriseInternal, AcademicThesis],
                severity: Error,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_page_01_a4,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PAGE-02".into(),
                rule_name: "Định lề trang văn bản hành chính".into(),
                category: PageAndSection,
                description: "Trên/Dưới 20–25 mm; Trái 30–35 mm; Phải 15–20 mm (trang ngang đảo lề đóng gáy mép trên).".into(),
                legal_basis: "Mục I.2 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_and_party.clone(),
                severity: Error,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_page_02_margins,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PAGE-03".into(),
                rule_name: "Cấu trúc đa Section & Trang ngang".into(),
                category: PageAndSection,
                description: "Phát hiện cấu hình nhiều Section/hướng giấy khác nhau và đảm bảo không gộp hỏng bố cục.".into(),
                legal_basis: "Mục I.1 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Information,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: false,
                enabled: true,
            },
            check_fn: check_page_03_sections,
        },
        // 4.2 Font & Character
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "FONT-01".into(),
                rule_name: "Bộ mã tiếng Việt Unicode TCVN 6909:2001".into(),
                category: FontAndEncoding,
                description: "Phát hiện văn bản sử dụng bảng mã cũ TCVN3 (.Vn*) hoặc VNI (VNI-*) và chuyển sang Unicode chuẩn.".into(),
                legal_basis: "Khoản 2 Điều 9 & Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Critical,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_font_01_legacy_encoding,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "FONT-02".into(),
                rule_name: "Phông chữ Times New Roman & tính đồng nhất".into(),
                category: FontAndEncoding,
                description: "Kiểm tra phông Times New Roman, thiếu khai báo phông hoặc trộn nhiều phông trong đoạn (trừ ký hiệu/công thức/ảnh).".into(),
                legal_basis: "Khoản 2 Điều 9 & Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: vec![AdminNd30, PartyDocument, EnterpriseInternal, AcademicThesis],
                severity: Error,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_font_02_family,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "FONT-03".into(),
                rule_name: "Cỡ chữ theo từng thành phần thể thức".into(),
                category: FontAndEncoding,
                description: "Lời văn 13–14pt; Quốc hiệu 12–13pt; Tiêu ngữ 13–14pt; Nơi nhận 11–12pt; Bảng dữ liệu 11–13pt.".into(),
                legal_basis: "Mục II & Bảng tổng hợp Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_and_party.clone(),
                severity: Error,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_font_03_component_sizes,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "FONT-04".into(),
                rule_name: "Màu chữ đen chuẩn (000000)".into(),
                category: FontAndEncoding,
                description: "Màu chữ nội dung văn bản hành chính dùng màu đen, trừ siêu liên kết (Hyperlink).".into(),
                legal_basis: "Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_and_party.clone(),
                severity: Warning,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_font_04_black_color,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "FONT-05".into(),
                rule_name: "Khoảng trắng thừa, dấu câu & ký tự điều khiển".into(),
                category: FontAndEncoding,
                description: "Phát hiện nhiều dấu cách liên tiếp, khoảng trắng trước dấu câu và ký tự điều khiển bất thường (giữ nguyên khoảng trống điền số/ngày).".into(),
                legal_basis: "Kỹ thuật trình bày soạn thảo văn bản NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Warning,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_font_05_whitespace_ctrl,
        },
        // 4.3 Paragraph & Alignment
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PARA-01".into(),
                rule_name: "Căn đều hai bên phần lời văn chính".into(),
                category: ParagraphAndAlignment,
                description: "Các đoạn lời văn chính căn đều hai bên (Justify); không đụng bảng đầu trang, tiêu đề căn giữa, chữ ký hay nơi nhận.".into(),
                legal_basis: "Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_and_party.clone(),
                severity: Error,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_para_01_alignment,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PARA-02".into(),
                rule_name: "Thụt đầu dòng lời văn (1.0–1.27 cm)".into(),
                category: ParagraphAndAlignment,
                description: "Đoạn lời văn chính khi xuống dòng thụt đầu dòng 1 cm hoặc 1.27 cm; không thụt lề trái/phải bất thường.".into(),
                legal_basis: "Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_and_party.clone(),
                severity: Warning,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_para_02_indent,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PARA-03".into(),
                rule_name: "Giãn dòng (Single–1.5 lines) & Khoảng cách đoạn (>= 6pt)".into(),
                category: ParagraphAndAlignment,
                description: "Khoảng cách giữa các dòng từ dòng đơn đến 1.5 lines; khoảng cách đoạn tối thiểu 6pt và không quá lớn.".into(),
                legal_basis: "Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: vec![AdminNd30, PartyDocument, EnterpriseInternal, AcademicThesis],
                severity: Warning,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_para_03_spacing,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PARA-04".into(),
                rule_name: "Đoạn trống thừa & ngắt dòng mềm (Shift+Enter)".into(),
                category: ParagraphAndAlignment,
                description: "Phát hiện đoạn trống thừa liên tiếp và dấu ngắt dòng mềm trong thân văn bản (bảo toàn khoảng trống chữ ký & đường kẻ pBdr).".into(),
                legal_basis: "Kỹ thuật trình bày OpenXML NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Warning,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_para_04_empty_and_softbreak,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PARA-05".into(),
                rule_name: "Xung đột định dạng trực tiếp với Paragraph Style".into(),
                category: ParagraphAndAlignment,
                description: "Phát hiện đoạn có thuộc tính trực tiếp ghi đè ngược với Style đang gán (cần xác nhận trước khi đồng bộ).".into(),
                legal_basis: "Tính nhất quán cấu trúc Style OpenXML".into(),
                applicable_document_types: all_core.clone(),
                severity: Information,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_para_05_style_conflict,
        },
        // 4.4 Heading & Content Structure
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "STRUCT-01".into(),
                rule_name: "Trình bày Tên loại, Trích yếu & Căn cứ pháp lý".into(),
                category: HeadingAndStructure,
                description: "Kiểm tra khối Căn cứ (in nghiêng, 13–14pt) và Tên loại văn bản (in hoa, đứng, đậm, căn giữa).".into(),
                legal_basis: "Khoản 5 & Khoản 6 Mục II Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_strict.clone(),
                severity: Warning,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_struct_01_intro_blocks,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "STRUCT-02".into(),
                rule_name: "Hệ thống đề mục Phần/Chương/Mục/Điều & Phụ lục".into(),
                category: HeadingAndStructure,
                description: "Kiểm tra cỡ chữ, độ đậm và tính nhất quán của đề mục và Phụ lục (không đổi thứ tự hay số hiệu điều khoản).".into(),
                legal_basis: "Mục II.6 & Mục IV Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Warning,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_struct_02_headings,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "STRUCT-03".into(),
                rule_name: "Gán Heading Styles & màu tiêu đề chuẩn".into(),
                category: HeadingAndStructure,
                description: "Kiểm tra đồng bộ Heading Style (1–4) cho mục lục tự động khi văn bản có hệ thống đề mục rõ ràng.".into(),
                legal_basis: "Chuẩn hóa điều hướng & Mục lục NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Information,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_struct_03_styles,
        },
        // 4.5 Table Layout
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "TBL-01".into(),
                rule_name: "Bảng biểu không vượt khỏi khổ vùng in".into(),
                category: TableLayout,
                description: "Phát hiện bảng dữ liệu có tổng độ rộng vượt quá chiều rộng vùng in của Section (chỉ co bảng khi được xác nhận).".into(),
                legal_basis: "Mục I.1 & Bố cục trang in NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Error,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_tbl_01_width_overflow,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "TBL-02".into(),
                rule_name: "Lặp dòng tiêu đề & Ngắt trang tự nhiên trong bảng nhiều hàng".into(),
                category: TableLayout,
                description: "Kiểm tra cờ lặp dòng tiêu đề w:tblHeader ở hàng đầu và gỡ khóa w:cantSplit trên hàng dữ liệu quá dài.".into(),
                legal_basis: "Kỹ thuật trình bày bảng biểu nhiều trang Word".into(),
                applicable_document_types: all_core.clone(),
                severity: Warning,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_tbl_02_pagination,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "TBL-03".into(),
                rule_name: "Phông chữ, cỡ chữ (11–13pt) & thụt lề trong ô bảng dữ liệu".into(),
                category: TableLayout,
                description: "Chuẩn hóa chữ Times New Roman trong bảng dữ liệu và bỏ thụt đầu dòng thừa gây chật ô.".into(),
                legal_basis: "Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_and_party.clone(),
                severity: Warning,
                safety_tier: GroupAAuto,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_tbl_03_cell_style,
        },
        // 4.6 Page Numbers & Header/Footer
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PGNUM-01".into(),
                rule_name: "Số trang đặt Header giữa trên (13–14pt), ẩn trang đầu".into(),
                category: PageNumberAndHeaderFooter,
                description: "Số Ả Rập cỡ 13–14pt đứng canh giữa ở lề trên (Header), không hiển thị số ở trang đầu tiên.".into(),
                legal_basis: "Khoản 3 Điều 9 & Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_strict.clone(),
                severity: Warning,
                safety_tier: GroupBConfirm,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: true,
                enabled: true,
            },
            check_fn: check_pgnum_01_layout,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PGNUM-02".into(),
                rule_name: "Bảo toàn Header/Footer & trường động PAGE/NUMPAGES".into(),
                category: PageNumberAndHeaderFooter,
                description: "Kiểm tra Header/Footer hiện có, cảnh báo nếu phát hiện số trang gõ tĩnh thay vì trường PAGE.".into(),
                legal_basis: "Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: all_core.clone(),
                severity: Information,
                safety_tier: GroupCManualOnly,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: false,
                enabled: true,
            },
            check_fn: check_pgnum_02_fields,
        },
        // 4.7 Capitalization & Abbreviation (Phụ lục II & III)
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "CAP-01".into(),
                rule_name: "Viết hoa thành phần thể thức & quyền hạn ký (Phụ lục II)".into(),
                category: CapitalizationAndAbbreviation,
                description: "Kiểm tra viết hoa Quốc hiệu, Tên loại văn bản, quyền hạn ký (TM., KT., TL., TUQ., Q.) và Nơi nhận; không tự sửa tên/chức danh người ký.".into(),
                legal_basis: "Phụ lục II NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_strict.clone(),
                severity: Warning,
                safety_tier: GroupCManualOnly,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: false,
                enabled: true,
            },
            check_fn: check_cap_01_admin_caps,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "CAP-02".into(),
                rule_name: "Nhất quán viết hoa tên cơ quan, địa danh & danh từ riêng".into(),
                category: CapitalizationAndAbbreviation,
                description: "Phát hiện cách viết hoa không đồng nhất (Nhà nước, Chính phủ, Việt Nam, UBND...) để người dùng rà soát thủ công.".into(),
                legal_basis: "Mục II, III, IV Phụ lục II NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_and_party,
                severity: Information,
                safety_tier: GroupCManualOnly,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: false,
                enabled: true,
            },
            check_fn: check_cap_02_consistency,
        },
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "ABBR-01".into(),
                rule_name: "Chữ viết tắt tên loại văn bản hành chính (Phụ lục III)".into(),
                category: CapitalizationAndAbbreviation,
                description: "Đối chiếu ký hiệu văn bản với 27 chữ viết tắt tên loại tại Phụ lục III (Công văn & Thư công không viết tắt tên loại).".into(),
                legal_basis: "Phụ lục III NĐ 30/2020/NĐ-CP".into(),
                applicable_document_types: admin_strict,
                severity: Warning,
                safety_tier: GroupCManualOnly,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: false,
                enabled: true,
            },
            check_fn: check_abbr_01_doc_symbols,
        },
        // Preservation & Integrity
        StdRuleDefinition {
            descriptor: StdRuleDescriptor {
                rule_id: "PRESERVE-01".into(),
                rule_name: "Kiểm kê đối tượng nhúng, liên kết & tính toàn vẹn OpenXML".into(),
                category: PreservationAndIntegrity,
                description: "Kiểm tra rId quan hệ OpenXML và thống kê Ảnh/Logo, Hyperlink, Bookmark, Footnote, Endnote, Comment, Macro VBA.".into(),
                legal_basis: "Tiêu chuẩn bảo toàn dữ liệu ECMA-376 OpenXML".into(),
                applicable_document_types: all_core,
                severity: Critical,
                safety_tier: GroupCManualOnly,
                check_function: String::new(),
                fix_function: None,
                auto_fix_supported: false,
                enabled: true,
            },
            check_fn: check_preserve_01_integrity,
        },
    ];

    for d in &mut defs {
        let (chk, fx): (&str, Option<&str>) = match d.descriptor.rule_id.as_str() {
            "PAGE-01" => ("rules::check_page_01_a4", Some("fixer::fix_section_a4")),
            "PAGE-02" => ("rules::check_page_02_margins", Some("fixer::fix_section_margins")),
            "PAGE-03" => ("rules::check_page_03_sections", None),
            "FONT-01" => ("rules::check_font_01_legacy_encoding", Some("vn_cleaner::convert_legacy_runs_in_element")),
            "FONT-02" => ("rules::check_font_02_family", Some("fixer::set_paragraph_font_preserving_anchors")),
            "FONT-03" => ("rules::check_font_03_component_sizes", Some("fixer::set_paragraph_size_preserving_anchors")),
            "FONT-04" => ("rules::check_font_04_black_color", Some("fixer::set_paragraph_black_color_preserving_hyperlinks")),
            "FONT-05" => ("rules::check_font_05_whitespace_ctrl", Some("vn_cleaner::clean_paragraph_text")),
            "PARA-01" => ("rules::check_para_01_alignment", Some("fixer::apply_paragraph_rule_fix(PARA-01)")),
            "PARA-02" => ("rules::check_para_02_indent", Some("fixer::apply_paragraph_rule_fix(PARA-02)")),
            "PARA-03" => ("rules::check_para_03_spacing", Some("fixer::apply_paragraph_rule_fix(PARA-03)")),
            "PARA-04" => ("rules::check_para_04_empty_and_softbreak", Some("fixer::apply_selected_fixes(PARA-04)")),
            "PARA-05" => ("rules::check_para_05_style_conflict", Some("fixer::strip_direct_run_fonts_when_styled")),
            "STRUCT-01" => ("rules::check_struct_01_intro_blocks", Some("fixer::apply_paragraph_rule_fix(STRUCT-01)")),
            "STRUCT-02" => ("rules::check_struct_02_headings", Some("fixer::apply_paragraph_rule_fix(STRUCT-02)")),
            "STRUCT-03" => ("rules::check_struct_03_styles", Some("fixer::ensure_black_heading_styles_in_pkg")),
            "TBL-01" => ("rules::check_tbl_01_width_overflow", Some("fixer::fix_table_width")),
            "TBL-02" => ("rules::check_tbl_02_pagination", Some("fixer::fix_table_pagination")),
            "TBL-03" => ("rules::check_tbl_03_cell_style", Some("fixer::fix_table_cells_font_and_indent")),
            "PGNUM-01" => ("rules::check_pgnum_01_layout", Some("fixer::apply_nd30_page_number_header")),
            "PGNUM-02" => ("rules::check_pgnum_02_fields", None),
            "CAP-01" => ("rules::check_cap_01_admin_caps", None),
            "CAP-02" => ("rules::check_cap_02_consistency", None),
            "ABBR-01" => ("rules::check_abbr_01_doc_symbols", None),
            "PRESERVE-01" => ("rules::check_preserve_01_integrity", None),
            _ => ("rules::unknown", None),
        };
        d.descriptor.check_function = chk.to_string();
        d.descriptor.fix_function = fx.map(|s| s.to_string());
    }
    defs
}

pub fn is_rule_applicable_to_doc(
    rule: &StdRuleDescriptor,
    ir: &DocumentIr,
    ctx: &RuleEvalContext,
) -> (bool, Option<RuleCheckStatus>) {
    if ctx.disabled_rule_ids.iter().any(|id| id == &rule.rule_id) {
        return (false, Some(RuleCheckStatus::NotApplicable));
    }

    if ir.effective_scope == StdDocScope::UnknownRequiresSelection {
        // Scope-agnostic technical safety rules still run; NĐ30-specific rules mark Indeterminate
        let scope_agnostic = matches!(
            rule.rule_id.as_str(),
            "FONT-01" | "FONT-05" | "PAGE-03" | "PGNUM-02" | "PRESERVE-01"
        );
        if !scope_agnostic {
            return (false, Some(RuleCheckStatus::Indeterminate));
        }
        return (true, None);
    }

    if ir.effective_scope == StdDocScope::EnterpriseInternal {
        let ent_nd30 = ctx
            .org_profile
            .map(|o| o.sector == OrgSector::Enterprise && o.use_nd30_layout_for_enterprise)
            .unwrap_or(false);
        if !ent_nd30
            && matches!(
                rule.rule_id.as_str(),
                "PAGE-02" | "PARA-01" | "PARA-02" | "PGNUM-01" | "CAP-01" | "ABBR-01" | "STRUCT-01"
            )
        {
            return (false, Some(RuleCheckStatus::NotApplicable));
        }
    }

    if !rule.applicable_document_types.contains(&ir.effective_scope) {
        return (false, Some(RuleCheckStatus::NotApplicable));
    }

    (true, None)
}

fn make_finding(
    rule_id: &str,
    rule_name: &str,
    category: RuleCategory,
    severity: StdSeverity,
    status: RuleCheckStatus,
    safety_tier: FixSafetyTier,
    legal_basis: &str,
    auto_fix: bool,
    idx: usize,
    location: impl Into<String>,
    element_path: impl Into<String>,
    current_value: impl Into<String>,
    expected_value: impl Into<String>,
    proposal: impl Into<String>,
) -> StdFinding {
    StdFinding {
        finding_id: format!("{}-{}", rule_id, idx + 1),
        rule_id: rule_id.to_string(),
        rule_name: rule_name.to_string(),
        category,
        severity,
        status,
        safety_tier,
        location: location.into(),
        element_path: element_path.into(),
        current_value: current_value.into(),
        expected_value: expected_value.into(),
        proposal: proposal.into(),
        legal_basis: legal_basis.to_string(),
        auto_fix_supported: auto_fix,
        disposition: Default::default(),
    }
}

// ============================================================================
// INDIVIDUAL RULE CHECK IMPLEMENTATIONS
// ============================================================================

fn check_page_01_a4(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Khoản 1 Điều 9 & Mục I Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for s in &ir.sections {
        let (w, h) = (s.summary.paper_width_mm, s.summary.paper_height_mm);
        let is_a4 = ((w - 210.0).abs() <= 4.0 && (h - 297.0).abs() <= 4.0)
            || ((w - 297.0).abs() <= 4.0 && (h - 210.0).abs() <= 4.0);
        if !is_a4 {
            findings.push(make_finding(
                "PAGE-01",
                "Khổ giấy chuẩn A4 (210 × 297 mm)",
                RuleCategory::PageAndSection,
                StdSeverity::Error,
                RuleCheckStatus::Failed,
                if ir.sections.len() > 1 {
                    FixSafetyTier::GroupBConfirm
                } else {
                    FixSafetyTier::GroupAAuto
                },
                basis,
                true,
                findings.len(),
                format!("Section {}", s.summary.section_index),
                format!("sect[{}]", s.summary.section_index),
                format!("{:.1} × {:.1} mm ({})", w, h, s.summary.orientation),
                if s.summary.orientation == "landscape" {
                    "297.0 × 210.0 mm (A4 ngang)"
                } else {
                    "210.0 × 297.0 mm (A4 dọc)"
                },
                format!(
                    "Đưa khổ giấy Section {} về chuẩn A4 ({}).",
                    s.summary.section_index, s.summary.orientation
                ),
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_page_02_margins(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục I.2 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for s in &ir.sections {
        let sum = &s.summary;
        let is_land = sum.orientation == "landscape";
        let (top_ok, bot_ok, left_ok, right_ok) = if is_land {
            let bind_ok = (29.0..=36.0).contains(&sum.margin_top_mm)
                || (29.0..=36.0).contains(&sum.margin_left_mm);
            (
                bind_ok || (14.5..=26.0).contains(&sum.margin_top_mm),
                (14.5..=26.0).contains(&sum.margin_bottom_mm),
                bind_ok || (14.5..=26.0).contains(&sum.margin_left_mm),
                (14.5..=26.0).contains(&sum.margin_right_mm),
            )
        } else {
            (
                (19.0..=26.0).contains(&sum.margin_top_mm),
                (19.0..=26.0).contains(&sum.margin_bottom_mm),
                (29.0..=36.0).contains(&sum.margin_left_mm),
                (14.5..=21.0).contains(&sum.margin_right_mm),
            )
        };

        if !(top_ok && bot_ok && left_ok && right_ok) {
            findings.push(make_finding(
                "PAGE-02",
                "Định lề trang văn bản hành chính",
                RuleCategory::PageAndSection,
                StdSeverity::Error,
                RuleCheckStatus::Failed,
                if ir.sections.len() > 1 {
                    FixSafetyTier::GroupBConfirm
                } else {
                    FixSafetyTier::GroupAAuto
                },
                basis,
                true,
                findings.len(),
                format!("Section {} ({})", sum.section_index, sum.orientation),
                format!("sect[{}]", sum.section_index),
                format!(
                    "Trên {:.1} mm · Dưới {:.1} mm · Trái {:.1} mm · Phải {:.1} mm",
                    sum.margin_top_mm, sum.margin_bottom_mm, sum.margin_left_mm, sum.margin_right_mm
                ),
                if is_land {
                    "Trên/Trái đóng gáy 30 mm; các mép còn lại 15–25 mm".to_string()
                } else {
                    "Trên/Dưới 20–25 mm · Trái 30–35 mm · Phải 15–20 mm".to_string()
                },
                if is_land {
                    format!(
                        "Điều chỉnh lề Section {} trang ngang (đóng gáy mép trên 30 mm, còn lại 20/15/20 mm).",
                        sum.section_index
                    )
                } else {
                    format!(
                        "Điều chỉnh lề Section {} về 20 mm (trên/dưới), 30 mm (trái), 20 mm (phải).",
                        sum.section_index
                    )
                },
            ));
        }
    }

    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_page_03_sections(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    if ir.sections.len() <= 1 {
        return RuleCheckResult {
            status: RuleCheckStatus::Passed,
            findings: vec![],
        };
    }

    let landscape_cnt = ir
        .sections
        .iter()
        .filter(|s| s.summary.orientation == "landscape")
        .count();
    let portrait_cnt = ir.sections.len() - landscape_cnt;
    let desc = if landscape_cnt > 0 && portrait_cnt > 0 {
        format!(
            "Tài liệu có {} Section ({} Dọc, {} Ngang) — sẽ bảo toàn toàn bộ cấu trúc ngắt section và hướng trang.",
            ir.sections.len(),
            portrait_cnt,
            landscape_cnt
        )
    } else {
        format!(
            "Tài liệu có {} Section — hệ thống bảo toàn từng section break gốc.",
            ir.sections.len()
        )
    };

    let f = make_finding(
        "PAGE-03",
        "Cấu trúc đa Section & Trang ngang",
        RuleCategory::PageAndSection,
        StdSeverity::Information,
        RuleCheckStatus::Passed,
        FixSafetyTier::GroupBConfirm,
        "Mục I.1 Phụ lục I NĐ 30/2020/NĐ-CP",
        false,
        0,
        "Toàn tài liệu",
        "body",
        format!("{} Section", ir.sections.len()),
        "Bảo toàn hướng giấy từng Section".to_string(),
        desc,
    );
    RuleCheckResult {
        status: RuleCheckStatus::Passed,
        findings: vec![f],
    }
}

fn check_font_01_legacy_encoding(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Khoản 2 Điều 9 & Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if p.has_legacy_encoding {
            findings.push(make_finding(
                "FONT-01",
                "Bộ mã tiếng Việt Unicode TCVN 6909:2001",
                RuleCategory::FontAndEncoding,
                StdSeverity::Critical,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!("Font mã cũ: {}", p.font_names.join(", ")),
                "Unicode (Times New Roman)",
                "Chuyển đổi bảng mã TCVN3/VNI sang Unicode chuẩn TCVN 6909:2001.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_font_02_family(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Khoản 2 Điều 9 & Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if p.trimmed.is_empty() {
            continue;
        }
        let non_tnr: Vec<String> = p
            .font_names
            .iter()
            .filter(|f| !is_non_replaceable_font(f) && !f.eq_ignore_ascii_case("Times New Roman"))
            .cloned()
            .collect();

        if p.has_legacy_encoding {
            // FONT-01 already handles legacy TCVN3/VNI encoding explicitly
            continue;
        }

        if !non_tnr.is_empty() || p.has_mixed_fonts || p.has_missing_font_decl {
            let cur = if !non_tnr.is_empty() {
                format!("Đang dùng: {}", non_tnr.join(", "))
            } else if p.has_mixed_fonts {
                format!("Trộn nhiều phông: {}", p.font_names.join(", "))
            } else {
                "Thiếu khai báo phông chữ rõ ràng".to_string()
            };
            findings.push(make_finding(
                "FONT-02",
                "Phông chữ Times New Roman & tính đồng nhất",
                RuleCategory::FontAndEncoding,
                StdSeverity::Error,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                cur,
                "Times New Roman",
                "Đồng bộ phông Times New Roman (giữ nguyên ký tự đặc biệt/hình ảnh).",
            ));
        }
    }

    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_font_03_component_sizes(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục II & Bảng tổng hợp Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if p.trimmed.is_empty() {
            continue;
        }
        let (min_pt, max_pt, target_pt): (f64, f64, f64) = match p.role {
            ParagraphRole::AdminHeaderCell => (12.0, 14.0, 13.0),
            ParagraphRole::DocTypeHeading => (13.0, 14.0, 14.0),
            ParagraphRole::DocSubject => (12.0, 14.0, 14.0),
            ParagraphRole::LegalBasisCanCu => (13.0, 14.0, 14.0),
            ParagraphRole::RecipientKinhGui => (13.0, 14.0, 14.0),
            ParagraphRole::StructuralHeading(_) | ParagraphRole::AppendixHeading => (12.5, 15.0, 14.0),
            ParagraphRole::BodyProse => (13.0, 14.0, 14.0),
            ParagraphRole::RecipientNoiNhan => (11.0, 12.0, 11.0),
            ParagraphRole::SignatureBlock => (12.5, 14.0, 14.0),
            ParagraphRole::DataTableCell | ParagraphRole::EmptyParagraph => continue,
        };

        let bad_sizes: Vec<String> = p
            .sizes_pt
            .iter()
            .filter(|&&sz| sz < min_pt - 0.1 || sz > max_pt + 0.1)
            .map(|sz| format!("{:.1}pt", sz))
            .collect();

        if !bad_sizes.is_empty() {
            findings.push(make_finding(
                "FONT-03",
                "Cỡ chữ theo từng thành phần thể thức",
                RuleCategory::FontAndEncoding,
                StdSeverity::Error,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!("Cỡ chữ: {}", bad_sizes.join(", ")),
                format!("{min_pt:.0}–{max_pt:.0}pt (chuẩn {target_pt:.0}pt)"),
                format!(
                    "Điều chỉnh cỡ chữ thành phần \"{}\" về {:.0}pt.",
                    p.role.label_vi(),
                    target_pt
                ),
            ));
        }
    }

    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_font_04_black_color(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if !p.non_black_colors.is_empty() {
            findings.push(make_finding(
                "FONT-04",
                "Màu chữ đen chuẩn (000000)",
                RuleCategory::FontAndEncoding,
                StdSeverity::Warning,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!("Mã màu: #{}", p.non_black_colors.join(", #")),
                "#000000 (Đen)",
                "Đưa màu chữ về đen chuẩn 000000 (giữ nguyên màu Hyperlink).",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_font_05_whitespace_ctrl(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Kỹ thuật trình bày soạn thảo văn bản NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if p.role == ParagraphRole::AdminHeaderCell {
            continue;
        }
        if p.has_multi_space || p.has_punct_space_error || p.has_control_chars {
            let mut probs = Vec::new();
            if p.has_multi_space {
                probs.push("nhiều khoảng trắng liên tiếp");
            }
            if p.has_punct_space_error {
                probs.push("khoảng trắng trước dấu câu");
            }
            if p.has_control_chars {
                probs.push("ký tự điều khiển ẩn");
            }
            findings.push(make_finding(
                "FONT-05",
                "Khoảng trắng thừa, dấu câu & ký tự điều khiển",
                RuleCategory::FontAndEncoding,
                StdSeverity::Warning,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!(
                    "{}: \"{}\"",
                    probs.join(", "),
                    p.trimmed.chars().take(50).collect::<String>()
                ),
                "Khoảng trắng đơn & chuẩn dấu câu tiếng Việt",
                "Chuẩn hóa khoảng trắng và xóa ký tự điều khiển ẩn.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_para_01_alignment(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    let prose_paras: Vec<&ParagraphInfo> = ir
        .paragraphs
        .iter()
        .filter(|p| p.role == ParagraphRole::BodyProse && !p.trimmed.is_empty())
        .collect();

    let all_center = prose_paras.len() >= 3 && prose_paras.iter().all(|p| p.effective_jc == "center");
    for p in prose_paras {
        if p.effective_jc != "both" {
            let reason = if all_center {
                "Văn bản bị căn giữa toàn bộ"
            } else if p.effective_jc == "center" {
                "Căn giữa (center) không phù hợp lời văn"
            } else {
                "Căn trái/phải thay vì căn đều hai bên"
            };
            findings.push(make_finding(
                "PARA-01",
                "Căn đều hai bên phần lời văn chính",
                RuleCategory::ParagraphAndAlignment,
                StdSeverity::Error,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!("jc={} ({})", p.effective_jc, reason),
                "both (Căn đều hai bên / Justify)",
                "Căn đều hai bên cho đoạn lời văn chính.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_para_02_indent(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if !matches!(
            p.role,
            ParagraphRole::BodyProse | ParagraphRole::LegalBasisCanCu
        ) || p.trimmed.is_empty()
        {
            continue;
        }
        // Allowable NĐ30 first-line indent: 1.0cm (567) to 1.27cm (720) with minor tolerance (510..=780)
        let bad_first = p.indent_first_line < 510 || p.indent_first_line > 780;
        let bad_left_right = p.indent_left > 450 || p.indent_right > 450;
        if bad_first || bad_left_right {
            let cur_cm = p.indent_first_line as f64 / 567.0;
            findings.push(make_finding(
                "PARA-02",
                "Thụt đầu dòng lời văn (1.0–1.27 cm)",
                RuleCategory::ParagraphAndAlignment,
                StdSeverity::Warning,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!(
                    "firstLine={:.2} cm · left={:.2} cm · right={:.2} cm",
                    cur_cm,
                    p.indent_left as f64 / 567.0,
                    p.indent_right as f64 / 567.0
                ),
                "Thụt đầu dòng 1.0–1.27 cm (720 twips); lề đoạn trái/phải = 0",
                "Chuẩn hóa thụt đầu dòng về 1.27 cm và đưa lề đoạn trái/phải về 0.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_para_03_spacing(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if !matches!(
            p.role,
            ParagraphRole::BodyProse | ParagraphRole::LegalBasisCanCu
        ) || p.trimmed.is_empty()
        {
            continue;
        }
        let line_ok = if p.line_rule == "auto" {
            (240..=360).contains(&p.line_spacing)
        } else {
            (260..=440).contains(&p.line_spacing)
        };
        let total_para_gap_pt = (p.spacing_before + p.spacing_after) as f64 / 20.0;
        let gap_ok = total_para_gap_pt >= 5.5 && p.spacing_before <= 360 && p.spacing_after <= 360;

        if !line_ok || !gap_ok {
            findings.push(make_finding(
                "PARA-03",
                "Giãn dòng (Single–1.5 lines) & Khoảng cách đoạn (>= 6pt)",
                RuleCategory::ParagraphAndAlignment,
                StdSeverity::Warning,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!(
                    "Giãn dòng {:.2}x ({}) · Trước {:.1}pt · Sau {:.1}pt",
                    p.line_spacing as f64 / 240.0,
                    p.line_rule,
                    p.spacing_before as f64 / 20.0,
                    p.spacing_after as f64 / 20.0
                ),
                "Giãn dòng 1.0–1.5 lines (khuyến nghị 1.15–1.2x); khoảng cách đoạn >= 6pt",
                "Chuẩn hóa giãn dòng 1.15 lines (276 twips) và khoảng cách sau đoạn 6pt (120 twips).",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_para_04_empty_and_softbreak(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Kỹ thuật trình bày OpenXML NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    let top_level: Vec<&ParagraphInfo> = ir
        .paragraphs
        .iter()
        .filter(|p| p.in_table_idx.is_none())
        .collect();

    let mut consec_empty = 0usize;
    for p in &top_level {
        if p.role == ParagraphRole::EmptyParagraph && p.is_empty_removable {
            consec_empty += 1;
            if consec_empty >= 2 {
                findings.push(make_finding(
                    "PARA-04",
                    "Đoạn trống thừa & ngắt dòng mềm (Shift+Enter)",
                    RuleCategory::ParagraphAndAlignment,
                    StdSeverity::Warning,
                    RuleCheckStatus::Failed,
                    FixSafetyTier::GroupAAuto,
                    basis,
                    true,
                    findings.len(),
                    format!("Đoạn #{} (sau đoạn trống)", p.global_index + 1),
                    p.element_path.clone(),
                    "Đoạn trống liên tiếp (bấm Enter thừa)",
                    "Dùng khoảng cách đoạn (spacing before/after)",
                    "Xóa đoạn trống thừa (giữ nguyên đoạn vùng chữ ký / ngắt section / neo bookmark).",
                ));
            }
        } else {
            consec_empty = 0;
        }

        if p.has_soft_break && matches!(p.role, ParagraphRole::BodyProse | ParagraphRole::LegalBasisCanCu)
        {
            findings.push(make_finding(
                "PARA-04",
                "Đoạn trống thừa & ngắt dòng mềm (Shift+Enter)",
                RuleCategory::ParagraphAndAlignment,
                StdSeverity::Warning,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                "Chứa ngắt dòng mềm <w:br/> làm kéo giãn căn đều hai bên",
                "Đoạn văn tách chuẩn bằng <w:p>",
                "Tách ngắt dòng mềm thành đoạn riêng để tránh giãn chữ rời rạc khi căn đều.",
            ));
        }
    }

    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_para_05_style_conflict(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if p.has_direct_formatting_conflict {
            findings.push(make_finding(
                "PARA-05",
                "Xung đột định dạng trực tiếp với Paragraph Style",
                RuleCategory::ParagraphAndAlignment,
                StdSeverity::Information,
                RuleCheckStatus::NeedsReview,
                FixSafetyTier::GroupBConfirm,
                "Tính nhất quán cấu trúc Style OpenXML",
                true,
                findings.len(),
                format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                p.element_path.clone(),
                format!(
                    "Style \"{}\" bị ghi đè trực tiếp (jc={})",
                    p.style_id.as_deref().unwrap_or("Normal"),
                    p.direct_jc.as_deref().unwrap_or("default")
                ),
                "Đồng bộ theo Style hoặc chuẩn hóa nhất quán",
                "Cho phép gỡ định dạng ghi đè xung đột với Style (Nhóm B - cần xác nhận).",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_struct_01_intro_blocks(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Khoản 5 & Khoản 6 Mục II Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if p.role == ParagraphRole::DocTypeHeading {
            if !p.is_bold_majority || p.effective_jc != "center" {
                findings.push(make_finding(
                    "STRUCT-01",
                    "Trình bày Tên loại, Trích yếu & Căn cứ pháp lý",
                    RuleCategory::HeadingAndStructure,
                    StdSeverity::Warning,
                    RuleCheckStatus::Failed,
                    FixSafetyTier::GroupBConfirm,
                    basis,
                    true,
                    findings.len(),
                    format!("Đoạn #{} (Tên loại VB)", p.global_index + 1),
                    p.element_path.clone(),
                    format!("bold={}, jc={}", p.is_bold_majority, p.effective_jc),
                    "Chữ đứng, IN HOA, đậm, căn giữa",
                    "Đưa tên loại văn bản về chữ đậm, căn giữa.",
                ));
            }
        } else if p.role == ParagraphRole::LegalBasisCanCu && !p.is_italic_majority {
            findings.push(make_finding(
                "STRUCT-01",
                "Trình bày Tên loại, Trích yếu & Căn cứ pháp lý",
                RuleCategory::HeadingAndStructure,
                StdSeverity::Warning,
                RuleCheckStatus::NeedsReview,
                FixSafetyTier::GroupBConfirm,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} (Căn cứ pháp lý)", p.global_index + 1),
                p.element_path.clone(),
                "Chữ đứng (không in nghiêng)",
                "Chữ in nghiêng (13–14pt)",
                "Chuyển dòng \"Căn cứ...\" sang kiểu chữ in nghiêng theo quy chuẩn thể thức.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_struct_02_headings(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục II.6 & Mục IV Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if let ParagraphRole::StructuralHeading(lvl) = p.role {
            let needs_center = lvl == 1 && (collapse_spaces_upper(&p.trimmed).starts_with("PHẦN ") || collapse_spaces_upper(&p.trimmed).starts_with("CHƯƠNG "));
            let jc_bad = needs_center && p.effective_jc != "center";
            if !p.is_bold_majority || jc_bad {
                findings.push(make_finding(
                    "STRUCT-02",
                    "Hệ thống đề mục Phần/Chương/Mục/Điều & Phụ lục",
                    RuleCategory::HeadingAndStructure,
                    StdSeverity::Warning,
                    RuleCheckStatus::Failed,
                    FixSafetyTier::GroupBConfirm,
                    basis,
                    true,
                    findings.len(),
                    format!("Đoạn #{} (Đề mục cấp {})", p.global_index + 1, lvl),
                    p.element_path.clone(),
                    format!(
                        "\"{}\" (bold={}, jc={})",
                        p.trimmed.chars().take(40).collect::<String>(),
                        p.is_bold_majority,
                        p.effective_jc
                    ),
                    if needs_center {
                        "Đậm, căn giữa (không đổi thứ tự/số hiệu điều khoản)"
                    } else {
                        "Đậm, đồng nhất theo cấp đề mục (không đổi số hiệu)"
                    },
                    "Chuẩn hóa kiểu chữ đậm/căn chỉnh cho đề mục (bảo toàn nguyên văn nội dung & số thứ tự).",
                ));
            }
        } else if p.role == ParagraphRole::AppendixHeading && (!p.is_bold_majority || p.effective_jc != "center")
        {
            findings.push(make_finding(
                "STRUCT-02",
                "Hệ thống đề mục Phần/Chương/Mục/Điều & Phụ lục",
                RuleCategory::HeadingAndStructure,
                StdSeverity::Warning,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupBConfirm,
                basis,
                true,
                findings.len(),
                format!("Đoạn #{} (Phụ lục)", p.global_index + 1),
                p.element_path.clone(),
                format!("bold={}, jc={}", p.is_bold_majority, p.effective_jc),
                "Chữ đứng đậm, căn giữa",
                "Đưa tiêu đề Phụ lục về đậm, căn giữa.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_struct_03_styles(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        if let ParagraphRole::StructuralHeading(lvl) = p.role {
            let expected_sid = format!("Heading{}", lvl);
            let missing_heading_style = p
                .style_id
                .as_deref()
                .map(|s| !s.to_ascii_lowercase().starts_with("heading"))
                .unwrap_or(true);
            if missing_heading_style {
                findings.push(make_finding(
                    "STRUCT-03",
                    "Gán Heading Styles & màu tiêu đề chuẩn",
                    RuleCategory::HeadingAndStructure,
                    StdSeverity::Information,
                    RuleCheckStatus::NeedsReview,
                    FixSafetyTier::GroupBConfirm,
                    "Chuẩn hóa điều hướng & Mục lục NĐ 30/2020/NĐ-CP",
                    true,
                    findings.len(),
                    format!("Đoạn #{} ({})", p.global_index + 1, p.role.label_vi()),
                    p.element_path.clone(),
                    format!("Style: {}", p.style_id.as_deref().unwrap_or("Normal")),
                    format!("{} (màu đen 000000)", expected_sid),
                    format!(
                        "Đề xuất gán {} để hỗ trợ Mục lục tự động (chỉ áp dụng khi xác nhận).",
                        expected_sid
                    ),
                ));
            }
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_tbl_01_width_overflow(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục I.1 & Bố cục trang in NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for t in &ir.tables {
        if t.summary.exceeds_printable_width {
            let total_mm = t.summary.total_width_twips as f64 / 56.6929;
            let print_mm = t.summary.printable_width_twips as f64 / 56.6929;
            findings.push(make_finding(
                "TBL-01",
                "Bảng biểu không vượt khỏi khổ vùng in",
                RuleCategory::TableLayout,
                StdSeverity::Error,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupBConfirm,
                basis,
                true,
                findings.len(),
                format!("Bảng #{} (Section {})", t.table_index + 1, t.section_index),
                t.summary.element_path.clone(),
                format!("Chiều rộng {:.1} mm (vượt mép phải)", total_mm),
                format!("Tối đa {:.1} mm (vừa vùng in)", print_mm),
                format!(
                    "Co tỷ lệ độ rộng cột Bảng #{} vừa vùng in {:.1} mm (không tự xoay ngang toàn tài liệu).",
                    t.table_index + 1,
                    print_mm
                ),
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_tbl_02_pagination(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let mut findings = Vec::new();
    for t in &ir.tables {
        if t.admin_kind != crate::engine::formatter::AdminTableKind::None {
            continue;
        }
        if t.summary.row_count >= 3 && (!t.summary.has_repeat_header || !t.locked_long_row_indices.is_empty())
        {
            let mut msg = Vec::new();
            if !t.summary.has_repeat_header {
                msg.push("chưa bật lặp hàng tiêu đề (w:tblHeader)");
            }
            if !t.locked_long_row_indices.is_empty() {
                msg.push("hàng dài bị khóa w:cantSplit có nguy cơ đẩy sang trang mới");
            }
            findings.push(make_finding(
                "TBL-02",
                "Lặp dòng tiêu đề & Ngắt trang tự nhiên trong bảng nhiều hàng",
                RuleCategory::TableLayout,
                StdSeverity::Warning,
                RuleCheckStatus::NeedsReview,
                FixSafetyTier::GroupBConfirm,
                "Kỹ thuật trình bày bảng biểu nhiều trang Word",
                true,
                findings.len(),
                format!("Bảng #{} ({} hàng)", t.table_index + 1, t.summary.row_count),
                t.summary.element_path.clone(),
                msg.join("; "),
                "Bật w:tblHeader hàng đầu, mở ngắt tự nhiên cho hàng dài",
                "Bật lặp tiêu đề hàng 1 và mở w:cantSplit trên các hàng dữ liệu dài (Nhóm B - cần xác nhận).",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_tbl_03_cell_style(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for t in &ir.tables {
        if t.admin_kind != crate::engine::formatter::AdminTableKind::None {
            continue;
        }
        let bad_fonts = t
            .row_fonts
            .iter()
            .any(|f| !is_non_replaceable_font(f) && !f.eq_ignore_ascii_case("Times New Roman"));
        let bad_sizes = t.row_sizes_pt.iter().any(|&s| !(10.5..=13.5).contains(&s));
        if bad_fonts || bad_sizes || t.row_has_bad_indent {
            let mut cur = Vec::new();
            if bad_fonts {
                cur.push(format!("Font: {}", t.row_fonts.join(", ")));
            }
            if bad_sizes {
                let ss: Vec<String> = t.row_sizes_pt.iter().map(|s| format!("{:.1}pt", s)).collect();
                cur.push(format!("Cỡ: {}", ss.join(", ")));
            }
            if t.row_has_bad_indent {
                cur.push("Thụt lề ô > 0".to_string());
            }
            findings.push(make_finding(
                "TBL-03",
                "Phông chữ, cỡ chữ (11–13pt) & thụt lề trong ô bảng dữ liệu",
                RuleCategory::TableLayout,
                StdSeverity::Warning,
                RuleCheckStatus::Failed,
                FixSafetyTier::GroupAAuto,
                basis,
                true,
                findings.len(),
                format!("Bảng #{}", t.table_index + 1),
                t.summary.element_path.clone(),
                cur.join(" · "),
                "Times New Roman, 11–13pt, thụt đầu dòng = 0",
                "Đồng bộ phông/cỡ chữ (12–13pt) và xóa thụt đầu dòng thừa trong ô bảng.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_pgnum_01_layout(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Khoản 3 Điều 9 & Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();

    let footer_has_page = ir
        .headers_footers
        .iter()
        .any(|hf| hf.kind == "footer" && hf.has_page_field);
    if footer_has_page {
        findings.push(make_finding(
            "PGNUM-01",
            "Số trang đặt Header giữa trên (13–14pt), ẩn trang đầu",
            RuleCategory::PageNumberAndHeaderFooter,
            StdSeverity::Warning,
            RuleCheckStatus::Failed,
            FixSafetyTier::GroupBConfirm,
            basis,
            true,
            findings.len(),
            "Footer hiện có",
            "word/footer*.xml",
            "Trường PAGE đang đặt ở Footer (lề dưới)",
            "Đặt số trang canh giữa lề trên (Header), cỡ 13–14pt, ẩn trang 1",
            "Đề xuất chuyển số trang lên Header giữa trên & bật titlePg (chỉ thực hiện khi xác nhận).",
        ));
    }

    for hf in &ir.headers_footers {
        if hf.kind == "header" && hf.has_page_field {
            let bad_sz = hf.font_sizes_pt.iter().any(|&s| !(12.5..=14.5).contains(&s));
            if bad_sz {
                findings.push(make_finding(
                    "PGNUM-01",
                    "Số trang đặt Header giữa trên (13–14pt), ẩn trang đầu",
                    RuleCategory::PageNumberAndHeaderFooter,
                    StdSeverity::Warning,
                    RuleCheckStatus::Failed,
                    FixSafetyTier::GroupBConfirm,
                    basis,
                    true,
                    findings.len(),
                    hf.part_name.clone(),
                    hf.part_name.clone(),
                    format!("Cỡ chữ số trang: {:?}", hf.font_sizes_pt),
                    "13–14pt đứng, canh giữa, bỏ trang 1",
                    "Chuẩn hóa cỡ chữ số trang trong Header về 13pt (Nhóm B - cần xác nhận).",
                ));
            }
        }
    }

    let is_long_doc = ir.paragraphs.len() >= 18 || ir.sections.len() > 1;
    let first_sect_missing_title_pg = ir
        .sections
        .first()
        .map(|s| !s.summary.different_first_page)
        .unwrap_or(false);
    let any_header_page = ir
        .headers_footers
        .iter()
        .any(|hf| hf.kind == "header" && hf.has_page_field);
    if is_long_doc && any_header_page && first_sect_missing_title_pg {
        findings.push(make_finding(
            "PGNUM-01",
            "Số trang đặt Header giữa trên (13–14pt), ẩn trang đầu",
            RuleCategory::PageNumberAndHeaderFooter,
            StdSeverity::Warning,
            RuleCheckStatus::Failed,
            FixSafetyTier::GroupBConfirm,
            basis,
            true,
            findings.len(),
            "Section 1",
            "sect[1]",
            "Hiển thị Header/Số trang ngay ở trang đầu tiên",
            "Bật Different First Page (w:titlePg)",
            "Bật w:titlePg để không hiển thị số trang thứ nhất theo NĐ 30.",
        ));
    }

    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn check_pgnum_02_fields(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for hf in &ir.headers_footers {
        if hf.static_page_digit_suspect {
            findings.push(make_finding(
                "PGNUM-02",
                "Bảo toàn Header/Footer & trường động PAGE/NUMPAGES",
                RuleCategory::PageNumberAndHeaderFooter,
                StdSeverity::Warning,
                RuleCheckStatus::NeedsReview,
                FixSafetyTier::GroupCManualOnly,
                basis,
                false,
                findings.len(),
                hf.part_name.clone(),
                hf.part_name.clone(),
                format!("Văn bản tĩnh nghi là số trang gõ tay: \"{}\"", hf.text_preview),
                "Dùng trường tự động PAGE",
                "Thay số gõ tĩnh bằng Field PAGE tự động (hệ thống không tự xóa nội dung Header/Footer).",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_cap_01_admin_caps(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Phụ lục II NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    for p in &ir.paragraphs {
        let up = collapse_spaces_upper(&p.trimmed);
        if p.role == ParagraphRole::DocTypeHeading && p.trimmed != up {
            findings.push(make_finding(
                "CAP-01",
                "Viết hoa thành phần thể thức & quyền hạn ký (Phụ lục II)",
                RuleCategory::CapitalizationAndAbbreviation,
                StdSeverity::Warning,
                RuleCheckStatus::NeedsReview,
                FixSafetyTier::GroupCManualOnly,
                basis,
                false,
                findings.len(),
                format!("Đoạn #{} (Tên loại VB)", p.global_index + 1),
                p.element_path.clone(),
                p.trimmed.clone(),
                up.clone(),
                "Viết IN HOA tên loại văn bản theo Phụ lục II (chỉ cảnh báo, không tự thay nội dung).",
            ));
        }
        if p.role == ParagraphRole::SignatureBlock {
            for prefix in ["tm.", "kt.", "tl.", "tuq.", "q."] {
                if p.trimmed.to_lowercase().starts_with(prefix) && !p.trimmed.starts_with(&prefix.to_uppercase()) {
                    findings.push(make_finding(
                        "CAP-01",
                        "Viết hoa thành phần thể thức & quyền hạn ký (Phụ lục II)",
                        RuleCategory::CapitalizationAndAbbreviation,
                        StdSeverity::Warning,
                        RuleCheckStatus::NeedsReview,
                        FixSafetyTier::GroupCManualOnly,
                        basis,
                        false,
                        findings.len(),
                        format!("Đoạn #{} (Quyền hạn ký)", p.global_index + 1),
                        p.element_path.clone(),
                        p.trimmed.clone(),
                        format!("{} ... (IN HOA chức vụ, họ tên viết hoa chữ đầu)", prefix.to_uppercase()),
                        "Kiểm tra thủ công khối chữ ký (Nhóm C - tuyệt đối không tự động sửa).",
                    ));
                }
            }
        }
        if p.role == ParagraphRole::RecipientNoiNhan && up.starts_with("NƠI NHẬN") && !p.trimmed.starts_with("Nơi nhận:") {
            findings.push(make_finding(
                "CAP-01",
                "Viết hoa thành phần thể thức & quyền hạn ký (Phụ lục II)",
                RuleCategory::CapitalizationAndAbbreviation,
                StdSeverity::Information,
                RuleCheckStatus::NeedsReview,
                FixSafetyTier::GroupCManualOnly,
                basis,
                false,
                findings.len(),
                format!("Đoạn #{} (Nơi nhận)", p.global_index + 1),
                p.element_path.clone(),
                p.trimmed.clone(),
                "Nơi nhận: (chữ nghiêng đậm 12pt)",
                "Rà soát cách trình bày nhãn \"Nơi nhận:\" theo Mục II.8 Phụ lục I.",
            ));
        }
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_cap_02_consistency(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Mục II, III, IV Phụ lục II NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    let watched = [
        ("việt nam", "Việt Nam"),
        ("nhà nước", "Nhà nước"),
        ("chính phủ", "Chính phủ"),
        ("quốc hội", "Quốc hội"),
    ];

    let mut seen_variants: HashMap<&'static str, Vec<(usize, String, String)>> = HashMap::new();
    for p in &ir.paragraphs {
        if !matches!(p.role, ParagraphRole::BodyProse | ParagraphRole::LegalBasisCanCu) {
            continue;
        }
        let lower = p.trimmed.to_lowercase();
        for &(needle, standard) in &watched {
            if let Some(pos) = lower.find(needle) {
                if let Some(slice) = p.trimmed.get(pos..pos + needle.len()) {
                    if slice != standard && slice != standard.to_uppercase() {
                        seen_variants
                            .entry(standard)
                            .or_default()
                            .push((p.global_index, p.element_path.clone(), slice.to_string()));
                    }
                }
            }
        }
    }

    for (standard, locs) in seen_variants {
        if let Some((idx, path, found_var)) = locs.first() {
            findings.push(make_finding(
                "CAP-02",
                "Nhất quán viết hoa tên cơ quan, địa danh & danh từ riêng",
                RuleCategory::CapitalizationAndAbbreviation,
                StdSeverity::Information,
                RuleCheckStatus::NeedsReview,
                FixSafetyTier::GroupCManualOnly,
                basis,
                false,
                findings.len(),
                format!("Đoạn #{} (+{} vị trí)", idx + 1, locs.len().saturating_sub(1)),
                path.clone(),
                format!("Ghi nhận cụm \"{}\"", found_var),
                format!("Viết hoa chuẩn: \"{}\"", standard),
                format!("Đề xuất rà soát quy tắc viết hoa cho \"{}\" (không tự sửa ngữ nghĩa).", standard),
            ));
        }
    }

    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_abbr_01_doc_symbols(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Phụ lục III NĐ 30/2020/NĐ-CP";
    let mut findings = Vec::new();
    let official_set: HashSet<&str> = PHU_LUC_III_ABBR.iter().map(|&(c, _)| c).collect();

    for p in ir.paragraphs.iter().take(25) {
        if let Some(caps) = re_doc_symbol().captures(&p.trimmed) {
            let abbr = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            if abbr.is_empty() {
                continue;
            }
            if abbr.eq_ignore_ascii_case("CV") {
                findings.push(make_finding(
                    "ABBR-01",
                    "Chữ viết tắt tên loại văn bản hành chính (Phụ lục III)",
                    RuleCategory::CapitalizationAndAbbreviation,
                    StdSeverity::Information,
                    RuleCheckStatus::NeedsReview,
                    FixSafetyTier::GroupCManualOnly,
                    basis,
                    false,
                    findings.len(),
                    format!("Đoạn #{} (Số, ký hiệu)", p.global_index + 1),
                    p.element_path.clone(),
                    format!("Ký hiệu chứa \"{}\" ({})", abbr, p.trimmed),
                    "Theo Phụ lục III NĐ 30, Công văn không dùng chữ viết tắt tên loại",
                    "Lưu ý tham khảo: nếu không bắt buộc theo quy định nội bộ, Công văn bỏ phần chữ viết tắt loại văn bản (không tự sửa Số văn bản).",
                ));
            } else if !official_set.contains(abbr) {
                let exact_case_match = PHU_LUC_III_ABBR
                    .iter()
                    .find(|&&(code, _)| code.eq_ignore_ascii_case(abbr) || strip_đ(code).eq_ignore_ascii_case(abbr));
                let exp = if let Some(&(code, label)) = exact_case_match {
                    format!("Chuẩn Phụ lục III là \"{}\" ({})", code, label)
                } else {
                    "Kiểm tra khớp bảng 27 chữ viết tắt Phụ lục III (hoặc ký hiệu chuyên ngành hợp lệ)".to_string()
                };
                findings.push(make_finding(
                    "ABBR-01",
                    "Chữ viết tắt tên loại văn bản hành chính (Phụ lục III)",
                    RuleCategory::CapitalizationAndAbbreviation,
                    if exact_case_match.is_some() {
                        StdSeverity::Warning
                    } else {
                        StdSeverity::Information
                    },
                    RuleCheckStatus::NeedsReview,
                    FixSafetyTier::GroupCManualOnly,
                    basis,
                    false,
                    findings.len(),
                    format!("Đoạn #{} (Số, ký hiệu)", p.global_index + 1),
                    p.element_path.clone(),
                    p.trimmed.clone(),
                    exp,
                    "Rà soát ký hiệu tên loại văn bản theo Phụ lục III (không tự thay ký hiệu chuyên ngành/số phát hành).",
                ));
            }
        }
    }

    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::NeedsReview
        },
        findings,
    }
}

fn check_preserve_01_integrity(ir: &DocumentIr, _ctx: &RuleEvalContext) -> RuleCheckResult {
    let basis = "Tiêu chuẩn bảo toàn dữ liệu ECMA-376 OpenXML";
    let mut findings = Vec::new();
    if !ir.broken_rids.is_empty() {
        findings.push(make_finding(
            "PRESERVE-01",
            "Kiểm kê đối tượng nhúng, liên kết & tính toàn vẹn OpenXML",
            RuleCategory::PreservationAndIntegrity,
            StdSeverity::Critical,
            RuleCheckStatus::Failed,
            FixSafetyTier::GroupCManualOnly,
            basis,
            false,
            findings.len(),
            "word/_rels/document.xml.rels",
            "rels",
            format!("Thiếu quan hệ OpenXML cho: {}", ir.broken_rids.join(", ")),
            "Mọi r:id/r:embed đều trỏ đúng mục đích trong .rels",
            "Tài liệu gốc có tham chiếu quan hệ bị đứt; kiểm tra lại tệp trước khi sửa sâu.",
        ));
    }
    RuleCheckResult {
        status: if findings.is_empty() {
            RuleCheckStatus::Passed
        } else {
            RuleCheckStatus::Failed
        },
        findings,
    }
}

fn strip_đ(s: &str) -> String {
    s.replace('Đ', "D").replace('đ', "d")
}

fn re_doc_symbol() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^\s*S[Ốố]\s*:\s*[^/\s]*/([A-Za-zÀ-ỹĐđ]+)[\-/]").expect("valid")
    })
}
