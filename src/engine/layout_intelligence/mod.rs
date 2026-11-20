pub mod analyzer;
pub mod mapper;
pub mod smart_formatter;

use crate::engine::openxml_arch::{OpenXmlPackage, SelectivePackageView};
use crate::engine::std_engine::ir::DocumentIr;
use crate::engine::system_office;
use crate::models::{
    IntelligentDocumentMap, LayoutExecutionRequest, LayoutExecutionResult,
    LayoutIntelligenceReport, LayoutIntelligenceSummary, LayoutProposalTier, OrganizationProfile,
    RecognizedStructureKind, StdDocScope,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct LayoutIrBundle {
    pub ir: DocumentIr,
}

impl LayoutIrBundle {
    pub fn from_view(
        display_path: &Path,
        view: &SelectivePackageView,
        scope: StdDocScope,
        org_profile: Option<&OrganizationProfile>,
    ) -> Result<Self, String> {
        let ir = DocumentIr::build(display_path, view, scope, org_profile)?;
        Ok(Self { ir })
    }
}

/// Shared mapping entrypoint reusable by Phase 3, Phase 4, Phase 5, and future Phase 7.
pub fn build_map_from_ir(
    view: &SelectivePackageView,
    ir: &DocumentIr,
) -> Result<IntelligentDocumentMap, String> {
    mapper::build_intelligent_map(view, ir)
}

/// Performs full Layout Intelligence analysis using an already-parsed `SelectivePackageView` + `DocumentIr`.
pub fn analyze_view_and_ir(
    view: &SelectivePackageView,
    ir: &DocumentIr,
) -> Result<LayoutIntelligenceReport, String> {
    let mut maps = build_map_from_ir(view, ir)?;
    let (warnings, proposals) = analyzer::analyze_layout(&mut maps);

    let image_count = maps
        .visual_objects
        .iter()
        .filter(|v| v.object_kind == "image")
        .count();
    let text_box_count = maps
        .visual_objects
        .iter()
        .filter(|v| v.object_kind == "textBox")
        .count();
    let heading_count = maps
        .document_map
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                RecognizedStructureKind::HeadingLevel1
                    | RecognizedStructureKind::HeadingLevel2
                    | RecognizedStructureKind::HeadingLevel3
                    | RecognizedStructureKind::HeadingLevel4
            )
        })
        .count();
    let auto_proposal_count = proposals
        .iter()
        .filter(|p| p.tier == LayoutProposalTier::HighConfidenceAuto)
        .count();
    let confirm_proposal_count = proposals.len().saturating_sub(auto_proposal_count);

    let summary = LayoutIntelligenceSummary {
        section_count: maps.section_map.len(),
        estimated_page_count: maps.page_layout_map.len().max(1),
        paragraph_count: maps.paragraph_map.len(),
        table_count: maps.table_map.len(),
        image_count,
        text_box_count,
        heading_count,
        style_count: maps.style_map.len(),
        header_footer_count: maps.header_footer_map.len(),
        warning_count: warnings.len(),
        auto_proposal_count,
        confirm_proposal_count,
    };

    Ok(LayoutIntelligenceReport {
        file_path: ir.file_path.clone(),
        file_name: ir.file_name.clone(),
        scope: ir.effective_scope,
        summary,
        maps,
        warnings,
        proposals,
        assets: ir.assets.clone(),
    })
}

pub fn analyze_file(
    path: &Path,
    scope: StdDocScope,
    org_profile: Option<&OrganizationProfile>,
) -> Result<LayoutIntelligenceReport, String> {
    if !path.is_file() {
        return Err(format!("Không tìm thấy tệp văn bản: {}", path.display()));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut temp_docx: Option<PathBuf> = None;
    let working_path = if ext == "doc" {
        let tmp = env::temp_dir().join(format!(
            "monword_layout_ana_{}_{}.docx",
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
            "Định dạng .{} chưa hỗ trợ phân tích bố cục (.docx, .docm, .doc).",
            ext
        ));
    };

    let view_res = OpenXmlPackage::open_selective_for_analysis(&working_path);
    if let Some(t) = temp_docx {
        let _ = fs::remove_file(t);
    }
    let view = view_res?;

    let mut bundle = LayoutIrBundle::from_view(path, &view, scope, org_profile)?;
    bundle.ir.file_ext = ext;
    analyze_view_and_ir(&view, &bundle.ir)
}

pub fn execute_proposals(
    req: &LayoutExecutionRequest,
    org_profile: Option<&OrganizationProfile>,
) -> LayoutExecutionResult {
    smart_formatter::execute_layout_proposals(req, org_profile)
}
