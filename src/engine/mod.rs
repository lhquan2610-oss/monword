pub mod formatter;
pub mod openxml_arch;
pub mod page_toc;
pub mod security_macro;
pub mod system_office;
pub mod vn_cleaner;
pub mod xml_helpers;

use crate::engine::openxml_arch::OpenXmlPackage;
use crate::models::{AuditReport, FileProcessResult, FileQueueItem, OutputMode, ProcessConfig};
use chrono::Local;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const SUPPORTED_EXTS: &[&str] = &["docx", "docm", "doc", "xlsx", "xlsm"];

pub fn scan_paths(inputs: &[String]) -> Vec<FileQueueItem> {
    let mut items = Vec::new();
    for raw in inputs {
        let path = Path::new(raw);
        collect_supported_files(path, &mut items);
    }
    // Deduplicate by lowercase path string
    let mut seen = std::collections::HashSet::new();
    items.retain(|it| seen.insert(it.path.to_ascii_lowercase()));
    items
}

fn collect_supported_files(path: &Path, out: &mut Vec<FileQueueItem>) {
    if path.is_file() {
        if let Some(item) = to_queue_item(path) {
            out.push(item);
        }
        return;
    }
    if !path.is_dir() {
        return;
    }
    let dir_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(dir_name.as_str(), "backup" | ".chuanhoa_backup" | "_disabled") {
        return;
    }
    let Ok(rd) = fs::read_dir(path) else {
        return;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_dir() {
            collect_supported_files(&p, out);
        } else if let Some(item) = to_queue_item(&p) {
            out.push(item);
        }
    }
}

fn to_queue_item(path: &Path) -> Option<FileQueueItem> {
    let name = path.file_name()?.to_str()?.to_string();
    if name.starts_with("~$") || name.starts_with('.') {
        return None;
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !SUPPORTED_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let size_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Some(FileQueueItem {
        path: path.to_string_lossy().to_string(),
        name,
        ext,
        size_bytes,
    })
}

pub fn audit_single_file(path: &Path) -> Result<AuditReport, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "doc" {
        return Ok(AuditReport {
            file_path: path.to_string_lossy().to_string(),
            file_name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            file_type: "doc".to_string(),
            score: 50,
            section_count: 1,
            table_count: 0,
            paragraph_count: 0,
            has_macros: false,
            is_protected: false,
            has_legacy_encoding: false,
            issues: vec![crate::models::AuditIssue {
                category: "Định dạng cũ (.doc)".to_string(),
                severity: crate::models::AuditSeverity::Warning,
                message: "Tệp Word 97-2003 (.doc) cần chuyển sang .docx qua Word COM trước khi kiểm tra sâu.".to_string(),
                count: 1,
            }],
        });
    }

    let pkg = OpenXmlPackage::open(path)?;
    Ok(vn_cleaner::audit_package(path, &pkg))
}

pub fn create_backup_copy(original_path: &Path) -> Result<PathBuf, String> {
    let parent = original_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let backup_dir = parent.join(".chuanhoa_backup");
    fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Không tạo được thư mục backup {:?}: {}", backup_dir, e))?;

    let fname = original_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let bak_name = format!("{}.{}.bak", fname, stamp);
    let bak_path = backup_dir.join(bak_name);

    fs::copy(original_path, &bak_path)
        .map_err(|e| format!("Lỗi tạo bản sao lưu {:?}: {}", bak_path, e))?;
    Ok(bak_path)
}

pub fn restore_backup(original_path: &Path, backup_path: &Path) -> Result<String, String> {
    if !backup_path.is_file() {
        return Err(format!("Không tìm thấy bản sao lưu {:?}.", backup_path));
    }
    fs::copy(backup_path, original_path)
        .map_err(|e| format!("Không thể khôi phục tệp gốc (có thể đang mở): {}", e))?;
    Ok(format!(
        "Đã khôi phục nguyên trạng tệp {:?} từ bản sao lưu.",
        original_path.file_name().unwrap_or_default()
    ))
}

pub fn resolve_output_path(input_path: &Path, cfg: &ProcessConfig) -> PathBuf {
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("docx")
        .to_ascii_lowercase();

    let target_ext = if cfg.security.clean_macros {
        match ext.as_str() {
            "docm" | "doc" => "docx",
            "xlsm" => "xlsx",
            other => other,
        }
    } else if ext == "doc" {
        "docx"
    } else {
        ext.as_str()
    };

    match cfg.output_mode {
        OutputMode::NewCopy => {
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document");
            let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
            parent.join(format!("{}_chuanhoa.{}", stem, target_ext))
        }
        OutputMode::OverwriteWithBackup => {
            if ext != target_ext {
                input_path.with_extension(target_ext)
            } else {
                input_path.to_path_buf()
            }
        }
    }
}

pub fn process_single_file(input_path: &Path, cfg: &ProcessConfig) -> FileProcessResult {
    let mut logs = Vec::new();
    let mut working_path = input_path.to_path_buf();
    let mut temp_converted_docx: Option<PathBuf> = None;

    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // Handle legacy .doc conversion via Word COM if enabled
    if ext == "doc" {
        if !cfg.convert_legacy_doc {
            return FileProcessResult {
                input_path: input_path.to_string_lossy().to_string(),
                output_path: String::new(),
                backup_path: None,
                success: false,
                stats_summary: String::new(),
                logs,
                error: Some("Tệp .doc cần bật tùy chọn chuyển đổi Word COM (.doc → .docx).".to_string()),
                audit_before: None,
                audit_after: None,
            };
        }
        let temp_docx = env::temp_dir().join(format!(
            "chuanhoa_{}_{}.docx",
            std::process::id(),
            input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("doc")
        ));
        match system_office::word_com_convert_doc_to_docx(input_path, &temp_docx) {
            Ok(()) => {
                logs.push("Đã chuyển đổi tệp .doc cũ sang .docx qua Word COM.".to_string());
                working_path = temp_docx.clone();
                temp_converted_docx = Some(temp_docx);
            }
            Err(e) => {
                return FileProcessResult {
                    input_path: input_path.to_string_lossy().to_string(),
                    output_path: String::new(),
                    backup_path: None,
                    success: false,
                    stats_summary: String::new(),
                    logs,
                    error: Some(e),
                    audit_before: None,
                    audit_after: None,
                };
            }
        }
    }

    let mut pkg = match OpenXmlPackage::open(&working_path) {
        Ok(p) => p,
        Err(e) => {
            if let Some(tmp) = temp_converted_docx {
                let _ = fs::remove_file(tmp);
            }
            return FileProcessResult {
                input_path: input_path.to_string_lossy().to_string(),
                output_path: String::new(),
                backup_path: None,
                success: false,
                stats_summary: String::new(),
                logs,
                error: Some(e),
                audit_before: None,
                audit_after: None,
            };
        }
    };

    let audit_before = Some(vn_cleaner::audit_package(input_path, &pkg));

    let backup_path = if cfg.output_mode == OutputMode::OverwriteWithBackup {
        match create_backup_copy(input_path) {
            Ok(b) => {
                logs.push(format!("Đã tạo bản sao lưu an toàn: {}", b.display()));
                Some(b)
            }
            Err(e) => {
                if let Some(tmp) = temp_converted_docx {
                    let _ = fs::remove_file(tmp);
                }
                return FileProcessResult {
                    input_path: input_path.to_string_lossy().to_string(),
                    output_path: String::new(),
                    backup_path: None,
                    success: false,
                    stats_summary: String::new(),
                    logs,
                    error: Some(e),
                    audit_before,
                    audit_after: None,
                };
            }
        }
    } else {
        None
    };

    // Execute OpenXML pipeline: clean Vietnamese & legacy encoding BEFORE formatting fonts/paragraphs
    let clean_stats = match vn_cleaner::clean_vietnamese_document(&mut pkg, &cfg.cleaner, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            return fail_result(input_path, backup_path, logs, e, audit_before, temp_converted_docx);
        }
    };

    let fmt_stats = match formatter::apply_formatting(&mut pkg, &cfg.format, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            return fail_result(input_path, backup_path, logs, e, audit_before, temp_converted_docx);
        }
    };

    let page_stats = match page_toc::apply_page_and_toc(&mut pkg, &cfg.page_toc, &cfg.format, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            return fail_result(input_path, backup_path, logs, e, audit_before, temp_converted_docx);
        }
    };

    let sec_stats = match security_macro::apply_security_and_macros(&mut pkg, &cfg.security, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            return fail_result(input_path, backup_path, logs, e, audit_before, temp_converted_docx);
        }
    };

    let output_path = resolve_output_path(input_path, cfg);
    if let Err(e) = pkg.save_to(&output_path) {
        return fail_result(input_path, backup_path, logs, e, audit_before, temp_converted_docx);
    }

    // Optional Word COM post-step: update TOC page numbers & export PDF
    if pkg.is_word() && (cfg.also_export_pdf || cfg.page_toc.insert_toc) {
        let pdf_dest = if cfg.also_export_pdf {
            Some(output_path.with_extension("pdf"))
        } else {
            None
        };
        match system_office::word_com_refresh_and_export_pdf(
            &output_path,
            cfg.page_toc.insert_toc,
            pdf_dest.as_deref(),
        ) {
            Ok(()) => {
                if let Some(ref pdf) = pdf_dest {
                    logs.push(format!("Đã xuất kèm PDF chuẩn in ấn: {}", pdf.display()));
                }
            }
            Err(e) => {
                logs.push(format!("Lưu ý COM Worker (file Word đã lưu xong): {}", e));
            }
        }
    }

    if let Some(tmp) = temp_converted_docx {
        let _ = fs::remove_file(tmp);
    }

    let audit_after = Some(vn_cleaner::audit_package(&output_path, &pkg));

    let stats_summary = format!(
        "{} đoạn · {} bảng · {} chuyển mã · {} macro gỡ",
        fmt_stats.paragraphs_formatted,
        fmt_stats.tables_optimized,
        clean_stats.legacy_runs_converted,
        sec_stats.macros_removed
    );
    let _ = page_stats;

    FileProcessResult {
        input_path: input_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        success: true,
        stats_summary,
        logs,
        error: None,
        audit_before,
        audit_after,
    }
}

fn fail_result(
    input_path: &Path,
    backup_path: Option<PathBuf>,
    logs: Vec<String>,
    err: String,
    audit_before: Option<AuditReport>,
    temp_converted_docx: Option<PathBuf>,
) -> FileProcessResult {
    if let Some(tmp) = temp_converted_docx {
        let _ = fs::remove_file(tmp);
    }
    FileProcessResult {
        input_path: input_path.to_string_lossy().to_string(),
        output_path: String::new(),
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        success: false,
        stats_summary: String::new(),
        logs,
        error: Some(err),
        audit_before,
        audit_after: None,
    }
}
