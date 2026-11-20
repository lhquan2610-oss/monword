use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::error_code::{
    format_io_error, is_locked_or_conflict_io, AppErrorCode, AppStructuredError,
};
use crate::engine::reliability::logger::{record_op, OpTimer};
use crate::engine::reliability::preservation::{
    validate_all_package_xml_entries, verify_openxml_preservation, PreservationPolicy,
    PreservationSnapshot,
};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeSaveJournal {
    pub target_path: String,
    pub staged_prev_path: String,
    pub tmp_new_path: String,
    pub original_size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FaultMode {
    None,
    FailTempValidation(String),
    FailRenameNewToTarget,
    AbortProcessMidSwap,
}

thread_local! {
    static TEST_FAULT_MODE: RefCell<FaultMode> = const { RefCell::new(FaultMode::None) };
}

fn current_fault_mode() -> FaultMode {
    TEST_FAULT_MODE.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
pub fn with_fault_mode_for_test<F, R>(mode: FaultMode, f: F) -> R
where
    F: FnOnce() -> R,
{
    TEST_FAULT_MODE.with(|cell| {
        *cell.borrow_mut() = mode;
    });
    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            TEST_FAULT_MODE.with(|cell| {
                *cell.borrow_mut() = FaultMode::None;
            });
        }
    }
    let _guard = ResetGuard;
    f()
}

pub fn save_bytes_atomic(
    dest_path: &Path,
    bytes: &[u8],
    use_backup_journal_dir: bool,
) -> Result<u64, String> {
    atomic_write_with_validation(
        dest_path,
        |file| {
            file.write_all(bytes)
                .map_err(|e| format_io_error("Không ghi được dữ liệu tệp tạm", &e, true))
        },
        |tmp_path| {
            let meta = fs::metadata(tmp_path)
                .map_err(|e| format_io_error("Không kiểm tra được tệp tạm", &e, false))?;
            if meta.len() != bytes.len() as u64 {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrPostSaveVerify,
                    format!(
                        "Kích thước tệp tạm ({}) không khớp dữ liệu ghi ({}).",
                        meta.len(),
                        bytes.len()
                    ),
                )
                .format_user_message());
            }
            Ok(())
        },
        use_backup_journal_dir,
    )
}

pub fn save_json_atomic<T: Serialize>(dest_path: &Path, value: &T) -> Result<u64, String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| {
        AppStructuredError::new(
            AppErrorCode::ErrFileWrite,
            format!("Lỗi tuần tự hóa JSON '{}': {}", dest_path.display(), e),
        )
        .format_user_message()
    })?;
    save_bytes_atomic(dest_path, json.as_bytes(), false)
}

pub fn save_zip_archive_atomic<F>(
    dest_path: &Path,
    write_zip: F,
    expected_entries: usize,
) -> Result<u64, String>
where
    F: FnOnce(&mut File) -> Result<(), String>,
{
    atomic_write_with_validation(
        dest_path,
        write_zip,
        |tmp_path| {
            let file = File::open(tmp_path)
                .map_err(|e| format_io_error("Không mở lại được tệp ZIP tạm", &e, false))?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrOpenXmlCorrupt,
                    format!("Tệp tạm không đạt kiểm tra cấu trúc ZIP: {}", e),
                )
                .format_user_message()
            })?;
            let mut actual_files = 0usize;
            for i in 0..archive.len() {
                let mut item = archive.by_index(i).map_err(|e| {
                    AppStructuredError::new(
                        AppErrorCode::ErrOpenXmlCorrupt,
                        format!("Tệp tạm lỗi entry ZIP #{}: {}", i, e),
                    )
                    .format_user_message()
                })?;
                if !item.is_dir() {
                    actual_files += 1;
                    std::io::copy(&mut item, &mut std::io::sink()).map_err(|e| {
                        AppStructuredError::new(
                            AppErrorCode::ErrOpenXmlCorrupt,
                            format!("Tệp tạm lỗi CRC entry '{}': {}", item.name(), e),
                        )
                        .format_user_message()
                    })?;
                }
            }
            if actual_files != expected_entries && archive.len() < expected_entries {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrPostSaveVerify,
                    "Số lượng entry trong tệp ZIP tạm không khớp gói nguồn.".to_string(),
                )
                .format_user_message());
            }
            Ok(())
        },
        false,
    )
}

pub fn save_openxml_verified(
    pkg: &OpenXmlPackage,
    dest_path: &Path,
    baseline_opt: Option<&PreservationSnapshot>,
    policy: &PreservationPolicy,
) -> Result<u64, String> {
    let timer = OpTimer::start("safe_save", "save_openxml_verified", Some(dest_path));
    validate_all_package_xml_entries(pkg)?;
    if let Some(before) = baseline_opt {
        verify_openxml_preservation(before, pkg, policy)?;
    }

    let res = atomic_write_with_validation(
        dest_path,
        |tmp_file| write_package_to_file(pkg, tmp_file),
        |tmp_path| {
            // Re-open the temp file from disk via full OpenXmlPackage reader to guarantee valid ZIP + XML + Preservation
            let reopened = OpenXmlPackage::open(tmp_path)?;
            validate_all_package_xml_entries(&reopened)?;
            if let Some(before) = baseline_opt {
                verify_openxml_preservation(before, &reopened, policy)?;
            }
            Ok(())
        },
        true,
    );

    let size_bytes = match res {
        Ok(sz) => sz,
        Err(e) => {
            timer.finish_err(
                crate::engine::reliability::error_code::classify_error_message(
                    &e,
                    AppErrorCode::ErrFileWrite,
                )
                .code
                .as_str(),
                &e,
            );
            return Err(e);
        }
    };

    // Final disk post-save verification directly on destination path
    let verified_view = OpenXmlPackage::open_selective_for_analysis(dest_path).map_err(|e| {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrPostSaveVerify,
            format!("Không mở lại được tệp sau khi lưu để xác minh: {}", e),
        )
        .format_user_message();
        timer.finish_err("ERR_POST_SAVE_VERIFY", &msg);
        msg
    })?;

    if !verified_view.has_entry("word/document.xml") && !verified_view.has_entry("xl/workbook.xml")
    {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrPostSaveVerify,
            "Tệp đích sau khi lưu thiếu phần nội dung chính.".to_string(),
        )
        .format_user_message();
        timer.finish_err("ERR_POST_SAVE_VERIFY", &msg);
        return Err(msg);
    }

    timer.finish_ok(&format!(
        "Lưu nguyên tử & xác minh hậu lưu thành công ({} bytes)",
        size_bytes
    ));
    Ok(size_bytes)
}

fn write_package_to_file(pkg: &OpenXmlPackage, tmp_file: &mut File) -> Result<(), String> {
    let bytes = pkg.to_bytes()?;
    tmp_file
        .write_all(&bytes)
        .map_err(|e| format_io_error("Lỗi ghi gói OpenXML ra tệp tạm", &e, true))
}

pub fn atomic_write_with_validation<W, V>(
    dest_path: &Path,
    write_fn: W,
    validate_tmp_fn: V,
    use_backup_journal_dir: bool,
) -> Result<u64, String>
where
    W: FnOnce(&mut File) -> Result<(), String>,
    V: FnOnce(&Path) -> Result<(), String>,
{
    if dest_path.is_dir() {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrFileWrite,
            format!(
                "Đường dẫn đích '{}' là thư mục, không thể ghi tệp đè lên thư mục.",
                dest_path.display()
            ),
        )
        .format_user_message());
    }
    let parent = dest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        fs::create_dir_all(parent)
            .map_err(|e| format_io_error("Không tạo được thư mục đích", &e, true))?;
    }

    // Recover any interrupted transaction targeting the same file BEFORE touching it
    let _ = recover_interrupted_save_for_target(dest_path);

    let seq = TMP_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let fname = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file.tmp");
    let tmp_path = parent.join(format!(".{}.mwtmp.{}.{}", fname, pid, seq));

    // 1. Write uniquely named temporary file and fsync before closing
    {
        let mut tmp_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|e| format_io_error("Không tạo được tệp tạm", &e, true))?;

        if let Err(e) = write_fn(&mut tmp_file) {
            drop(tmp_file);
            let _ = fs::remove_file(&tmp_path);
            return Err(e);
        }
        if let Err(e) = tmp_file.flush() {
            drop(tmp_file);
            let _ = fs::remove_file(&tmp_path);
            return Err(format_io_error("Lỗi flush dữ liệu tệp tạm", &e, true));
        }
        if let Err(e) = tmp_file.sync_all() {
            drop(tmp_file);
            let _ = fs::remove_file(&tmp_path);
            return Err(format_io_error("Lỗi đồng bộ tệp tạm xuống đĩa (sync_all)", &e, true));
        }
    }

    // 2. Inject simulated temp-validation fault in test mode if configured
    if let FaultMode::FailTempValidation(reason) = current_fault_mode() {
        let _ = fs::remove_file(&tmp_path);
        return Err(AppStructuredError::new(AppErrorCode::ErrPostSaveVerify, reason).format_user_message());
    }

    // 3. Validate temporary file BEFORE ever touching dest_path!
    let tmp_size = fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
    if tmp_size == 0 {
        let _ = fs::remove_file(&tmp_path);
        return Err(AppStructuredError::new(
            AppErrorCode::ErrPostSaveVerify,
            "Tệp tạm sau khi ghi có kích thước 0 byte; đã hủy thay thế tệp gốc.",
        )
        .format_user_message());
    }
    if let Err(e) = validate_tmp_fn(&tmp_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    // 4. If destination does not exist yet, atomic rename from tmp -> dest
    if !dest_path.exists() {
        if let Err(e) = fs::rename(&tmp_path, dest_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format_io_error("Không thể lưu tệp đích mới", &e, true));
        }
        let final_len = fs::metadata(dest_path).map(|m| m.len()).unwrap_or(tmp_size);
        return Ok(final_len);
    }

    // 5. Destination exists: stage dest_path -> staged_prev (NEVER delete dest_path before commit!),
    //    write journal so power loss mid-swap can recover automatically.
    let original_size = fs::metadata(dest_path).map(|m| m.len()).unwrap_or(0);
    let staged_prev_path = parent.join(format!(".{}.mwprev.{}.{}", fname, pid, seq));
    let journal_path = journal_path_for(dest_path, use_backup_journal_dir);
    if let Some(jp) = journal_path.parent() {
        let _ = fs::create_dir_all(jp);
    }

    let journal = SafeSaveJournal {
        target_path: dest_path.to_string_lossy().to_string(),
        staged_prev_path: staged_prev_path.to_string_lossy().to_string(),
        tmp_new_path: tmp_path.to_string_lossy().to_string(),
        original_size,
        created_at: Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
    };
    if let Ok(j_bytes) = serde_json::to_vec_pretty(&journal) {
        if let Ok(mut jf) = File::create(&journal_path) {
            let _ = jf.write_all(&j_bytes);
            let _ = jf.sync_all();
        }
    }

    // Stage 5a: Move existing dest_path -> staged_prev_path.
    // On Windows, if Word/Excel locks dest_path, this FAILS immediately leaving dest_path 100% untouched!
    if let Err(e) = fs::rename(dest_path, &staged_prev_path) {
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&journal_path);
        if is_locked_or_conflict_io(&e) {
            return Err(AppStructuredError::new(
                AppErrorCode::ErrFileLockedOrConflict,
                format!(
                    "Không thể thay thế '{}' vì tệp đang được chương trình khác khóa/mở ({}). Dữ liệu gốc vẫn nguyên vẹn.",
                    fname, e
                ),
            )
            .format_user_message());
        }
        return Err(format_io_error("Lỗi tạm chuyển tệp đích trước khi thay thế", &e, true));
    }

    // Test hook: simulate abrupt crash/power-loss right after staging original file
    let fault = current_fault_mode();
    if fault == FaultMode::AbortProcessMidSwap {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrFileWrite,
            "Mô phỏng gián đoạn tiến trình / mất điện giữa bước hoán đổi tệp.".to_string(),
        )
        .format_user_message());
    }

    // Stage 5b: Move validated tmp_path -> dest_path
    let commit_res = if fault == FaultMode::FailRenameNewToTarget {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Simulated rename failure during commit",
        ))
    } else {
        fs::rename(&tmp_path, dest_path)
    };

    if let Err(e) = commit_res {
        // Immediate rollback to restore original staged_prev_path -> dest_path!
        let rollback_ok = fs::rename(&staged_prev_path, dest_path).is_ok();
        let _ = fs::remove_file(&tmp_path);
        if rollback_ok {
            let _ = fs::remove_file(&journal_path);
        }
        return Err(AppStructuredError::new(
            if is_locked_or_conflict_io(&e) {
                AppErrorCode::ErrFileLockedOrConflict
            } else {
                AppErrorCode::ErrFileWrite
            },
            format!(
                "Lỗi khi đổi tên tệp tạm thành tệp đích '{}': {}. {}",
                fname,
                e,
                if rollback_ok {
                    "Đã hoàn trả nguyên vẹn bản gốc."
                } else {
                    "Bản gốc còn giữ trong journal phục hồi."
                }
            ),
        )
        .format_user_message());
    }

    // Stage 5c: Confirm dest_path is present & non-empty BEFORE removing staged_prev_path and journal
    let final_meta = fs::metadata(dest_path).map_err(|e| {
        let _ = fs::rename(&staged_prev_path, dest_path);
        format_io_error("Không xác minh được tệp đích sau khi đổi tên", &e, false)
    })?;
    if final_meta.len() == 0 {
        let _ = fs::rename(&staged_prev_path, dest_path);
        return Err(AppStructuredError::new(
            AppErrorCode::ErrPostSaveVerify,
            "Tệp đích sau khi thay thế bị 0 byte; đã tự động khôi phục bản gốc.",
        )
        .format_user_message());
    }

    let _ = fs::remove_file(&staged_prev_path);
    let _ = fs::remove_file(&journal_path);
    Ok(final_meta.len())
}

fn journal_path_for(dest_path: &Path, use_backup_journal_dir: bool) -> PathBuf {
    let parent = dest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let fname = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let sanitized: String = fname
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    if use_backup_journal_dir
        && !parent
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case(".chuanhoa_backup"))
            .unwrap_or(false)
    {
        parent
            .join(".chuanhoa_backup")
            .join(format!(".tx_{}.json", sanitized))
    } else {
        parent.join(format!(".tx_{}.json", sanitized))
    }
}

pub fn recover_interrupted_save_for_target(dest_path: &Path) -> Result<Option<String>, String> {
    let mut notes = Vec::new();
    for use_bak in [true, false] {
        let jpath = journal_path_for(dest_path, use_bak);
        if jpath.is_file() {
            if let Some(msg) = process_single_journal(&jpath) {
                notes.push(msg);
            }
        }
    }
    Ok(notes.into_iter().next())
}

pub fn recover_interrupted_saves_in_dir(dir: &Path) -> Vec<String> {
    let mut recovered = Vec::new();
    let candidates = [dir.to_path_buf(), dir.join(".chuanhoa_backup")];
    for folder in &candidates {
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.starts_with(".tx_") && name.ends_with(".json") && p.is_file() {
                if let Some(msg) = process_single_journal(&p) {
                    recovered.push(msg);
                }
            } else if name.contains(".mwtmp.") && p.is_file() {
                // Only clean orphaned temp files from dead PIDs or older than 2 hours
                if let Some(msg) = cleanup_orphaned_mwtmp(&p, &name) {
                    recovered.push(msg);
                }
            }
        }
    }
    recovered
}

fn process_single_journal(journal_path: &Path) -> Option<String> {
    let raw = fs::read_to_string(journal_path).ok()?;
    let Ok(j) = serde_json::from_str::<SafeSaveJournal>(&raw) else {
        let _ = fs::remove_file(journal_path);
        return None;
    };

    let target = Path::new(&j.target_path);
    let staged = Path::new(&j.staged_prev_path);
    let tmp_new = Path::new(&j.tmp_new_path);

    let msg = if !target.exists() {
        if staged.is_file() && fs::metadata(staged).map(|m| m.len()).unwrap_or(0) > 0 {
            if fs::rename(staged, target).is_ok() {
                let _ = fs::remove_file(tmp_new);
                let _ = fs::remove_file(journal_path);
                let m = format!(
                    "Đã tự động khôi phục nguyên trạng '{}' từ giao dịch lưu bị gián đoạn.",
                    target.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                );
                record_op("safe_save", "recover_interrupted_tx", Some(target), "ok", 0, None, &m);
                Some(m)
            } else {
                None
            }
        } else if tmp_new.is_file() && fs::metadata(tmp_new).map(|m| m.len()).unwrap_or(0) > 0 {
            if fs::rename(tmp_new, target).is_ok() {
                let _ = fs::remove_file(journal_path);
                let m = format!(
                    "Đã hoàn tất tệp '{}' từ tệp tạm hợp lệ còn sót sau gián đoạn.",
                    target.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                );
                record_op("safe_save", "recover_interrupted_tx", Some(target), "ok", 0, None, &m);
                Some(m)
            } else {
                None
            }
        } else {
            let _ = fs::remove_file(journal_path);
            None
        }
    } else {
        let _ = fs::remove_file(staged);
        let _ = fs::remove_file(tmp_new);
        let _ = fs::remove_file(journal_path);
        None
    };

    msg
}

fn cleanup_orphaned_mwtmp(p: &Path, name: &str) -> Option<String> {
    // Format: .<fname>.mwtmp.<pid>.<seq>
    let parts: Vec<&str> = name.split(".mwtmp.").collect();
    if parts.len() != 2 {
        return None;
    }
    let pid_part = parts[1].split('.').next().and_then(|s| s.parse::<u32>().ok());
    let current_pid = std::process::id();
    // Never delete a sibling thread's temp file belonging to the current running process unless stale
    if pid_part == Some(current_pid) {
        return None;
    }
    if fs::remove_file(p).is_ok() {
        Some(format!("Đã dọn tệp tạm mồ côi '{}'", name))
    } else {
        None
    }
}
