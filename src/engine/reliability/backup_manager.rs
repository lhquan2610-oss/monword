use crate::engine::compliance_audit::sha256_util::sha256_hex;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::error_code::{format_io_error, AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::preservation::validate_all_package_xml_entries;
use crate::engine::reliability::safe_save::{save_bytes_atomic, save_json_atomic};
use crate::models::{BackupKind, BackupVersionRecord};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_MAX_PRE_EDIT_VERSIONS: usize = 15;
static BACKUP_SEQ_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct BackupManifest {
    records: Vec<BackupVersionRecord>,
}

#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    pub max_pre_edit_versions: usize,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_pre_edit_versions: DEFAULT_MAX_PRE_EDIT_VERSIONS,
        }
    }
}

pub fn backup_dir_for(original_path: &Path) -> PathBuf {
    let parent = original_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    parent.join(".chuanhoa_backup")
}

fn manifest_path_for(backup_dir: &Path) -> PathBuf {
    backup_dir.join("backup_manifest.json")
}

fn load_manifest(backup_dir: &Path) -> BackupManifest {
    let mp = manifest_path_for(backup_dir);
    let Ok(raw) = fs::read_to_string(mp) else {
        return BackupManifest::default();
    };
    let cleaned = raw.trim_start_matches('\u{feff}');
    serde_json::from_str::<BackupManifest>(cleaned).unwrap_or_default()
}

fn save_manifest(backup_dir: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let mp = manifest_path_for(backup_dir);
    save_json_atomic(&mp, manifest)?;
    Ok(())
}

pub fn create_versioned_backup_with_policy(
    original_path: &Path,
    source_module: &str,
    policy: RetentionPolicy,
) -> Result<BackupVersionRecord, String> {
    let timer = OpTimer::start("backup_manager", "create_backup", Some(original_path));
    if !original_path.is_file() {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrFileRead,
            format!("Không tìm thấy tệp gốc để sao lưu '{}'.", original_path.display()),
        )
        .format_user_message();
        timer.finish_err("ERR_FILE_READ", &msg);
        return Err(msg);
    }

    let raw_bytes = fs::read(original_path)
        .map_err(|e| format_io_error("Lỗi đọc tệp gốc khi tạo bản sao lưu", &e, false))?;
    if raw_bytes.is_empty() {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrBackupInvalid,
            "Tệp gốc rỗng (0 byte), không thể tạo bản sao lưu hợp lệ.",
        )
        .format_user_message();
        timer.finish_err("ERR_BACKUP_INVALID", &msg);
        return Err(msg);
    }

    let backup_dir = backup_dir_for(original_path);
    fs::create_dir_all(&backup_dir)
        .map_err(|e| format_io_error("Không tạo được thư mục .chuanhoa_backup", &e, true))?;

    let fname = original_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document");
    let sha256 = sha256_hex(&raw_bytes);
    let sha8: String = sha256.chars().take(8).collect();

    let mut manifest = load_manifest(&backup_dir);

    let has_valid_initial = manifest.records.iter().any(|r| {
        r.original_file_name.eq_ignore_ascii_case(fname)
            && r.kind == BackupKind::InitialBaseline
            && Path::new(&r.backup_path).is_file()
    });
    let kind = if has_valid_initial {
        BackupKind::PreEditVersion
    } else {
        BackupKind::InitialBaseline
    };

    let next_ver = manifest
        .records
        .iter()
        .filter(|r| r.original_file_name.eq_ignore_ascii_case(fname))
        .map(|r| r.version_number)
        .max()
        .unwrap_or(0)
        + 1;

    // Guaranteed unique collision-free creation via O_EXCL (`create_new(true)`) + monotonic counter
    let mut chosen_name = String::new();
    let mut chosen_path = PathBuf::new();
    for _ in 0..32 {
        let seq = BACKUP_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        let stamp = Local::now().format("%Y%m%d_%H%M%S_%3f");
        let role_tag = if kind == BackupKind::InitialBaseline {
            "_init"
        } else {
            ""
        };
        let candidate_name = format!(
            "{}.{}_v{:04}_{}_{}{}.bak",
            fname, stamp, next_ver, seq, sha8, role_tag
        );
        let candidate_path = backup_dir.join(&candidate_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate_path)
        {
            Ok(mut f) => {
                if let Err(e) = f
                    .write_all(&raw_bytes)
                    .and_then(|_| f.flush())
                    .and_then(|_| f.sync_all())
                {
                    drop(f);
                    let _ = fs::remove_file(&candidate_path);
                    return Err(format_io_error("Lỗi ghi dữ liệu vào tệp sao lưu", &e, true));
                }
                chosen_name = candidate_name;
                chosen_path = candidate_path;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format_io_error("Không tạo được tệp sao lưu mới", &e, true)),
        }
    }

    if chosen_name.is_empty() {
        let msg = AppStructuredError::new(
            AppErrorCode::ErrFileWrite,
            "Không thể cấp phát tên bản sao lưu duy nhất sau nhiều lần thử.",
        )
        .format_user_message();
        timer.finish_err("ERR_FILE_WRITE", &msg);
        return Err(msg);
    }

    // Immediate read-back integrity verification of the backup file just written
    let verified_bytes = fs::read(&chosen_path)
        .map_err(|e| format_io_error("Không đọc lại được bản sao lưu vừa tạo", &e, false))?;
    if verified_bytes.len() != raw_bytes.len() || sha256_hex(&verified_bytes) != sha256 {
        let _ = fs::remove_file(&chosen_path);
        let msg = AppStructuredError::new(
            AppErrorCode::ErrBackupInvalid,
            "Xác minh SHA-256 bản sao lưu vừa tạo không khớp bản gốc.",
        )
        .format_user_message();
        timer.finish_err("ERR_BACKUP_INVALID", &msg);
        return Err(msg);
    }

    let record = BackupVersionRecord {
        backup_id: format!("bak_{}_{}", next_ver, sha8),
        original_file_name: fname.to_string(),
        backup_file_name: chosen_name,
        backup_path: chosen_path.to_string_lossy().to_string(),
        kind,
        version_number: next_ver,
        source_module: source_module.to_string(),
        created_at: Local::now().format("%d/%m/%Y %H:%M:%S%.3f").to_string(),
        size_bytes: raw_bytes.len() as u64,
        sha256,
        integrity_ok: true,
    };

    manifest.records.push(record.clone());
    apply_retention_policy(&mut manifest, fname, policy);
    save_manifest(&backup_dir, &manifest)?;

    timer.finish_ok(&format!(
        "Tạo backup {} ({:?}, {} bytes)",
        record.backup_id, record.kind, record.size_bytes
    ));
    Ok(record)
}

pub fn create_versioned_backup(
    original_path: &Path,
    source_module: &str,
) -> Result<BackupVersionRecord, String> {
    create_versioned_backup_with_policy(original_path, source_module, RetentionPolicy::default())
}

fn apply_retention_policy(
    manifest: &mut BackupManifest,
    original_file_name: &str,
    policy: RetentionPolicy,
) {
    let keep_limit = policy.max_pre_edit_versions.max(1);
    let mut pre_edit_indices: Vec<(usize, u32)> = manifest
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.original_file_name.eq_ignore_ascii_case(original_file_name)
                && r.kind == BackupKind::PreEditVersion
        })
        .map(|(idx, r)| (idx, r.version_number))
        .collect();

    if pre_edit_indices.len() <= keep_limit {
        return;
    }

    // Oldest version numbers first; InitialBaseline is NEVER touched!
    pre_edit_indices.sort_by_key(|(_, ver)| *ver);
    let to_prune_count = pre_edit_indices.len() - keep_limit;
    let mut prune_paths = Vec::with_capacity(to_prune_count);
    for (idx, _) in pre_edit_indices.into_iter().take(to_prune_count) {
        prune_paths.push(manifest.records[idx].backup_path.clone());
    }

    for path_str in &prune_paths {
        let _ = fs::remove_file(path_str);
    }
    manifest
        .records
        .retain(|r| !prune_paths.contains(&r.backup_path));
}

pub fn list_backups_for_file(original_path: &Path) -> Vec<BackupVersionRecord> {
    let backup_dir = backup_dir_for(original_path);
    let fname = original_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if fname.is_empty() {
        return Vec::new();
    }
    let expected_ext = original_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("docx")
        .to_ascii_lowercase();

    let mut records: Vec<BackupVersionRecord> = load_manifest(&backup_dir)
        .records
        .into_iter()
        .filter(|r| r.original_file_name.eq_ignore_ascii_case(fname))
        .collect();

    // Also discover legacy `.bak` files in `.chuanhoa_backup` not yet registered in manifest
    if let Ok(rd) = fs::read_dir(&backup_dir) {
        for entry in rd.filter_map(|e| e.ok()) {
            let p = entry.path();
            let bname = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if p.is_file()
                && bname.starts_with(fname)
                && bname.ends_with(".bak")
                && !records
                    .iter()
                    .any(|r| r.backup_path.eq_ignore_ascii_case(&p.to_string_lossy()))
            {
                let data = fs::read(&p).unwrap_or_default();
                records.push(BackupVersionRecord {
                    backup_id: format!("legacy_{}", bname),
                    original_file_name: fname.to_string(),
                    backup_file_name: bname.clone(),
                    backup_path: p.to_string_lossy().to_string(),
                    kind: if bname.contains("init") {
                        BackupKind::InitialBaseline
                    } else {
                        BackupKind::PreEditVersion
                    },
                    version_number: 0,
                    source_module: "legacy".to_string(),
                    created_at: "-".to_string(),
                    size_bytes: data.len() as u64,
                    sha256: if data.is_empty() {
                        String::new()
                    } else {
                        sha256_hex(&data)
                    },
                    integrity_ok: false,
                });
            }
        }
    }

    for rec in &mut records {
        let bp = Path::new(&rec.backup_path);
        rec.integrity_ok = validate_backup_file(
            bp,
            &expected_ext,
            if rec.sha256.is_empty() {
                None
            } else {
                Some(rec.sha256.as_str())
            },
        )
        .is_ok();
    }

    records.sort_by(|a, b| b.version_number.cmp(&a.version_number));
    records
}

pub fn validate_backup_file(
    backup_path: &Path,
    expected_ext: &str,
    expected_sha256: Option<&str>,
) -> Result<Vec<u8>, String> {
    if !backup_path.is_file() {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrBackupInvalid,
            format!("Không tìm thấy bản sao lưu '{}'.", backup_path.display()),
        )
        .format_user_message());
    }

    let bytes = fs::read(backup_path).map_err(|e| {
        AppStructuredError::new(
            AppErrorCode::ErrBackupInvalid,
            format!("Không thể đọc bản sao lưu '{}': {}", backup_path.display(), e),
        )
        .format_user_message()
    })?;

    if bytes.is_empty() {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrBackupInvalid,
            format!(
                "Bản sao lưu '{}' bị rỗng (0 byte) và đã bị từ chối khôi phục.",
                backup_path.file_name().and_then(|n| n.to_str()).unwrap_or("backup")
            ),
        )
        .format_user_message());
    }

    if let Some(expected_hash) = expected_sha256 {
        let actual_hash = sha256_hex(&bytes);
        if !actual_hash.eq_ignore_ascii_case(expected_hash) {
            return Err(AppStructuredError::new(
                AppErrorCode::ErrBackupInvalid,
                format!(
                    "Bản sao lưu '{}' bị hỏng hoặc đã bị sửa đổi (SHA-256 không khớp).",
                    backup_path.file_name().and_then(|n| n.to_str()).unwrap_or("backup")
                ),
            )
            .format_user_message());
        }
    }

    let ext = expected_ext.trim_start_matches('.').to_ascii_lowercase();
    if matches!(ext.as_str(), "docx" | "docm" | "xlsx" | "xlsm") || expected_ext.is_empty() {
        // Accept valid OpenXML OR valid CFBF (.doc) when ext unknown
        let is_cfbf = bytes.len() >= 8 && bytes[..8] == [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        if !(expected_ext.is_empty() && is_cfbf) {
            let pkg = OpenXmlPackage::from_bytes(&bytes).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrBackupInvalid,
                    format!("Bản sao lưu không phải cấu trúc OpenXML hợp lệ: {}", e),
                )
                .format_user_message()
            })?;
            validate_all_package_xml_entries(&pkg).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrBackupInvalid,
                    format!("Bản sao lưu chứa dữ liệu XML lỗi: {}", e),
                )
                .format_user_message()
            })?;
        }
    } else if ext == "doc" {
        let is_cfbf = bytes.len() >= 8 && bytes[..8] == [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        let is_zip = bytes.len() >= 4 && bytes[..2] == [0x50, 0x4B];
        if !is_cfbf && !is_zip {
            return Err(AppStructuredError::new(
                AppErrorCode::ErrBackupInvalid,
                "Bản sao lưu .doc không đúng định dạng tệp Word hợp lệ.",
            )
            .format_user_message());
        }
    }

    Ok(bytes)
}

pub fn restore_backup_checked(original_path: &Path, backup_path: &Path) -> Result<String, String> {
    let timer = OpTimer::start("backup_manager", "restore_backup", Some(original_path));
    let expected_ext = original_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let manifest = load_manifest(&backup_dir_for(original_path));
    let expected_sha = manifest
        .records
        .iter()
        .find(|r| {
            Path::new(&r.backup_path) == backup_path
                || r.backup_file_name
                    == backup_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
        })
        .map(|r| r.sha256.clone());

    let bytes = match validate_backup_file(backup_path, &expected_ext, expected_sha.as_deref()) {
        Ok(b) => b,
        Err(e) => {
            timer.finish_err("ERR_BACKUP_INVALID", &e);
            return Err(e);
        }
    };

    if let Err(e) = save_bytes_atomic(original_path, &bytes, true) {
        let code = crate::engine::reliability::error_code::classify_error_message(
            &e,
            AppErrorCode::ErrFileWrite,
        )
        .code
        .as_str();
        timer.finish_err(code, &e);
        return Err(e);
    }

    let msg = format!(
        "Đã kiểm định toàn vẹn và khôi phục nguyên trạng '{}' từ bản sao lưu '{}'.",
        original_path.file_name().and_then(|n| n.to_str()).unwrap_or("tệp"),
        backup_path.file_name().and_then(|n| n.to_str()).unwrap_or("backup")
    );
    timer.finish_ok(&msg);
    Ok(msg)
}

pub fn restore_initial_baseline(original_path: &Path) -> Result<String, String> {
    let records = list_backups_for_file(original_path);
    let initial = records
        .iter()
        .find(|r| r.kind == BackupKind::InitialBaseline && r.integrity_ok)
        .or_else(|| records.iter().rev().find(|r| r.integrity_ok))
        .ok_or_else(|| {
            AppStructuredError::new(
                AppErrorCode::ErrBackupInvalid,
                format!(
                    "Không tìm thấy bản sao lưu gốc (Initial Baseline) hợp lệ cho '{}'.",
                    original_path.file_name().and_then(|n| n.to_str()).unwrap_or("tệp")
                ),
            )
            .format_user_message()
        })?;
    restore_backup_checked(original_path, Path::new(&initial.backup_path))
}

pub fn restore_latest_pre_edit(original_path: &Path) -> Result<String, String> {
    let records = list_backups_for_file(original_path);
    let latest = records
        .iter()
        .find(|r| r.integrity_ok)
        .ok_or_else(|| {
            AppStructuredError::new(
                AppErrorCode::ErrBackupInvalid,
                format!(
                    "Không tìm thấy bản sao lưu gần nhất hợp lệ cho '{}'.",
                    original_path.file_name().and_then(|n| n.to_str()).unwrap_or("tệp")
                ),
            )
            .format_user_message()
        })?;
    restore_backup_checked(original_path, Path::new(&latest.backup_path))
}
