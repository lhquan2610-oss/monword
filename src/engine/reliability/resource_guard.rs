use crate::engine::reliability::error_code::{AppErrorCode, AppStructuredError};
use std::cell::RefCell;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    pub max_compressed_file_bytes: u64,
    pub max_uncompressed_total_bytes: u64,
    pub max_single_entry_bytes: u64,
    pub max_single_xml_bytes: u64,
    pub max_zip_entries: usize,
    pub max_compression_ratio: f64,
    pub zip_bomb_min_uncompressed_bytes: u64,
    pub max_xml_elements: usize,
    pub max_xml_depth: usize,
    pub word_com_timeout_ms: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_compressed_file_bytes: 150 * 1024 * 1024,
            max_uncompressed_total_bytes: 512 * 1024 * 1024,
            max_single_entry_bytes: 64 * 1024 * 1024,
            max_single_xml_bytes: 32 * 1024 * 1024,
            max_zip_entries: 5_000,
            max_compression_ratio: 120.0,
            zip_bomb_min_uncompressed_bytes: 16 * 1024 * 1024,
            max_xml_elements: 250_000,
            max_xml_depth: 256,
            word_com_timeout_ms: 45_000,
        }
    }
}

thread_local! {
    static TEST_LIMITS_OVERRIDE: RefCell<Option<ResourceLimits>> = const { RefCell::new(None) };
}

pub fn current_limits() -> ResourceLimits {
    TEST_LIMITS_OVERRIDE.with(|cell| cell.borrow().unwrap_or_default())
}

#[cfg(test)]
pub fn with_custom_limits_for_test<F, R>(limits: ResourceLimits, f: F) -> R
where
    F: FnOnce() -> R,
{
    TEST_LIMITS_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = Some(limits);
    });
    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            TEST_LIMITS_OVERRIDE.with(|cell| {
                *cell.borrow_mut() = None;
            });
        }
    }
    let _guard = ResetGuard;
    f()
}

// ============================================================================
// COOPERATIVE BATCH / IPC TASK CANCELLATION
// ============================================================================

static BATCH_CANCEL_FLAG: AtomicBool = AtomicBool::new(false);

pub fn reset_batch_cancel() {
    BATCH_CANCEL_FLAG.store(false, Ordering::SeqCst);
}

pub fn request_batch_cancel() -> bool {
    BATCH_CANCEL_FLAG.store(true, Ordering::SeqCst);
    true
}

pub fn is_batch_cancelled() -> bool {
    BATCH_CANCEL_FLAG.load(Ordering::SeqCst)
}

// ============================================================================
// ZIP / FILE / XML BOUNDED GUARDS
// ============================================================================

pub fn check_compressed_file_size(bytes_len: u64, path_opt: Option<&Path>) -> Result<(), String> {
    let limits = current_limits();
    if bytes_len > limits.max_compressed_file_bytes {
        let name = path_opt
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("tệp");
        return Err(AppStructuredError::new(
            AppErrorCode::ErrResourceLimit,
            format!(
                "Dung lượng tệp '{}' ({:.2} MB) vượt giới hạn an toàn tối đa {:.2} MB.",
                name,
                bytes_len as f64 / (1024.0 * 1024.0),
                limits.max_compressed_file_bytes as f64 / (1024.0 * 1024.0),
            ),
        )
        .format_user_message());
    }
    Ok(())
}

pub fn check_zip_entry_count(count: usize) -> Result<(), String> {
    let limits = current_limits();
    if count > limits.max_zip_entries {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrResourceLimit,
            format!(
                "Gói ZIP chứa {} tệp con, vượt giới hạn cho phép {} tệp.",
                count, limits.max_zip_entries
            ),
        )
        .format_user_message());
    }
    Ok(())
}

pub fn check_compression_ratio(
    entry_name: &str,
    uncompressed_bytes: u64,
    compressed_bytes: u64,
) -> Result<(), String> {
    let limits = current_limits();
    if uncompressed_bytes >= limits.zip_bomb_min_uncompressed_bytes {
        let denom = compressed_bytes.max(1) as f64;
        let ratio = uncompressed_bytes as f64 / denom;
        if ratio > limits.max_compression_ratio {
            return Err(AppStructuredError::new(
                AppErrorCode::ErrZipBomb,
                format!(
                    "Phát hiện tỷ lệ nén ZIP bất thường ở '{}' ({:.1}:1, giải nén {} bytes từ {} bytes — vượt ngưỡng {:.1}:1).",
                    entry_name, ratio, uncompressed_bytes, compressed_bytes, limits.max_compression_ratio
                ),
            )
            .format_user_message());
        }
    }
    Ok(())
}

pub fn read_zip_entry_bounded<R: Read>(
    reader: &mut R,
    entry_name: &str,
    compressed_size: u64,
    declared_uncompressed: u64,
    running_uncompressed_total: &mut u64,
    is_xml_like: bool,
) -> Result<Vec<u8>, String> {
    let limits = current_limits();
    let per_entry_limit = if is_xml_like {
        limits.max_single_xml_bytes.min(limits.max_single_entry_bytes)
    } else {
        limits.max_single_entry_bytes
    };

    // Fast metadata pre-checks (without trusting ZIP header blindly!)
    check_compression_ratio(entry_name, declared_uncompressed, compressed_size)?;
    if declared_uncompressed > per_entry_limit {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrResourceLimit,
            format!(
                "Thành phần '{}' ({} bytes) vượt giới hạn dung lượng đơn lẻ ({} bytes).",
                entry_name, declared_uncompressed, per_entry_limit
            ),
        )
        .format_user_message());
    }

    let remaining_total = limits
        .max_uncompressed_total_bytes
        .saturating_sub(*running_uncompressed_total);
    if remaining_total == 0 {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrResourceLimit,
            format!(
                "Tổng dung lượng giải nén ZIP vượt giới hạn an toàn {} bytes.",
                limits.max_uncompressed_total_bytes
            ),
        )
        .format_user_message());
    }

    // Read at most `effective_limit + 1` bytes so a spoofed header cannot exhaust RAM
    let effective_limit = per_entry_limit.min(remaining_total);
    let initial_cap = (declared_uncompressed as usize).min(4 * 1024 * 1024);
    let mut data = Vec::with_capacity(initial_cap);
    let mut bounded = reader.take(effective_limit.saturating_add(1));
    bounded
        .read_to_end(&mut data)
        .map_err(|e| format!("Lỗi giải nén '{}': {}", entry_name, e))?;

    let actual_len = data.len() as u64;
    if actual_len > per_entry_limit {
        let ratio = actual_len as f64 / compressed_size.max(1) as f64;
        if actual_len >= limits.zip_bomb_min_uncompressed_bytes
            && ratio > limits.max_compression_ratio
        {
            return Err(AppStructuredError::new(
                AppErrorCode::ErrZipBomb,
                format!(
                    "Phát hiện Zip Bomb tại '{}' (giải nén vượt {} bytes với tỷ lệ nén {:.1}:1).",
                    entry_name, per_entry_limit, ratio
                ),
            )
            .format_user_message());
        }
        return Err(AppStructuredError::new(
            AppErrorCode::ErrResourceLimit,
            format!(
                "Thành phần '{}' vượt giới hạn dung lượng thực tế ({} bytes).",
                entry_name, per_entry_limit
            ),
        )
        .format_user_message());
    }

    check_compression_ratio(entry_name, actual_len, compressed_size)?;

    *running_uncompressed_total = running_uncompressed_total.saturating_add(actual_len);
    if *running_uncompressed_total > limits.max_uncompressed_total_bytes {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrResourceLimit,
            format!(
                "Tổng dung lượng giải nén gói OpenXML vượt ngưỡng {} bytes.",
                limits.max_uncompressed_total_bytes
            ),
        )
        .format_user_message());
    }

    Ok(data)
}

pub fn check_xml_size(byte_len: usize) -> Result<(), String> {
    let limits = current_limits();
    if (byte_len as u64) > limits.max_single_xml_bytes {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrResourceLimit,
            format!(
                "Phần XML có kích thước {} bytes vượt giới hạn {} bytes.",
                byte_len, limits.max_single_xml_bytes
            ),
        )
        .format_user_message());
    }
    Ok(())
}
