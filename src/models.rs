use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetId {
    Nd30,
    VbDang,
    DoanhNghiep,
    LuanVan,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    #[default]
    NewCopy,
    OverwriteWithBackup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaperSize {
    A4,
    A3,
    Letter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatConfig {
    pub enabled: bool,
    pub font_name: String,
    pub font_size_pt: f64,
    pub line_spacing: f64,
    pub space_before_pt: f64,
    pub space_after_pt: f64,
    pub first_line_indent_cm: f64,
    pub paper_size: PaperSize,
    pub margin_top_cm: f64,
    pub margin_bottom_cm: f64,
    pub margin_left_cm: f64,
    pub margin_right_cm: f64,
    pub protect_admin_blocks: bool,
    pub smart_tables: bool,
    pub table_font_size_pt: f64,
    pub repeat_table_headers: bool,
    pub sync_styles_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuoteStyle {
    CurlyNd30,
    StraightDang,
    Keep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanerConfig {
    pub enabled: bool,
    pub fix_multiple_spaces: bool,
    pub fix_punctuation_spacing: bool,
    pub fix_soft_breaks: bool,
    pub remove_empty_paragraphs: bool,
    pub quote_style: QuoteStyle,
    pub normalize_nd30_dates: bool,
    pub convert_legacy_encoding: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageNumPosition {
    TopCenter,
    TopRight,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrientationMode {
    Keep,
    AllPortrait,
    AllLandscape,
    RotateRanges,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTocConfig {
    pub page_numbers_enabled: bool,
    pub page_num_position: PageNumPosition,
    pub page_num_font_size_pt: f64,
    pub page_num_distance_cm: f64,
    pub skip_first_page: bool,
    pub mirror_odd_even: bool,
    pub start_at: u32,
    pub remove_existing_page_numbers: bool,
    pub orientation_mode: OrientationMode,
    pub landscape_ranges: String,
    pub auto_swap_binding_margin: bool,
    pub reset_portrait_and_remove_sections: bool,
    pub header_text: String,
    pub footer_text: String,
    pub apply_nd30_headings: bool,
    pub auto_detect_headings: bool,
    pub insert_toc: bool,
    pub toc_levels: u8,
    pub toc_page_break_after: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LockMode {
    None,
    ReadOnly,
    CommentsOnly,
    TrackedChanges,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityConfig {
    pub unlock_protection: bool,
    pub lock_mode: LockMode,
    pub lock_password: String,
    pub scrub_personal_info: bool,
    pub author: String,
    pub title: String,
    pub clean_macros: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessConfig {
    pub preset: PresetId,
    pub output_mode: OutputMode,
    pub also_export_pdf: bool,
    pub convert_legacy_doc: bool,
    pub format: FormatConfig,
    pub cleaner: CleanerConfig,
    pub page_toc: PageTocConfig,
    pub security: SecurityConfig,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            font_name: "Times New Roman".to_string(),
            font_size_pt: 14.0,
            line_spacing: 1.15,
            space_before_pt: 0.0,
            space_after_pt: 6.0,
            first_line_indent_cm: 1.27,
            paper_size: PaperSize::A4,
            margin_top_cm: 2.0,
            margin_bottom_cm: 2.0,
            margin_left_cm: 3.0,
            margin_right_cm: 2.0,
            protect_admin_blocks: true,
            smart_tables: true,
            table_font_size_pt: 13.0,
            repeat_table_headers: true,
            sync_styles_default: true,
        }
    }
}

impl Default for CleanerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fix_multiple_spaces: true,
            fix_punctuation_spacing: true,
            fix_soft_breaks: true,
            remove_empty_paragraphs: true,
            quote_style: QuoteStyle::CurlyNd30,
            normalize_nd30_dates: true,
            convert_legacy_encoding: true,
        }
    }
}

impl Default for PageTocConfig {
    fn default() -> Self {
        Self {
            page_numbers_enabled: true,
            page_num_position: PageNumPosition::TopCenter,
            page_num_font_size_pt: 13.0,
            page_num_distance_cm: 1.25,
            skip_first_page: true,
            mirror_odd_even: false,
            start_at: 1,
            remove_existing_page_numbers: false,
            orientation_mode: OrientationMode::Keep,
            landscape_ranges: String::new(),
            auto_swap_binding_margin: true,
            reset_portrait_and_remove_sections: false,
            header_text: String::new(),
            footer_text: String::new(),
            apply_nd30_headings: false,
            auto_detect_headings: false,
            insert_toc: false,
            toc_levels: 3,
            toc_page_break_after: true,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            unlock_protection: false,
            lock_mode: LockMode::None,
            lock_password: String::new(),
            scrub_personal_info: false,
            author: String::new(),
            title: String::new(),
            clean_macros: false,
        }
    }
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self::from_preset(PresetId::Nd30)
    }
}

impl ProcessConfig {
    pub fn from_preset(preset: PresetId) -> Self {
        match preset {
            PresetId::Nd30 => Self {
                preset: PresetId::Nd30,
                output_mode: OutputMode::NewCopy,
                also_export_pdf: false,
                convert_legacy_doc: true,
                format: FormatConfig::default(),
                cleaner: CleanerConfig::default(),
                page_toc: PageTocConfig::default(),
                security: SecurityConfig::default(),
            },
            PresetId::VbDang => Self {
                preset: PresetId::VbDang,
                output_mode: OutputMode::NewCopy,
                also_export_pdf: false,
                convert_legacy_doc: true,
                format: FormatConfig {
                    font_size_pt: 14.0,
                    line_spacing: 1.15,
                    margin_top_cm: 2.0,
                    margin_bottom_cm: 2.0,
                    margin_left_cm: 3.0,
                    margin_right_cm: 2.0,
                    ..FormatConfig::default()
                },
                cleaner: CleanerConfig {
                    quote_style: QuoteStyle::StraightDang,
                    normalize_nd30_dates: false,
                    ..CleanerConfig::default()
                },
                page_toc: PageTocConfig {
                    page_numbers_enabled: true,
                    page_num_position: PageNumPosition::TopCenter,
                    page_num_font_size_pt: 14.0,
                    skip_first_page: true,
                    ..PageTocConfig::default()
                },
                security: SecurityConfig::default(),
            },
            PresetId::DoanhNghiep => Self {
                preset: PresetId::DoanhNghiep,
                output_mode: OutputMode::NewCopy,
                also_export_pdf: false,
                convert_legacy_doc: true,
                format: FormatConfig {
                    font_size_pt: 13.0,
                    line_spacing: 1.15,
                    space_after_pt: 6.0,
                    first_line_indent_cm: 1.0,
                    margin_top_cm: 2.0,
                    margin_bottom_cm: 2.0,
                    margin_left_cm: 2.5,
                    margin_right_cm: 2.0,
                    table_font_size_pt: 12.0,
                    ..FormatConfig::default()
                },
                cleaner: CleanerConfig {
                    quote_style: QuoteStyle::CurlyNd30,
                    normalize_nd30_dates: false,
                    ..CleanerConfig::default()
                },
                page_toc: PageTocConfig {
                    page_numbers_enabled: true,
                    page_num_position: PageNumPosition::BottomRight,
                    page_num_font_size_pt: 12.0,
                    skip_first_page: false,
                    ..PageTocConfig::default()
                },
                security: SecurityConfig::default(),
            },
            PresetId::LuanVan => Self {
                preset: PresetId::LuanVan,
                output_mode: OutputMode::NewCopy,
                also_export_pdf: false,
                convert_legacy_doc: true,
                format: FormatConfig {
                    font_size_pt: 13.0,
                    line_spacing: 1.5,
                    space_after_pt: 6.0,
                    first_line_indent_cm: 1.27,
                    margin_top_cm: 2.5,
                    margin_bottom_cm: 2.5,
                    margin_left_cm: 3.5,
                    margin_right_cm: 2.0,
                    protect_admin_blocks: false,
                    table_font_size_pt: 12.0,
                    ..FormatConfig::default()
                },
                cleaner: CleanerConfig {
                    quote_style: QuoteStyle::CurlyNd30,
                    normalize_nd30_dates: false,
                    ..CleanerConfig::default()
                },
                page_toc: PageTocConfig {
                    page_numbers_enabled: true,
                    page_num_position: PageNumPosition::BottomCenter,
                    page_num_font_size_pt: 13.0,
                    skip_first_page: true,
                    apply_nd30_headings: true,
                    auto_detect_headings: true,
                    insert_toc: true,
                    toc_levels: 3,
                    ..PageTocConfig::default()
                },
                security: SecurityConfig::default(),
            },
            PresetId::Custom => Self::from_preset(PresetId::Nd30),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub id: PresetId,
    pub name: String,
    pub badge: String,
    pub description: String,
    pub config: ProcessConfig,
}

pub fn get_all_presets() -> Vec<PresetInfo> {
    vec![
        PresetInfo {
            id: PresetId::Nd30,
            name: "Nghị định 30/2020/NĐ-CP".to_string(),
            badge: "Chuẩn Nhà nước".to_string(),
            description: "Times New Roman 14pt, lề 2-2-3-2 cm, số trang đầu trang giữa 13pt (bỏ trang 1), ngoặc kép “ ”, chuẩn ngày tháng NĐ30.".to_string(),
            config: ProcessConfig::from_preset(PresetId::Nd30),
        },
        PresetInfo {
            id: PresetId::VbDang,
            name: "Văn bản Đảng (36-HD/VPTW)".to_string(),
            badge: "Đảng & Đoàn thể".to_string(),
            description: "Times New Roman 14pt, lề 2-2-3-2 cm, số trang giữa trên 14pt (bỏ trang 1), giữ ngoặc kép thẳng \" \".".to_string(),
            config: ProcessConfig::from_preset(PresetId::VbDang),
        },
        PresetInfo {
            id: PresetId::DoanhNghiep,
            name: "Báo cáo & Hợp đồng DN".to_string(),
            badge: "Doanh nghiệp".to_string(),
            description: "Times New Roman 13pt, lề 2-2-2.5-2 cm, thụt đầu dòng 1.0 cm, đánh số trang góc dưới phải.".to_string(),
            config: ProcessConfig::from_preset(PresetId::DoanhNghiep),
        },
        PresetInfo {
            id: PresetId::LuanVan,
            name: "Đề án / Luận văn / Nghiên cứu".to_string(),
            badge: "Học thuật".to_string(),
            description: "Times New Roman 13pt, giãn dòng 1.5, lề gáy 3.5 cm, tự nhận diện Heading & chèn Mục lục tự động.".to_string(),
            config: ProcessConfig::from_preset(PresetId::LuanVan),
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditIssue {
    pub category: String,
    pub severity: AuditSeverity,
    pub message: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub file_path: String,
    pub file_name: String,
    pub file_type: String,
    pub score: u8,
    pub section_count: usize,
    pub table_count: usize,
    pub paragraph_count: usize,
    pub has_macros: bool,
    pub is_protected: bool,
    pub has_legacy_encoding: bool,
    pub issues: Vec<AuditIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileQueueItem {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProcessResult {
    pub input_path: String,
    pub output_path: String,
    pub backup_path: Option<String>,
    pub success: bool,
    pub stats_summary: String,
    pub logs: Vec<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub audit_before: Option<AuditReport>,
    pub audit_after: Option<AuditReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub office_version: Option<String>,
    pub word_protected_view: bool,
    pub word_start_screen_disabled: bool,
    pub word_spell_check_disabled: bool,
    pub vba_warnings_level: Option<u32>,
    pub normal_dotm_path: Option<String>,
    pub word_startup_dir: String,
    pub excel_addins_dir: String,
    pub user_fonts_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemTweaksRequest {
    pub disable_protected_view: Option<bool>,
    pub disable_start_screen: Option<bool>,
    pub disable_spell_check: Option<bool>,
    pub enable_vba_macros: Option<bool>,
    pub apply_normal_dotm_nd30: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddinItem {
    pub app: String,
    pub name: String,
    pub file_name: String,
    pub path: String,
    pub enabled: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManageAddinRequest {
    pub action: String, // "toggle" | "delete" | "add"
    pub app: String,    // "word" | "excel"
    pub path: Option<String>,
}

// ============================================================================
// PHASE 1: ORGANIZATION PROFILE MODELS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OrgSector {
    #[default]
    StateAgency,
    PartyOrg,
    Enterprise,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OrganizationProfile {
    pub id: String,
    pub profile_name: String,
    pub sector: OrgSector,
    pub governing_org: String,
    pub issuing_org: String,
    pub affiliated_unit: String,
    pub department_or_branch: String,
    pub agency_abbreviation: String,
    pub default_location: String,
    pub use_nd30_layout_for_enterprise: bool,
    pub show_governing_org_default: bool,
    pub show_affiliated_unit_default: bool,
    pub show_department_default: bool,
    pub updated_at: String,
}

impl Default for OrganizationProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            profile_name: String::new(),
            sector: OrgSector::StateAgency,
            governing_org: String::new(),
            issuing_org: String::new(),
            affiliated_unit: String::new(),
            department_or_branch: String::new(),
            agency_abbreviation: String::new(),
            default_location: String::new(),
            use_nd30_layout_for_enterprise: true,
            show_governing_org_default: false,
            show_affiliated_unit_default: false,
            show_department_default: false,
            updated_at: String::new(),
        }
    }
}

// ============================================================================
// PHASE 2: DOCUMENT HEADER BUILDER & LEGAL RULE MODELS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AdminDocType {
    #[default]
    CongVan,
    QuyetDinh,
    ToTrinh,
    BaoCao,
    ThongBao,
    KeHoach,
    BienBan,
    ChiThi,
    NghiQuyet,
    DeAn,
    QuyChe,
    HopDong,
    VanBanNoiBo,
}

impl AdminDocType {
    pub fn default_label(self) -> &'static str {
        match self {
            Self::CongVan => "CÔNG VĂN",
            Self::QuyetDinh => "QUYẾT ĐỊNH",
            Self::ToTrinh => "TỜ TRÌNH",
            Self::BaoCao => "BÁO CÁO",
            Self::ThongBao => "THÔNG BÁO",
            Self::KeHoach => "KẾ HOẠCH",
            Self::BienBan => "BIÊN BẢN",
            Self::ChiThi => "CHỈ THỊ",
            Self::NghiQuyet => "NGHỊ QUYẾT",
            Self::DeAn => "ĐỀ ÁN",
            Self::QuyChe => "QUY CHẾ",
            Self::HopDong => "HỢP ĐỒNG",
            Self::VanBanNoiBo => "VĂN BẢN NỘI BỘ",
        }
    }

    #[allow(dead_code)]
    pub fn symbol_prefix(self) -> &'static str {
        match self {
            Self::CongVan => "CV",
            Self::QuyetDinh => "QĐ",
            Self::ToTrinh => "TTr",
            Self::BaoCao => "BC",
            Self::ThongBao => "TB",
            Self::KeHoach => "KH",
            Self::BienBan => "BB",
            Self::ChiThi => "CT",
            Self::NghiQuyet => "NQ",
            Self::DeAn => "ĐA",
            Self::QuyChe => "QC",
            Self::HopDong => "HĐ",
            Self::VanBanNoiBo => "VB",
        }
    }

    pub fn uses_left_cell_subject(self) -> bool {
        matches!(self, Self::CongVan)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleObligation {
    Mandatory,
    DocTypeConditional,
    CaseSpecific,
    OrgConfigurable,
    Recommended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderRuleMeta {
    pub rule_id: String,
    pub title: String,
    pub description: String,
    pub obligation: RuleObligation,
    pub scope: String,
    pub legal_basis: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum HeaderNumberMode {
    #[default]
    KeepExisting,
    ManualInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum HeaderDateMode {
    #[default]
    KeepExisting,
    BlankDraft,
    UseToday,
    ManualDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeaderBuilderConfig {
    pub org_profile_id: Option<String>,
    pub doc_type: AdminDocType,
    pub output_mode: OutputMode,
    pub show_governing_org: bool,
    pub show_issuing_org: bool,
    pub show_affiliated_unit: bool,
    pub show_department: bool,
    pub show_org_underline: bool,
    pub show_national_header: bool,
    pub custom_national_header: String,
    pub custom_motto: String,
    pub show_motto_underline: bool,
    pub show_number_symbol: bool,
    pub number_mode: HeaderNumberMode,
    pub doc_number: String,
    pub doc_symbol: String,
    pub allow_override_existing_number: bool,
    pub allow_override_existing_org: bool,
    pub show_location_date: bool,
    pub location_override: String,
    pub date_mode: HeaderDateMode,
    pub manual_day: String,
    pub manual_month: String,
    pub manual_year: String,
    pub allow_override_existing_date: bool,
    pub show_doc_type_and_subject: bool,
    pub custom_doc_type_label: String,
    pub subject_text: String,
    pub summary_line: String,
    pub classification_mark: String,
    pub selected_candidate_index: Option<usize>,
}

impl Default for HeaderBuilderConfig {
    fn default() -> Self {
        Self {
            org_profile_id: None,
            doc_type: AdminDocType::CongVan,
            output_mode: OutputMode::NewCopy,
            show_governing_org: false,
            show_issuing_org: true,
            show_affiliated_unit: false,
            show_department: false,
            show_org_underline: true,
            show_national_header: true,
            custom_national_header: String::new(),
            custom_motto: String::new(),
            show_motto_underline: true,
            show_number_symbol: true,
            number_mode: HeaderNumberMode::KeepExisting,
            doc_number: String::new(),
            doc_symbol: String::new(),
            allow_override_existing_number: false,
            allow_override_existing_org: false,
            show_location_date: true,
            location_override: String::new(),
            date_mode: HeaderDateMode::KeepExisting,
            manual_day: String::new(),
            manual_month: String::new(),
            manual_year: String::new(),
            allow_override_existing_date: false,
            show_doc_type_and_subject: true,
            custom_doc_type_label: String::new(),
            subject_text: String::new(),
            summary_line: String::new(),
            classification_mark: String::new(),
            selected_candidate_index: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderClassification {
    Valid,
    NeedsStandardization,
    MissingComponents,
    DataConflict,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderLayoutKind {
    None,
    TableLayout,
    ParagraphLayout,
    MixedTableAndParagraphs,
    CustomTemplateWithLogo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedHeaderFields {
    pub governing_org: String,
    pub issuing_org: String,
    pub affiliated_unit: String,
    pub department: String,
    pub national_header: String,
    pub motto: String,
    pub has_org_line: bool,
    pub has_motto_line: bool,
    pub number_raw: String,
    pub number_value: String,
    pub symbol_value: String,
    pub location: String,
    pub date_raw: String,
    pub day: String,
    pub month: String,
    pub year: String,
    pub doc_type_label: String,
    pub subject: String,
    pub summary_line: String,
    pub classification_mark: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderCandidateSummary {
    pub candidate_index: usize,
    pub layout_kind: HeaderLayoutKind,
    pub body_start_index: usize,
    pub body_end_index: usize,
    pub has_logo_or_image: bool,
    pub preview_snippet: String,
    pub fields: DetectedHeaderFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderFinding {
    pub rule_id: String,
    pub severity: AuditSeverity,
    pub obligation: RuleObligation,
    pub message: String,
    pub legal_basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderInspectionResult {
    pub file_path: String,
    pub file_name: String,
    pub classification: HeaderClassification,
    pub classification_label: String,
    pub section_count: usize,
    pub has_header_footer: bool,
    pub has_macros: bool,
    pub candidates: Vec<HeaderCandidateSummary>,
    pub active_candidate_index: Option<usize>,
    pub findings: Vec<HeaderFinding>,
    pub applicable_rules: Vec<HeaderRuleMeta>,
    pub can_create_new: bool,
    pub can_standardize: bool,
    pub requires_user_choice: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderActionMode {
    StandardizeExisting,
    CreateNew,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderApplyResult {
    pub input_path: String,
    pub output_path: String,
    pub initial_backup_path: Option<String>,
    pub success: bool,
    pub classification_before: HeaderClassification,
    pub classification_after: HeaderClassification,
    pub logs: Vec<String>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub inspection_after: Option<HeaderInspectionResult>,
}

// ============================================================================
// PHASE 3: DOCUMENT STANDARDIZATION ENGINE MODELS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StdDocScope {
    #[default]
    AutoDetect,
    AdminNd30,
    EnterpriseInternal,
    SpecializedLegal,
    AcademicThesis,
    PartyDocument,
    Freeform,
    UnknownRequiresSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleCategory {
    PageAndSection,
    FontAndEncoding,
    ParagraphAndAlignment,
    HeadingAndStructure,
    TableLayout,
    PageNumberAndHeaderFooter,
    CapitalizationAndAbbreviation,
    PreservationAndIntegrity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StdSeverity {
    Critical,
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleCheckStatus {
    Passed,
    Failed,
    NeedsReview,
    NotApplicable,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FixSafetyTier {
    GroupAAuto,
    GroupBConfirm,
    GroupCManualOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FindingDisposition {
    #[default]
    Pending,
    AutoFixed,
    ApprovedFix,
    Skipped,
    NeedsManualReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdRuleDescriptor {
    pub rule_id: String,
    pub rule_name: String,
    pub category: RuleCategory,
    pub description: String,
    pub legal_basis: String,
    pub applicable_document_types: Vec<StdDocScope>,
    pub severity: StdSeverity,
    pub safety_tier: FixSafetyTier,
    pub check_function: String,
    pub fix_function: Option<String>,
    pub auto_fix_supported: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSummary {
    pub section_index: usize,
    pub paper_width_mm: f64,
    pub paper_height_mm: f64,
    pub orientation: String,
    pub margin_top_mm: f64,
    pub margin_bottom_mm: f64,
    pub margin_left_mm: f64,
    pub margin_right_mm: f64,
    pub different_first_page: bool,
    pub different_odd_even: bool,
    pub break_type: String,
    pub has_header_ref: bool,
    pub has_footer_ref: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphLocationPreview {
    pub element_path: String,
    pub index: usize,
    pub role: String,
    pub section_index: usize,
    pub excerpt: String,
    pub alignment: String,
    pub font_name: String,
    pub font_size_pt: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSummary {
    pub table_index: usize,
    pub element_path: String,
    pub kind: String,
    pub row_count: usize,
    pub col_count: usize,
    pub total_width_twips: i32,
    pub printable_width_twips: i32,
    pub exceeds_printable_width: bool,
    pub has_repeat_header: bool,
    pub has_locked_long_rows: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderFooterSummary {
    pub part_name: String,
    pub kind: String,
    pub has_page_field: bool,
    pub has_numpages_field: bool,
    pub static_page_digit_suspect: bool,
    pub font_names: Vec<String>,
    pub font_sizes_pt: Vec<f64>,
    pub text_preview: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetInventory {
    pub media_count: usize,
    pub media_bytes: u64,
    pub drawing_count: usize,
    pub hyperlink_count: usize,
    pub bookmark_count: usize,
    pub footnote_count: usize,
    pub endnote_count: usize,
    pub comment_count: usize,
    pub field_count: usize,
    pub has_vba_macros: bool,
    pub vba_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEvalSummary {
    pub rule_id: String,
    pub rule_name: String,
    pub category: RuleCategory,
    pub severity: StdSeverity,
    pub status: RuleCheckStatus,
    pub finding_count: usize,
    pub legal_basis: String,
    pub auto_fix_supported: bool,
    pub safety_tier: FixSafetyTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdFinding {
    pub finding_id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub category: RuleCategory,
    pub severity: StdSeverity,
    pub status: RuleCheckStatus,
    pub safety_tier: FixSafetyTier,
    pub location: String,
    pub element_path: String,
    pub current_value: String,
    pub expected_value: String,
    pub proposal: String,
    pub legal_basis: String,
    pub auto_fix_supported: bool,
    pub disposition: FindingDisposition,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdStatusCounts {
    pub passed: usize,
    pub failed: usize,
    pub needs_review: usize,
    pub not_applicable: usize,
    pub indeterminate: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdSeverityCounts {
    pub critical: usize,
    pub error: usize,
    pub warning: usize,
    pub information: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdAnalysisReport {
    pub file_path: String,
    pub file_name: String,
    pub file_ext: String,
    pub requested_scope: StdDocScope,
    pub effective_scope: StdDocScope,
    pub requires_scope_selection: bool,
    pub scope_reason: String,
    pub sections: Vec<SectionSummary>,
    pub tables: Vec<TableSummary>,
    pub headers_footers: Vec<HeaderFooterSummary>,
    pub paragraph_count: usize,
    pub paragraph_previews: Vec<ParagraphLocationPreview>,
    pub assets: AssetInventory,
    pub rule_summaries: Vec<RuleEvalSummary>,
    pub findings: Vec<StdFinding>,
    pub status_counts: StdStatusCounts,
    pub severity_counts: StdSeverityCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StdExecutionRequest {
    pub path: String,
    pub scope: StdDocScope,
    pub org_profile: Option<OrganizationProfile>,
    pub output_mode: OutputMode,
    pub apply_all_group_a: bool,
    pub approved_finding_ids: Vec<String>,
    pub skipped_finding_ids: Vec<String>,
    pub manual_review_finding_ids: Vec<String>,
    pub disabled_rule_ids: Vec<String>,
}

impl Default for StdExecutionRequest {
    fn default() -> Self {
        Self {
            path: String::new(),
            scope: StdDocScope::AutoDetect,
            org_profile: None,
            output_mode: OutputMode::NewCopy,
            apply_all_group_a: true,
            approved_finding_ids: Vec::new(),
            skipped_finding_ids: Vec::new(),
            manual_review_finding_ids: Vec::new(),
            disabled_rule_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixActionRecord {
    pub action_id: String,
    pub rule_id: String,
    pub safety_tier: FixSafetyTier,
    pub location: String,
    pub element_path: String,
    pub before_value: String,
    pub after_value: String,
    pub timestamp: String,
    pub reversible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffDimension {
    TextContent,
    FontFamily,
    FontSize,
    FontStyle,
    Alignment,
    SpacingAndIndent,
    PageMarginsAndSize,
    SectionStructure,
    TableStructure,
    HeaderFooterAndPageNum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub dimension: DiffDimension,
    pub location: String,
    pub before_val: String,
    pub after_val: String,
    pub reason: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreservationCheckSummary {
    pub passed: bool,
    pub text_paragraphs_before: usize,
    pub text_paragraphs_after: usize,
    pub tables_before: usize,
    pub tables_after: usize,
    pub media_before: usize,
    pub media_after: usize,
    pub drawings_before: usize,
    pub drawings_after: usize,
    pub hyperlinks_before: usize,
    pub hyperlinks_after: usize,
    pub bookmarks_before: usize,
    pub bookmarks_after: usize,
    pub footnotes_before: usize,
    pub footnotes_after: usize,
    pub endnotes_before: usize,
    pub endnotes_after: usize,
    pub comments_before: usize,
    pub comments_after: usize,
    pub fields_before: usize,
    pub fields_after: usize,
    pub macros_before: bool,
    pub macros_after: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDiffReport {
    pub entries: Vec<DiffEntry>,
    pub dimension_counts: BTreeMap<String, usize>,
    pub preservation: PreservationCheckSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostSaveVerification {
    pub output_exists: bool,
    pub output_size_bytes: u64,
    pub reopened_from_disk: bool,
    pub xml_valid: bool,
    pub preservation_passed: bool,
    pub remaining_critical_or_error: usize,
    pub summary_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdExecutionResult {
    pub success: bool,
    pub input_path: String,
    pub output_path: String,
    pub backup_path: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub logs: Vec<String>,
    pub actions_applied: Vec<FixActionRecord>,
    pub report_before: Option<StdAnalysisReport>,
    pub report_after: Option<StdAnalysisReport>,
    pub diff: Option<DocumentDiffReport>,
    pub verification: Option<PostSaveVerification>,
}

// ============================================================================
// PHASE 4: TEMPLATE ENGINE MODELS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TemplateGroup {
    #[default]
    Administrative,
    Enterprise,
    Party,
    Academic,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TemplateUsageStatus {
    #[default]
    Active,
    Draft,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TemplateBlockKind {
    RecipientSalutation,
    AuthorityHeading,
    PreambleBasis,
    DecisionKeyword,
    MetadataBlock,
    HeadingSection,
    ArticleClause,
    #[default]
    ParagraphBlock,
    TableBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SignatureLayoutKind {
    #[default]
    SingleRight,
    DualSecretaryChair,
    TripleApproval,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateTableColumn {
    pub header: String,
    pub width_pct: u32,
    pub align: String,
}

impl Default for TemplateTableColumn {
    fn default() -> Self {
        Self {
            header: String::new(),
            width_pct: 25,
            align: "left".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateTableBlueprint {
    pub caption: String,
    pub columns: Vec<TemplateTableColumn>,
    pub sample_rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateSectionBlock {
    pub section_id: String,
    pub title: String,
    pub kind: TemplateBlockKind,
    pub required: bool,
    pub enabled_by_default: bool,
    pub heading_alignment: String,
    pub heading_bold: bool,
    pub heading_italic: bool,
    pub heading_uppercase: bool,
    pub body_paragraphs: Vec<String>,
    pub inline_table: Option<TemplateTableBlueprint>,
}

impl Default for TemplateSectionBlock {
    fn default() -> Self {
        Self {
            section_id: String::new(),
            title: String::new(),
            kind: TemplateBlockKind::ParagraphBlock,
            required: false,
            enabled_by_default: true,
            heading_alignment: "left".to_string(),
            heading_bold: true,
            heading_italic: false,
            heading_uppercase: false,
            body_paragraphs: Vec::new(),
            inline_table: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateHeaderConfig {
    pub enabled: bool,
    pub base_admin_doc_type: AdminDocType,
    pub custom_doc_type_label: String,
    pub symbol_prefix: String,
    pub use_left_cell_subject: bool,
    pub show_governing_org: bool,
    pub show_issuing_org: bool,
    pub show_national_header: bool,
    pub respect_org_enterprise_layout: bool,
    pub show_number_symbol: bool,
    pub show_location_date: bool,
    pub show_doc_type_and_subject: bool,
    pub include_control_box: bool,
}

impl Default for TemplateHeaderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_admin_doc_type: AdminDocType::CongVan,
            custom_doc_type_label: String::new(),
            symbol_prefix: String::new(),
            use_left_cell_subject: false,
            show_governing_org: false,
            show_issuing_org: true,
            show_national_header: true,
            respect_org_enterprise_layout: true,
            show_number_symbol: true,
            show_location_date: true,
            show_doc_type_and_subject: true,
            include_control_box: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateFooterConfig {
    pub show_recipient_list: bool,
    pub recipient_header_label: String,
    pub default_recipients: Vec<String>,
    pub include_page_numbers: bool,
    pub page_number_position: PageNumPosition,
    pub footer_note_template: String,
}

impl Default for TemplateFooterConfig {
    fn default() -> Self {
        Self {
            show_recipient_list: true,
            recipient_header_label: "Nơi nhận:".to_string(),
            default_recipients: vec!["Như trên;".to_string(), "Lưu: VT.".to_string()],
            include_page_numbers: true,
            page_number_position: PageNumPosition::TopCenter,
            footer_note_template: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateSignatureConfig {
    pub layout: SignatureLayoutKind,
    pub authority_mode: String,
    pub authority_prefix_label: String,
    pub signer_title_placeholder: String,
    pub signer_name_placeholder: String,
    pub left_column_title: String,
    pub middle_column_title: String,
    pub blank_lines_for_seal: usize,
}

impl Default for TemplateSignatureConfig {
    fn default() -> Self {
        Self {
            layout: SignatureLayoutKind::SingleRight,
            authority_mode: String::new(),
            authority_prefix_label: String::new(),
            signer_title_placeholder: "{{chuc_vu}}".to_string(),
            signer_name_placeholder: "{{nguoi_ky}}".to_string(),
            left_column_title: String::new(),
            middle_column_title: String::new(),
            blank_lines_for_seal: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateTableConfig {
    pub default_font_name: String,
    pub default_font_size_pt: f64,
    pub header_bold: bool,
    pub repeat_header_row: bool,
    pub prevent_row_split: bool,
    pub border_style: String,
    pub border_size_eighths_pt: u32,
    pub cell_padding_top_bottom_twips: u32,
    pub cell_padding_left_right_twips: u32,
    pub fit_to_printable_width: bool,
}

impl Default for TemplateTableConfig {
    fn default() -> Self {
        Self {
            default_font_name: "Times New Roman".to_string(),
            default_font_size_pt: 13.0,
            header_bold: true,
            repeat_header_row: true,
            prevent_row_split: true,
            border_style: "single".to_string(),
            border_size_eighths_pt: 4,
            cell_padding_top_bottom_twips: 60,
            cell_padding_left_right_twips: 108,
            fit_to_printable_width: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateFormatRules {
    pub paper_size: PaperSize,
    pub margin_top_mm: f64,
    pub margin_bottom_mm: f64,
    pub margin_left_mm: f64,
    pub margin_right_mm: f64,
    pub font_family: String,
    pub body_font_size_pt: f64,
    pub line_spacing_multiple: f64,
    pub space_before_pt: f64,
    pub space_after_pt: f64,
    pub first_line_indent_cm: f64,
    pub body_alignment: String,
    pub enforce_compatibility_check: bool,
}

impl Default for TemplateFormatRules {
    fn default() -> Self {
        Self {
            paper_size: PaperSize::A4,
            margin_top_mm: 20.0,
            margin_bottom_mm: 20.0,
            margin_left_mm: 30.0,
            margin_right_mm: 15.0,
            font_family: "Times New Roman".to_string(),
            body_font_size_pt: 14.0,
            line_spacing_multiple: 1.15,
            space_before_pt: 0.0,
            space_after_pt: 6.0,
            first_line_indent_cm: 1.0,
            body_alignment: "both".to_string(),
            enforce_compatibility_check: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DocumentTemplate {
    pub template_id: String,
    pub name: String,
    pub group: TemplateGroup,
    pub doc_type: String,
    pub scope: StdDocScope,
    pub content_structure: Vec<TemplateSectionBlock>,
    pub header_config: TemplateHeaderConfig,
    pub footer_config: TemplateFooterConfig,
    pub signature_config: TemplateSignatureConfig,
    pub table_config: TemplateTableConfig,
    pub format_rules: TemplateFormatRules,
    pub version: String,
    pub status: TemplateUsageStatus,
    pub description: String,
    pub is_system: bool,
    pub is_default_for_doc_type: bool,
    pub source_docx_asset_id: Option<String>,
    pub detected_placeholders: Vec<String>,
    pub updated_at: String,
}

impl Default for DocumentTemplate {
    fn default() -> Self {
        Self {
            template_id: String::new(),
            name: String::new(),
            group: TemplateGroup::Administrative,
            doc_type: "congVan".to_string(),
            scope: StdDocScope::AdminNd30,
            content_structure: Vec::new(),
            header_config: TemplateHeaderConfig::default(),
            footer_config: TemplateFooterConfig::default(),
            signature_config: TemplateSignatureConfig::default(),
            table_config: TemplateTableConfig::default(),
            format_rules: TemplateFormatRules::default(),
            version: "1.0.0".to_string(),
            status: TemplateUsageStatus::Active,
            description: String::new(),
            is_system: false,
            is_default_for_doc_type: false,
            source_docx_asset_id: None,
            detected_placeholders: Vec::new(),
            updated_at: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DynamicTemplateFields {
    pub ten_co_quan: String,
    pub co_quan_chu_quan: String,
    pub viet_tat_co_quan: String,
    pub so_van_ban: String,
    pub dia_danh: String,
    pub ngay_thang: String,
    pub day: String,
    pub month: String,
    pub year: String,
    pub nguoi_ky: String,
    pub chuc_vu: String,
    pub nguoi_nhan: String,
    pub trich_yeu: String,
    pub extra_fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingFieldMarker {
    pub field_key: String,
    pub label_vi: String,
    pub marker_text: String,
    pub required_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateVersionRecord {
    pub version: String,
    pub saved_at: String,
    pub change_note: String,
    pub has_docx_asset: bool,
    pub docx_asset_id: Option<String>,
    pub snapshot: DocumentTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCatalogResponse {
    pub templates: Vec<DocumentTemplate>,
    pub custom_dropin_dir: String,
    pub defaults: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SaveTemplateRequest {
    pub template: DocumentTemplate,
    pub confirm_system_override: bool,
    pub change_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportDocxTemplateRequest {
    pub source_path: String,
    pub name: String,
    pub group: TemplateGroup,
    pub doc_type: String,
    pub scope: StdDocScope,
}

impl Default for ImportDocxTemplateRequest {
    fn default() -> Self {
        Self {
            source_path: String::new(),
            name: String::new(),
            group: TemplateGroup::Custom,
            doc_type: "customDocx".to_string(),
            scope: StdDocScope::AdminNd30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TemplateTargetMode {
    #[default]
    CreateNewDocx,
    ApplyToExistingDocx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateApplyRequest {
    pub template_id: String,
    pub org_profile_id: Option<String>,
    pub org_profile_override: Option<OrganizationProfile>,
    pub target_mode: TemplateTargetMode,
    pub target_path: Option<String>,
    pub dest_path: Option<String>,
    pub output_mode: OutputMode,
    pub enabled_section_ids: Vec<String>,
    pub dynamic_fields: DynamicTemplateFields,
    pub insert_framing_on_existing: bool,
}

impl Default for TemplateApplyRequest {
    fn default() -> Self {
        Self {
            template_id: String::new(),
            org_profile_id: None,
            org_profile_override: None,
            target_mode: TemplateTargetMode::CreateNewDocx,
            target_path: None,
            dest_path: None,
            output_mode: OutputMode::NewCopy,
            enabled_section_ids: Vec::new(),
            dynamic_fields: DynamicTemplateFields::default(),
            insert_framing_on_existing: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePipelineStep {
    pub step_number: usize,
    pub step_name: String,
    pub detail: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCompatibilityReport {
    pub compatible: bool,
    pub scope: StdDocScope,
    pub passed_rules: usize,
    pub failed_rules: usize,
    pub needs_review_rules: usize,
    pub missing_fields: Vec<MissingFieldMarker>,
    pub preserved_assets: AssetInventory,
    pub warnings: Vec<String>,
    pub std_findings: Vec<StdFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreviewBlock {
    pub role: String,
    pub title: String,
    pub lines: Vec<String>,
    pub table_preview: Option<TemplateTableBlueprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreviewResult {
    pub template_id: String,
    pub template_name: String,
    pub doc_type: String,
    pub resolved_org_name: String,
    pub header_left_lines: Vec<String>,
    pub header_right_lines: Vec<String>,
    pub title_lines: Vec<String>,
    pub body_blocks: Vec<TemplatePreviewBlock>,
    pub recipient_lines: Vec<String>,
    pub signature_columns: Vec<Vec<String>>,
    pub pipeline_steps: Vec<TemplatePipelineStep>,
    pub compatibility: TemplateCompatibilityReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateApplyResult {
    pub success: bool,
    pub output_path: String,
    pub backup_path: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub post_save_verified: bool,
    pub output_size_bytes: u64,
    pub preview: Option<TemplatePreviewResult>,
    pub logs: Vec<String>,
}

// ============================================================================
// PHASE 5: LAYOUT INTELLIGENCE DATA MODELS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecognizedStructureKind {
    DocumentHeader,
    DocTypeTitle,
    DocSubject,
    MainTitle,
    MainBody,
    HeadingLevel1,
    HeadingLevel2,
    HeadingLevel3,
    HeadingLevel4,
    DataTable,
    ImageAsset,
    SignatureBlock,
    RecipientBlock,
    AppendixBlock,
    HeaderFooter,
    TextBox,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementLocator {
    pub part_name: String,
    pub element_path: String,
    pub body_child_index: Option<usize>,
    pub section_index: usize,
    pub estimated_page_start: usize,
    pub estimated_page_end: usize,
    pub column_index: usize,
    pub x_left_mm: f64,
    pub y_top_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapNode {
    pub node_id: String,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub kind: RecognizedStructureKind,
    pub label: String,
    pub excerpt: String,
    pub level: u8,
    pub confidence: f64,
    pub signals: Vec<String>,
    pub locator: ElementLocator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphMapEntry {
    pub para_id: String,
    pub global_index: usize,
    pub kind: RecognizedStructureKind,
    pub role_label: String,
    pub excerpt: String,
    pub style_id: Option<String>,
    pub font_name: String,
    pub font_size_pt: Option<f64>,
    pub alignment: String,
    pub indent_left_mm: f64,
    pub indent_right_mm: f64,
    pub indent_first_line_mm: f64,
    pub space_before_pt: f64,
    pub space_after_pt: f64,
    pub line_spacing_mult: f64,
    pub widow_control: bool,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub in_table_id: Option<String>,
    pub in_text_box_id: Option<String>,
    pub estimated_lines: usize,
    pub gap_from_prev_pt: f64,
    pub locator: ElementLocator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMapEntry {
    pub table_id: String,
    pub table_index: usize,
    pub table_role: String,
    pub row_count: usize,
    pub col_count: usize,
    pub col_widths_mm: Vec<f64>,
    pub total_width_mm: f64,
    pub printable_width_mm: f64,
    pub indent_left_mm: f64,
    pub alignment: String,
    pub has_repeat_header: bool,
    pub has_merged_cells: bool,
    pub locked_long_row_count: usize,
    pub overflows_width: bool,
    pub overflow_mm: f64,
    pub recommended_orientation: String,
    pub locator: ElementLocator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionMapEntry {
    pub section_id: String,
    pub section_index: usize,
    pub break_type: String,
    pub orientation: String,
    pub paper_width_mm: f64,
    pub paper_height_mm: f64,
    pub margin_top_mm: f64,
    pub margin_bottom_mm: f64,
    pub margin_left_mm: f64,
    pub margin_right_mm: f64,
    pub printable_width_mm: f64,
    pub printable_height_mm: f64,
    pub column_count: usize,
    pub column_space_mm: f64,
    pub equal_column_width: bool,
    pub col_widths_mm: Vec<f64>,
    pub header_ids: Vec<String>,
    pub footer_ids: Vec<String>,
    pub page_start: usize,
    pub page_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleMapEntry {
    pub style_id: String,
    pub style_name: String,
    pub style_type: String,
    pub based_on: Option<String>,
    pub is_default: bool,
    pub font_name: Option<String>,
    pub font_size_pt: Option<f64>,
    pub alignment: Option<String>,
    pub outline_level: Option<u8>,
    pub usage_count: usize,
    pub direct_conflict_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderFooterMapEntry {
    pub hf_id: String,
    pub part_name: String,
    pub hf_kind: String,
    pub bind_types: Vec<String>,
    pub linked_sections: Vec<usize>,
    pub paragraph_count: usize,
    pub table_count: usize,
    pub has_page_field: bool,
    pub has_numpages_field: bool,
    pub has_images: bool,
    pub text_preview: String,
    pub locator: ElementLocator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLayoutEntry {
    pub page_id: String,
    pub page_number: usize,
    pub section_index: usize,
    pub orientation: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub printable_width_mm: f64,
    pub printable_height_mm: f64,
    pub used_height_mm: f64,
    pub fill_ratio_pct: f64,
    pub column_count: usize,
    pub element_ids: Vec<String>,
    pub has_overflow: bool,
    pub has_orphan_or_stranded_heading: bool,
    pub has_abnormal_whitespace: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualObjectMapEntry {
    pub object_id: String,
    pub object_kind: String,
    pub anchor_mode: String,
    pub relationship_id: Option<String>,
    pub media_target: Option<String>,
    pub width_mm: f64,
    pub height_mm: f64,
    pub overflows_printable_area: bool,
    pub inner_paragraph_count: usize,
    pub text_excerpt: String,
    pub locator: ElementLocator,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligentDocumentMap {
    pub document_map: Vec<DocumentMapNode>,
    pub paragraph_map: Vec<ParagraphMapEntry>,
    pub table_map: Vec<TableMapEntry>,
    pub section_map: Vec<SectionMapEntry>,
    pub style_map: Vec<StyleMapEntry>,
    pub header_footer_map: Vec<HeaderFooterMapEntry>,
    pub page_layout_map: Vec<PageLayoutEntry>,
    pub visual_objects: Vec<VisualObjectMapEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutWarningKind {
    TableOverflowWidth,
    ImageOverflowPage,
    ParagraphIndentOverflow,
    WidowOrphanRisk,
    StrandedHeadingBottom,
    SignaturePageSplit,
    StackedEmptyParagraphs,
    PrematurePageBreak,
    ExcessiveSpacing,
    InconsistentSectionSpacing,
    MisalignedComponent,
    UnbalancedLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutWarning {
    pub warning_id: String,
    pub kind: LayoutWarningKind,
    pub severity: StdSeverity,
    pub title: String,
    pub detail: String,
    pub target_element_id: String,
    pub locator: ElementLocator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmartProposalKind {
    NormalizeSpacing,
    AlignHeadings,
    UnifyParagraphFormat,
    AdjustTableLayout,
    NormalizeSectionSpacing,
    ResolveOverflow,
    RecommendOrientation,
    RebalanceLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutProposalTier {
    HighConfidenceAuto,
    RequiresConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutProposal {
    pub proposal_id: String,
    pub kind: SmartProposalKind,
    pub tier: LayoutProposalTier,
    pub confidence: f64,
    pub title: String,
    pub reason: String,
    pub suggested_change: String,
    pub target_element_ids: Vec<String>,
    pub locator: ElementLocator,
    pub affects_pagination: bool,
    pub affects_section: bool,
    pub affects_table: bool,
    pub affects_signature: bool,
    pub param_value: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutIntelligenceSummary {
    pub section_count: usize,
    pub estimated_page_count: usize,
    pub paragraph_count: usize,
    pub table_count: usize,
    pub image_count: usize,
    pub text_box_count: usize,
    pub heading_count: usize,
    pub style_count: usize,
    pub header_footer_count: usize,
    pub warning_count: usize,
    pub auto_proposal_count: usize,
    pub confirm_proposal_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutIntelligenceReport {
    pub file_path: String,
    pub file_name: String,
    pub scope: StdDocScope,
    pub summary: LayoutIntelligenceSummary,
    pub maps: IntelligentDocumentMap,
    pub warnings: Vec<LayoutWarning>,
    pub proposals: Vec<LayoutProposal>,
    pub assets: AssetInventory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutExecutionRequest {
    pub path: String,
    pub scope: Option<StdDocScope>,
    pub output_mode: OutputMode,
    pub apply_high_confidence_auto: bool,
    pub accepted_proposal_ids: Vec<String>,
    pub rejected_proposal_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutExecutionResult {
    pub success: bool,
    pub output_path: String,
    pub backup_path: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub applied_proposal_ids: Vec<String>,
    pub skipped_proposal_ids: Vec<String>,
    pub content_preserved_verified: bool,
    pub relationships_preserved_verified: bool,
    pub post_save_report: Option<LayoutIntelligenceReport>,
    pub logs: Vec<String>,
}

// ============================================================================
// PHASE 6: ADVANCED COMPLIANCE & AUDIT MODELS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComplianceObligation {
    Mandatory,
    Conditional,
    Recommended,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComplianceBasisKind {
    LegalRegulation,
    TechnicalRecommendation,
    OrganizationPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleVersionEntry {
    pub rule_id: String,
    pub rule_version: String,
    pub form_component: String,
    pub basis_kind: ComplianceBasisKind,
    pub legal_citation: String,
    pub default_obligations: BTreeMap<String, ComplianceObligation>,
    pub condition_description: String,
    pub doc_type_exceptions: Vec<String>,
    pub remediation_guidance: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceRulePack {
    pub pack_id: String,
    pub pack_name: String,
    pub pack_version: String,
    pub effective_date: String,
    pub legal_reference_summary: String,
    pub rules: Vec<RuleVersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceMatrixRow {
    pub rule_id: String,
    pub rule_name: String,
    pub rule_version: String,
    pub format_group: RuleCategory,
    pub form_component: String,
    pub basis_kind: ComplianceBasisKind,
    pub legal_basis: String,
    pub applicable_doc_types: Vec<String>,
    pub scope: StdDocScope,
    pub org_config_note: String,
    pub obligation: ComplianceObligation,
    pub condition_detail: String,
    pub is_condition_met_on_doc: Option<bool>,
    pub evaluation_status: Option<RuleCheckStatus>,
    pub finding_count: usize,
    pub auto_fix_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceableFinding {
    pub finding_id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub rule_version: String,
    pub pack_version: String,
    pub legal_basis: String,
    pub basis_kind: ComplianceBasisKind,
    pub obligation: ComplianceObligation,
    pub applicable_scope: String,
    pub format_group: RuleCategory,
    pub form_component: String,
    pub severity: StdSeverity,
    pub location: String,
    pub element_path: String,
    pub check_result: RuleCheckStatus,
    pub current_value: String,
    pub expected_value: String,
    pub remediation_method: String,
    pub safety_tier: FixSafetyTier,
    pub auto_fix_supported: bool,
    pub disposition: FindingDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdvancedIssueKind {
    CrossComponentMismatch,
    ConfigurationConflict,
    MissingRequiredContent,
    StructuralAnomaly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedAuditIssue {
    pub issue_id: String,
    pub kind: AdvancedIssueKind,
    pub severity: StdSeverity,
    pub basis_kind: ComplianceBasisKind,
    pub legal_basis: String,
    pub title: String,
    pub explanation: String,
    pub location: String,
    pub element_path: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatedErrorCluster {
    pub cluster_id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub rule_version: String,
    pub occurrence_count: usize,
    pub sample_locations: Vec<String>,
    pub systemic_cause: String,
    pub recommended_batch_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentConsistencyMetrics {
    pub font_uniformity_pct: f64,
    pub size_uniformity_pct: f64,
    pub spacing_uniformity_pct: f64,
    pub heading_hierarchy_valid: bool,
    pub cross_component_consistent: bool,
    pub technical_quality_index: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFileDigest {
    pub file_name: String,
    pub file_ext: String,
    pub file_size_bytes: u64,
    pub doc_sha256: String,
    pub detected_doc_type: String,
    pub requested_scope: StdDocScope,
    pub effective_scope: StdDocScope,
    pub section_count: usize,
    pub estimated_page_count: usize,
    pub paragraph_count: usize,
    pub table_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRuleProfileUsed {
    pub pack_id: String,
    pub pack_version: String,
    pub effective_date: String,
    pub org_profile_id: Option<String>,
    pub org_profile_name: Option<String>,
    pub mandatory_count: usize,
    pub conditional_count: usize,
    pub recommended_count: usize,
    pub not_applicable_count: usize,
    pub disabled_rule_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecheckSummary {
    pub rechecked_at: String,
    pub output_path: String,
    pub output_sha256: String,
    pub before_failed_rules: usize,
    pub after_failed_rules: usize,
    pub before_findings: usize,
    pub after_findings: usize,
    pub fixed_count: usize,
    pub skipped_count: usize,
    pub remaining_mandatory_failures: usize,
    pub remaining_recommended_warnings: usize,
    pub new_anomalies_introduced: usize,
    pub preservation_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceAuditReport {
    pub audit_id: String,
    pub checked_at: String,
    pub legal_disclaimer: String,
    pub file_info: AuditFileDigest,
    pub rule_profile: AuditRuleProfileUsed,
    pub matrix: Vec<ComplianceMatrixRow>,
    pub traceable_findings: Vec<TraceableFinding>,
    pub advanced_issues: Vec<AdvancedAuditIssue>,
    pub repeated_clusters: Vec<RepeatedErrorCluster>,
    pub consistency: DocumentConsistencyMetrics,
    pub recheck: Option<AuditRecheckSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedRuleRecord {
    pub rule_id: String,
    pub rule_version: String,
    pub obligation: ComplianceObligation,
    pub status: RuleCheckStatus,
    pub finding_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactedFindingSummary {
    pub finding_id: String,
    pub rule_id: String,
    pub rule_version: String,
    pub obligation: ComplianceObligation,
    pub severity: StdSeverity,
    pub structural_locator: String,
    pub disposition: FindingDisposition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditHistoryEntry {
    pub audit_id: String,
    pub checked_at: String,
    pub file_name: String,
    pub file_size_bytes: u64,
    pub doc_sha256: String,
    pub doc_version_label: String,
    pub pack_id: String,
    pub pack_version: String,
    pub effective_scope: StdDocScope,
    pub detected_doc_type: String,
    pub technical_quality_index: u8,
    pub applied_rules: Vec<AppliedRuleRecord>,
    pub detected_findings: Vec<RedactedFindingSummary>,
    pub advanced_issue_codes: Vec<String>,
    pub fixed_finding_ids: Vec<String>,
    pub skipped_finding_ids: Vec<String>,
    pub recheck: Option<AuditRecheckSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRunComparison {
    pub older_audit_id: String,
    pub newer_audit_id: String,
    pub older_checked_at: String,
    pub newer_checked_at: String,
    pub same_document_hash: bool,
    pub older_doc_sha256: String,
    pub newer_doc_sha256: String,
    pub pack_version_changed: bool,
    pub older_pack_version: String,
    pub newer_pack_version: String,
    pub quality_index_delta: i16,
    pub resolved_rule_locations: Vec<String>,
    pub persistent_rule_locations: Vec<String>,
    pub newly_detected_rule_locations: Vec<String>,
    pub skipped_count_delta: isize,
    pub rule_version_diffs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ComplianceAuditRequest {
    pub path: String,
    pub scope: StdDocScope,
    pub doc_type_hint: Option<String>,
    pub org_profile: Option<OrganizationProfile>,
    pub disabled_rule_ids: Vec<String>,
    pub record_history: bool,
}

impl Default for ComplianceAuditRequest {
    fn default() -> Self {
        Self {
            path: String::new(),
            scope: StdDocScope::AutoDetect,
            doc_type_hint: None,
            org_profile: None,
            disabled_rule_ids: Vec::new(),
            record_history: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ComplianceAutoFixAndRecheckRequest {
    pub exec_request: StdExecutionRequest,
    pub doc_type_hint: Option<String>,
    pub record_history: bool,
}

impl Default for ComplianceAutoFixAndRecheckRequest {
    fn default() -> Self {
        Self {
            exec_request: StdExecutionRequest::default(),
            doc_type_hint: None,
            record_history: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceAutoFixAndRecheckResult {
    pub std_execution: StdExecutionResult,
    pub audit_before: Option<ComplianceAuditReport>,
    pub audit_after: Option<ComplianceAuditReport>,
    pub history_entry: Option<AuditHistoryEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditExportFormat {
    Docx,
    Pdf,
    Csv,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportResult {
    pub success: bool,
    pub output_path: String,
    pub format: AuditExportFormat,
    pub size_bytes: u64,
    pub error: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
}

// ============================================================================
// PHASE 7: LIVE PREVIEW ENGINE MODELS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PreviewOverrideConfig {
    pub enabled: bool,
    pub font_name: Option<String>,
    pub font_size_pt: Option<f64>,
    pub table_font_size_pt: Option<f64>,
    pub body_alignment: Option<String>,
    pub line_spacing_mult: Option<f64>,
    pub space_before_pt: Option<f64>,
    pub space_after_pt: Option<f64>,
    pub first_line_indent_cm: Option<f64>,
    pub paper_size: Option<PaperSize>,
    pub orientation_override: Option<String>,
    pub margin_top_cm: Option<f64>,
    pub margin_bottom_cm: Option<f64>,
    pub margin_left_cm: Option<f64>,
    pub margin_right_cm: Option<f64>,
    pub auto_swap_binding_margin: bool,
    pub template_id: Option<String>,
    pub org_profile_id: Option<String>,
    pub header_config: Option<HeaderBuilderConfig>,
    pub apply_header_mode: Option<HeaderActionMode>,
    pub apply_vn_cleaner: bool,
    pub apply_std_group_a: bool,
}

impl Default for PreviewOverrideConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            font_name: None,
            font_size_pt: None,
            table_font_size_pt: None,
            body_alignment: None,
            line_spacing_mult: None,
            space_before_pt: None,
            space_after_pt: None,
            first_line_indent_cm: None,
            paper_size: None,
            orientation_override: None,
            margin_top_cm: None,
            margin_bottom_cm: None,
            margin_left_cm: None,
            margin_right_cm: None,
            auto_swap_binding_margin: true,
            template_id: None,
            org_profile_id: None,
            header_config: None,
            apply_header_mode: None,
            apply_vn_cleaner: false,
            apply_std_group_a: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PreviewBuildRequest {
    pub job_id: String,
    pub path: String,
    pub after_file_path: Option<String>,
    pub include_after: bool,
    pub include_audit_sync: bool,
    pub scope: StdDocScope,
    pub process_config: Option<ProcessConfig>,
    pub overrides: PreviewOverrideConfig,
    pub prefer_incremental: bool,
}

impl Default for PreviewBuildRequest {
    fn default() -> Self {
        Self {
            job_id: "preview_default".to_string(),
            path: String::new(),
            after_file_path: None,
            include_after: false,
            include_audit_sync: false,
            scope: StdDocScope::AutoDetect,
            process_config: None,
            overrides: PreviewOverrideConfig::default(),
            prefer_incremental: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSpan {
    pub text: String,
    pub font_family: String,
    pub font_size_pt: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color_hex: String,
    pub highlight_hex: Option<String>,
    pub vert_align: String,
    pub is_dynamic_page_field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImageRef {
    pub rel_id: String,
    pub media_entry: String,
    pub mime_type: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub wrap_mode: String,
    pub alt_text: String,
    pub overflows_printable_area: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewParagraphBlock {
    pub element_path: String,
    pub global_index: usize,
    pub section_index: usize,
    pub role: String,
    pub list_prefix: String,
    pub alignment: String,
    pub indent_left_mm: f64,
    pub indent_right_mm: f64,
    pub first_line_indent_mm: f64,
    pub hanging_indent_mm: f64,
    pub space_before_pt: f64,
    pub space_after_pt: f64,
    pub line_spacing_mult: f64,
    pub exact_line_pt: Option<f64>,
    pub keep_next: bool,
    pub page_break_before: bool,
    pub has_explicit_page_break: bool,
    pub has_last_rendered_break: bool,
    pub plain_text: String,
    pub spans: Vec<PreviewSpan>,
    pub images: Vec<PreviewImageRef>,
    pub estimated_height_mm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBorderEdge {
    pub visible: bool,
    pub style: String,
    pub width_pt: f64,
    pub color_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTableBorders {
    pub top: PreviewBorderEdge,
    pub bottom: PreviewBorderEdge,
    pub left: PreviewBorderEdge,
    pub right: PreviewBorderEdge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTableCell {
    pub col_index: usize,
    pub col_span: usize,
    pub row_span: usize,
    pub merged_continuation: bool,
    pub width_mm: f64,
    pub v_align: String,
    pub shading_hex: Option<String>,
    pub borders: PreviewTableBorders,
    pub paragraphs: Vec<PreviewParagraphBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTableRow {
    pub row_index: usize,
    pub is_header_row: bool,
    pub cant_split: bool,
    pub min_height_mm: f64,
    pub cells: Vec<PreviewTableCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTableBlock {
    pub element_path: String,
    pub table_index: usize,
    pub section_index: usize,
    pub table_role: String,
    pub alignment: String,
    pub indent_left_mm: f64,
    pub total_width_mm: f64,
    pub col_widths_mm: Vec<f64>,
    pub borders: PreviewTableBorders,
    pub overflows_printable_width: bool,
    pub rows: Vec<PreviewTableRow>,
    pub row_slice_range: (usize, usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PreviewBlock {
    Paragraph(PreviewParagraphBlock),
    Table(PreviewTableBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSectionSpec {
    pub section_index: usize,
    pub paper_label: String,
    pub orientation: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub margin_top_mm: f64,
    pub margin_bottom_mm: f64,
    pub margin_left_mm: f64,
    pub margin_right_mm: f64,
    pub header_distance_mm: f64,
    pub footer_distance_mm: f64,
    pub different_first_page: bool,
    pub page_num_start: Option<u32>,
    pub has_dynamic_page_field: bool,
    pub default_header_blocks: Vec<PreviewParagraphBlock>,
    pub first_header_blocks: Vec<PreviewParagraphBlock>,
    pub default_footer_blocks: Vec<PreviewParagraphBlock>,
    pub first_footer_blocks: Vec<PreviewParagraphBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPageModel {
    pub page_number: usize,
    pub display_page_number: usize,
    pub total_pages: usize,
    pub section_index: usize,
    pub is_first_page_of_section: bool,
    pub paper_label: String,
    pub orientation: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub margin_top_mm: f64,
    pub margin_bottom_mm: f64,
    pub margin_left_mm: f64,
    pub margin_right_mm: f64,
    pub header_blocks: Vec<PreviewParagraphBlock>,
    pub footer_blocks: Vec<PreviewParagraphBlock>,
    pub blocks: Vec<PreviewBlock>,
    pub used_height_mm: f64,
    pub fill_ratio_pct: f64,
    pub break_reason: String,
    pub has_overflow_warning: bool,
    pub element_paths_on_page: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewElementDiff {
    pub diff_id: String,
    pub element_path: String,
    pub before_page: Option<usize>,
    pub after_page: Option<usize>,
    pub change_kinds: Vec<String>,
    pub summary_vi: String,
    pub before_summary: String,
    pub after_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintDiagnosticItem {
    pub item_id: String,
    pub kind: String,
    pub severity: StdSeverity,
    pub page_number: usize,
    pub element_path: String,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintCheckReport {
    pub is_print_ready: bool,
    pub paper_summary: String,
    pub orientation_summary: String,
    pub margin_summary: String,
    pub header_footer_summary: String,
    pub page_number_summary: String,
    pub clipped_count: usize,
    pub overflow_table_count: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<PrintDiagnosticItem>,
    pub fidelity_disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditSyncMarker {
    pub marker_id: String,
    pub source: String,
    pub rule_or_issue_id: String,
    pub title: String,
    pub severity: StdSeverity,
    pub obligation: String,
    pub legal_basis: String,
    pub element_path: String,
    pub page_number: usize,
    pub current_value: String,
    pub expected_value: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDocumentRender {
    pub label: String,
    pub doc_sha256: String,
    pub section_count: usize,
    pub total_pages: usize,
    pub paragraph_count: usize,
    pub table_count: usize,
    pub image_count: usize,
    pub sections: Vec<PreviewSectionSpec>,
    pub pages: Vec<PreviewPageModel>,
    pub print_check: PrintCheckReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSessionPayload {
    pub job_id: String,
    pub status: String,
    pub update_mode: String,
    pub elapsed_ms: u64,
    pub fidelity_mode: String,
    pub fidelity_disclaimer: String,
    pub word_installed: bool,
    pub before: Option<PreviewDocumentRender>,
    pub after: Option<PreviewDocumentRender>,
    pub diffs: Vec<PreviewElementDiff>,
    pub audit_markers: Vec<AuditSyncMarker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordComPreviewVerification {
    pub job_id: String,
    pub cancelled: bool,
    pub target_variant: String,
    pub word_page_count: usize,
    pub openxml_estimated_pages: usize,
    pub page_count_delta: isize,
    pub pdf_url_path: String,
    pub pdf_disk_path: String,
    pub temp_docx_path: String,
    pub elapsed_ms: u64,
    pub note: String,
}

// ============================================================================
// PHASE 8: RELIABILITY, BACKUP VERSIONING & STRUCTURED LOGGING MODELS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupKind {
    InitialBaseline,
    PreEditVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupVersionRecord {
    pub backup_id: String,
    pub original_file_name: String,
    pub backup_file_name: String,
    pub backup_path: String,
    pub kind: BackupKind,
    pub version_number: u32,
    pub source_module: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub integrity_ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub timestamp: String,
    pub task: String,
    pub file_name: String,
    pub module: String,
    pub status: String,
    pub elapsed_ms: u64,
    pub error_code: Option<String>,
    pub detail: String,
}
