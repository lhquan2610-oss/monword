use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetId {
    Nd30,
    VbDang,
    DoanhNghiep,
    LuanVan,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
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
