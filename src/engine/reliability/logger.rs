use crate::models::OperationLogEntry;
use chrono::Local;
use regex::Regex;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const MAX_IN_MEMORY_LOGS: usize = 500;
const MAX_LOG_FILE_BYTES: u64 = 2 * 1024 * 1024;

static LOG_RING: OnceLock<Mutex<VecDeque<OperationLogEntry>>> = OnceLock::new();
static LOG_FILE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

thread_local! {
    static TEST_LOG_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

fn ring() -> &'static Mutex<VecDeque<OperationLogEntry>> {
    LOG_RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_IN_MEMORY_LOGS)))
}

fn file_mutex() -> &'static Mutex<()> {
    LOG_FILE_MUTEX.get_or_init(|| Mutex::new(()))
}

pub fn default_log_file_path() -> PathBuf {
    let base = env::var("APPDATA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            env::var("LOCALAPPDATA")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    base.join("MonWord").join("logs").join("monword_ops.jsonl")
}

fn active_log_file_path() -> PathBuf {
    TEST_LOG_OVERRIDE.with(|cell| {
        cell.borrow()
            .clone()
            .unwrap_or_else(default_log_file_path)
    })
}

#[cfg(test)]
pub fn with_test_log_file<F, R>(log_path: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    TEST_LOG_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = Some(log_path.to_path_buf());
    });
    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            TEST_LOG_OVERRIDE.with(|cell| {
                *cell.borrow_mut() = None;
            });
        }
    }
    let _guard = ResetGuard;
    f()
}

pub fn safe_file_label(path_opt: Option<&Path>) -> String {
    match path_opt {
        Some(p) => p
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("unknown")
            .to_string(),
        None => "-".to_string(),
    }
}

fn re_win_drive_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)\b([a-z]):[\\/](?:[^\\/:*?"<>|\r\n'\s]+[\\/])+([^\\/:*?"<>|\r\n'\s]+)"#)
            .expect("valid path regex")
    })
}

fn re_unc_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"\\\\[^\\/\s]+[\\/](?:[^\\/:*?"<>|\r\n'\s]+[\\/])*([^\\/:*?"<>|\r\n'\s]+)"#)
            .expect("valid unc regex")
    })
}

/// Scrubs potentially sensitive paths/newlines and caps length so operational logs stay privacy-safe.
pub fn sanitize_detail(raw: &str) -> String {
    let single_line = raw
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let no_unc = re_unc_path().replace_all(&single_line, "[unc]/$1");
    let no_drive = re_win_drive_path().replace_all(&no_unc, "[path]/$2");
    let mut out = String::new();
    for ch in no_drive.chars().take(220) {
        out.push(ch);
    }
    if no_drive.chars().count() > 220 {
        out.push_str("...");
    }
    out
}

pub fn record_op(
    module: &str,
    task: &str,
    path_opt: Option<&Path>,
    status: &str,
    elapsed_ms: u64,
    error_code: Option<&str>,
    detail: &str,
) {
    let entry = OperationLogEntry {
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        task: task.to_string(),
        file_name: safe_file_label(path_opt),
        module: module.to_string(),
        status: status.to_string(),
        elapsed_ms,
        error_code: error_code.map(str::to_string),
        detail: sanitize_detail(detail),
    };

    if let Ok(mut buf) = ring().lock() {
        buf.push_front(entry.clone());
        while buf.len() > MAX_IN_MEMORY_LOGS {
            buf.pop_back();
        }
    }

    append_log_entry_to_disk(&active_log_file_path(), &entry);
}

fn append_log_entry_to_disk(log_path: &Path, entry: &OperationLogEntry) {
    let Ok(_guard) = file_mutex().lock() else {
        return;
    };
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(meta) = fs::metadata(log_path) {
        if meta.len() >= MAX_LOG_FILE_BYTES {
            let rotated = PathBuf::from(format!("{}.1", log_path.to_string_lossy()));
            let _ = fs::remove_file(&rotated);
            let _ = fs::rename(log_path, &rotated);
        }
    }

    if let Ok(json_line) = serde_json::to_string(entry) {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
            let _ = writeln!(file, "{}", json_line);
        }
    }
}

pub fn get_recent_logs(limit: usize) -> Vec<OperationLogEntry> {
    let cap = limit.clamp(1, MAX_IN_MEMORY_LOGS);
    let in_mem: Vec<OperationLogEntry> = if let Ok(buf) = ring().lock() {
        buf.iter().take(cap).cloned().collect()
    } else {
        Vec::new()
    };
    if !in_mem.is_empty() {
        return in_mem;
    }

    // Hydrate from disk if app just started cleanly
    read_logs_from_file(&active_log_file_path(), cap)
}

pub fn read_logs_from_file(path: &Path, limit: usize) -> Vec<OperationLogEntry> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut parsed: Vec<OperationLogEntry> = raw
        .lines()
        .filter_map(|line| serde_json::from_str::<OperationLogEntry>(line).ok())
        .collect();
    parsed.reverse();
    parsed.truncate(limit);
    parsed
}

pub fn clear_logs() -> Result<(), String> {
    if let Ok(mut buf) = ring().lock() {
        buf.clear();
    }
    let path = active_log_file_path();
    let _guard = file_mutex().lock();
    if path.is_file() {
        fs::write(&path, b"")
            .map_err(|e| format!("[ERR_FILE_WRITE] Không thể xóa tệp log: {}", e))?;
    }
    Ok(())
}

pub struct OpTimer {
    module: &'static str,
    task: &'static str,
    file_name: Option<PathBuf>,
    started: Instant,
}

impl OpTimer {
    pub fn start(module: &'static str, task: &'static str, path_opt: Option<&Path>) -> Self {
        Self {
            module,
            task,
            file_name: path_opt.map(|p| p.to_path_buf()),
            started: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub fn finish_ok(&self, detail: &str) {
        record_op(
            self.module,
            self.task,
            self.file_name.as_deref(),
            "ok",
            self.elapsed_ms(),
            None,
            detail,
        );
    }

    pub fn finish_err(&self, error_code: &str, detail: &str) {
        record_op(
            self.module,
            self.task,
            self.file_name.as_deref(),
            "error",
            self.elapsed_ms(),
            Some(error_code),
            detail,
        );
    }

    pub fn finish_cancelled(&self, detail: &str) {
        record_op(
            self.module,
            self.task,
            self.file_name.as_deref(),
            "cancelled",
            self.elapsed_ms(),
            Some("ERR_TASK_CANCELLED"),
            detail,
        );
    }
}
