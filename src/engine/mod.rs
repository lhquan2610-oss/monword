pub mod compliance_audit;
pub mod formatter;
pub mod header_builder;
pub mod layout_intelligence;
pub mod live_preview;
pub mod openxml_arch;
pub mod org_profile;
pub mod page_toc;
pub mod reliability;
pub mod security_macro;
pub mod std_engine;
pub mod system_office;
pub mod template_engine;
pub mod vn_cleaner;
pub mod xml_helpers;

use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::backup_manager;
use crate::engine::reliability::error_code::{classify_error_message, AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::preservation::{PreservationPolicy, PreservationSnapshot};
use crate::engine::reliability::safe_save;
use crate::models::{AuditReport, FileProcessResult, FileQueueItem, OutputMode, ProcessConfig};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const SUPPORTED_EXTS: &[&str] = &["docx", "docm", "doc", "xlsx", "xlsm"];

pub fn scan_paths(inputs: &[String]) -> Vec<FileQueueItem> {
    let mut items = Vec::new();
    for raw in inputs {
        let path = Path::new(raw);
        let dir_to_recover = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or_else(|| Path::new("."))
        };
        let _ = safe_save::recover_interrupted_saves_in_dir(dir_to_recover);
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
    let timer = OpTimer::start("engine", "audit_single_file", Some(path));
    if !path.is_file() {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrFileRead,
            format!("Không tìm thấy tệp '{}'.", path.display()),
        )
        .format_user_message();
        timer.finish_err("ERR_FILE_READ", &msg);
        return Err(msg);
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "doc" {
        let rep = AuditReport {
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
        };
        timer.finish_ok("Kiểm tra sơ bộ tệp .doc");
        return Ok(rep);
    }

    let pkg = match OpenXmlPackage::open(path) {
        Ok(p) => p,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
            let msg = classified.format_user_message();
            timer.finish_err(classified.code.as_str(), &msg);
            return Err(msg);
        }
    };
    let rep = vn_cleaner::audit_package(path, &pkg);
    timer.finish_ok(&format!("Audit OK · {} điểm · {} vấn đề", rep.score, rep.issues.len()));
    Ok(rep)
}

#[allow(dead_code)]
pub fn create_backup_copy(original_path: &Path) -> Result<PathBuf, String> {
    let rec = backup_manager::create_versioned_backup(original_path, "core_engine")?;
    Ok(PathBuf::from(rec.backup_path))
}

pub fn restore_backup(original_path: &Path, backup_path: &Path) -> Result<String, String> {
    backup_manager::restore_backup_checked(original_path, backup_path)
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
    let timer = OpTimer::start("engine", "process_single_file", Some(input_path));
    let mut logs = Vec::new();
    let mut working_path = input_path.to_path_buf();
    let mut temp_converted_docx: Option<PathBuf> = None;

    if !input_path.is_file() {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrFileRead,
            format!("Không tìm thấy tệp '{}'.", input_path.display()),
        )
        .format_user_message();
        timer.finish_err("ERR_FILE_READ", &msg);
        return fail_result_with_code(
            input_path,
            None,
            logs,
            msg,
            "ERR_FILE_READ",
            None,
            None,
        );
    }

    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // Handle legacy .doc conversion via Word COM if enabled
    if ext == "doc" {
        if !cfg.convert_legacy_doc {
            let msg = AppStructuredError::new(
                AppErrorCode::ErrDocConvert,
                "Tệp .doc cần bật tùy chọn chuyển đổi Word COM (.doc → .docx).",
            )
            .format_user_message();
            timer.finish_err("ERR_DOC_CONVERT", &msg);
            return fail_result_with_code(
                input_path,
                None,
                logs,
                msg,
                "ERR_DOC_CONVERT",
                None,
                None,
            );
        }
        let temp_docx = env::temp_dir().join(format!(
            "chuanhoa_{}_{}_{}.docx",
            std::process::id(),
            chrono::Local::now().timestamp_millis(),
            input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("doc")
        ));
        match system_office::word_com_convert_doc_to_docx(input_path, &temp_docx) {
            Ok(()) => {
                logs.push("Đã chuyển đổi tệp .doc cũ sang .docx qua Word COM.".to_string());
                working_path = temp_docx.clone();
                temp_converted_docx = Some(temp_docx);
            }
            Err(e) => {
                let classified = classify_error_message(&e, AppErrorCode::ErrDocConvert);
                let msg = classified.format_user_message();
                let code = classified.code.as_str().to_string();
                timer.finish_err(&code, &msg);
                return fail_result_with_code(
                    input_path,
                    None,
                    logs,
                    msg,
                    &code,
                    None,
                    None,
                );
            }
        }
    }

    let mut pkg = match OpenXmlPackage::open(&working_path) {
        Ok(p) => p,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrOpenXmlCorrupt);
            let msg = classified.format_user_message();
            let code = classified.code.as_str().to_string();
            timer.finish_err(&code, &msg);
            return fail_result_with_code(
                input_path,
                None,
                logs,
                msg,
                &code,
                None,
                temp_converted_docx,
            );
        }
    };

    let snapshot = match PreservationSnapshot::capture(&pkg) {
        Ok(s) => s,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let msg = classified.format_user_message();
            let code = classified.code.as_str().to_string();
            timer.finish_err(&code, &msg);
            return fail_result_with_code(
                input_path,
                None,
                logs,
                msg,
                &code,
                None,
                temp_converted_docx,
            );
        }
    };

    let audit_before = Some(vn_cleaner::audit_package(input_path, &pkg));

    let backup_path = if cfg.output_mode == OutputMode::OverwriteWithBackup {
        match backup_manager::create_versioned_backup(input_path, "one_click") {
            Ok(rec) => {
                logs.push(format!(
                    "Đã tạo bản sao lưu an toàn (v{}, {:?}): {}",
                    rec.version_number, rec.kind, rec.backup_path
                ));
                Some(PathBuf::from(rec.backup_path))
            }
            Err(e) => {
                let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
                let msg = classified.format_user_message();
                let code = classified.code.as_str().to_string();
                timer.finish_err(&code, &msg);
                return fail_result_with_code(
                    input_path,
                    None,
                    logs,
                    msg,
                    &code,
                    audit_before,
                    temp_converted_docx,
                );
            }
        }
    } else {
        None
    };

    // Execute OpenXML pipeline: clean Vietnamese & legacy encoding BEFORE formatting fonts/paragraphs
    let clean_stats = match vn_cleaner::clean_vietnamese_document(&mut pkg, &cfg.cleaner, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_result_with_code(
                input_path,
                backup_path,
                logs,
                msg,
                &code,
                audit_before,
                temp_converted_docx,
            );
        }
    };

    let fmt_stats = match formatter::apply_formatting(&mut pkg, &cfg.format, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_result_with_code(
                input_path,
                backup_path,
                logs,
                msg,
                &code,
                audit_before,
                temp_converted_docx,
            );
        }
    };

    let page_stats = match page_toc::apply_page_and_toc(&mut pkg, &cfg.page_toc, &cfg.format, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_result_with_code(
                input_path,
                backup_path,
                logs,
                msg,
                &code,
                audit_before,
                temp_converted_docx,
            );
        }
    };

    let sec_stats = match security_macro::apply_security_and_macros(&mut pkg, &cfg.security, &mut logs) {
        Ok(s) => s,
        Err(e) => {
            let classified = classify_error_message(&e, AppErrorCode::ErrXmlMalformed);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_result_with_code(
                input_path,
                backup_path,
                logs,
                msg,
                &code,
                audit_before,
                temp_converted_docx,
            );
        }
    };

    let policy = PreservationPolicy {
        allow_macro_strip: cfg.security.clean_macros,
        allow_section_reset: cfg.page_toc.reset_portrait_and_remove_sections,
        allow_hf_strip: cfg.page_toc.remove_existing_page_numbers,
    };

    let output_path = resolve_output_path(input_path, cfg);
    if let Err(e) = safe_save::save_openxml_verified(&pkg, &output_path, Some(&snapshot), &policy) {
        let classified = classify_error_message(&e, AppErrorCode::ErrFileWrite);
        let code = classified.code.as_str().to_string();
        let msg = classified.format_user_message();
        timer.finish_err(&code, &msg);
        return fail_result_with_code(
            input_path,
            backup_path,
            logs,
            msg,
            &code,
            audit_before,
            temp_converted_docx,
        );
    }

    // Re-read saved output from disk to guarantee actual disk readability
    let disk_pkg = match OpenXmlPackage::open(&output_path) {
        Ok(p) => p,
        Err(e) => {
            if let Some(ref bak) = backup_path {
                if cfg.output_mode == OutputMode::OverwriteWithBackup && output_path == input_path {
                    let _ = backup_manager::restore_backup_checked(input_path, bak);
                }
            }
            let classified = classify_error_message(&e, AppErrorCode::ErrPostSaveVerify);
            let code = classified.code.as_str().to_string();
            let msg = classified.format_user_message();
            timer.finish_err(&code, &msg);
            return fail_result_with_code(
                input_path,
                backup_path,
                logs,
                msg,
                &code,
                audit_before,
                temp_converted_docx,
            );
        }
    };

    // Optional Word COM post-step: update TOC page numbers & export PDF
    // Note: When `cfg.also_export_pdf` is true, PDF export is a critical requested deliverable:
    // if it fails, NEVER return `success: true`.
    if disk_pkg.is_word() && (cfg.also_export_pdf || cfg.page_toc.insert_toc) {
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
                if cfg.also_export_pdf {
                    let classified = classify_error_message(&e, AppErrorCode::ErrDocConvert);
                    let code = classified.code.as_str().to_string();
                    let msg = AppStructuredError::new(
                        classified.code,
                        format!(
                            "Đã chuẩn hóa tệp Word nhưng bước xuất kèm PDF thất bại: {}",
                            classified.cause
                        ),
                    )
                    .format_user_message();
                    logs.push(msg.clone());
                    timer.finish_err(&code, &msg);
                    if let Some(tmp) = temp_converted_docx {
                        let _ = fs::remove_file(tmp);
                    }
                    return FileProcessResult {
                        input_path: input_path.to_string_lossy().to_string(),
                        output_path: output_path.to_string_lossy().to_string(),
                        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
                        success: false,
                        stats_summary: String::new(),
                        logs,
                        error: Some(msg),
                        error_code: Some(code),
                        audit_before,
                        audit_after: Some(vn_cleaner::audit_package(&output_path, &disk_pkg)),
                    };
                }
                logs.push(format!("Lưu ý COM Worker (tệp Word đã lưu xong): {}", e));
            }
        }
    }

    if let Some(tmp) = temp_converted_docx {
        let _ = fs::remove_file(tmp);
    }

    let audit_after = Some(vn_cleaner::audit_package(&output_path, &disk_pkg));

    let stats_summary = format!(
        "{} đoạn · {} bảng · {} chuyển mã · {} macro gỡ",
        fmt_stats.paragraphs_formatted,
        fmt_stats.tables_optimized,
        clean_stats.legacy_runs_converted,
        sec_stats.macros_removed
    );
    let _ = page_stats;

    timer.finish_ok(&format!(
        "Xử lý thành công ({} đoạn, {} bảng)",
        fmt_stats.paragraphs_formatted, fmt_stats.tables_optimized
    ));

    FileProcessResult {
        input_path: input_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        success: true,
        stats_summary,
        logs,
        error: None,
        error_code: None,
        audit_before,
        audit_after,
    }
}

fn fail_result_with_code(
    input_path: &Path,
    backup_path: Option<PathBuf>,
    logs: Vec<String>,
    err: String,
    error_code: &str,
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
        error_code: Some(error_code.to_string()),
        audit_before,
        audit_after: None,
    }
}
