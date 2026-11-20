use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows_registry::CURRENT_USER;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const SFX_MAGIC: &[u8; 8] = b"MWSETUP4";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PUBLISHER_NAME: &str = "Mon Office Ecosystem";
pub const PRODUCT_NAME: &str = "MonWord — Chuẩn Hóa Văn Bản Pro";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALL_REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\MonWord";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallManifest {
    pub app_name: String,
    pub version: String,
    pub publisher: String,
    pub architecture: String,
    pub installed_at: String,
    pub previous_version_detected: Option<String>,
    pub upgraded: bool,
    pub install_dir: String,
    pub appdata_dir: String,
    pub user_data_untouched: bool,
    pub installed_files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub install_dir: PathBuf,
    pub appdata_dir: PathBuf,
    pub create_shortcuts: bool,
    pub write_registry: bool,
}

impl InstallOptions {
    #[allow(dead_code)]
    pub fn default_paths() -> Self {
        Self {
            install_dir: default_install_dir(),
            appdata_dir: default_user_appdata_dir(),
            create_shortcuts: true,
            write_registry: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub version: String,
    pub previous_version: Option<String>,
    pub upgraded: bool,
    pub install_dir: PathBuf,
    pub installed_files: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UninstallOutcome {
    pub removed_files: Vec<String>,
    pub install_dir: PathBuf,
    pub user_data_preserved: bool,
}

#[allow(dead_code)]
pub fn default_install_dir() -> PathBuf {
    if let Ok(local_app) = env::var("LOCALAPPDATA") {
        if !local_app.trim().is_empty() {
            return PathBuf::from(local_app).join("Programs").join("MonWord");
        }
    }
    env::temp_dir().join("Programs").join("MonWord")
}

#[allow(dead_code)]
pub fn default_user_appdata_dir() -> PathBuf {
    if let Ok(app_data) = env::var("APPDATA") {
        if !app_data.trim().is_empty() {
            return PathBuf::from(app_data).join("MonWord");
        }
    }
    env::temp_dir().join("MonWord")
}

/// Packs a compiled stub executable and a list of `(archive_rel_path, disk_path)` entries
/// into a standalone Windows x64 self-extracting setup `.exe`.
pub fn pack_self_extracting_installer(
    stub_exe_path: &Path,
    payload_files: &[(&str, &Path)],
    out_exe_path: &Path,
) -> Result<u64, String> {
    let stub_bytes = fs::read(stub_exe_path)
        .map_err(|e| format!("Không đọc được stub trình cài đặt {:?}: {}", stub_exe_path, e))?;
    // If the input stub itself already has a trailer, strip it first so repacking is idempotent.
    let clean_stub = match parse_trailer_offsets(&stub_bytes) {
        Some((stub_len, _)) => &stub_bytes[..stub_len as usize],
        None => stub_bytes.as_slice(),
    };

    let mut zip_cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zw = ZipWriter::new(&mut zip_cursor);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for &(rel_path, src_path) in payload_files {
            let clean_rel = normalize_rel_path(rel_path)?;
            let data = fs::read(src_path)
                .map_err(|e| format!("Không đọc được tệp đóng gói {:?}: {}", src_path, e))?;
            zw.start_file(&clean_rel, opts)
                .map_err(|e| format!("Lỗi tạo entry gói cài đặt '{}': {}", clean_rel, e))?;
            zw.write_all(&data)
                .map_err(|e| format!("Lỗi ghi entry gói cài đặt '{}': {}", clean_rel, e))?;
        }
        zw.finish()
            .map_err(|e| format!("Lỗi hoàn tất nén payload cài đặt: {}", e))?;
    }
    let zip_bytes = zip_cursor.into_inner();

    if let Some(parent) = out_exe_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Không tạo được thư mục đích {:?}: {}", parent, e))?;
        }
    }

    let tmp_out = out_exe_path.with_extension("exe.tmp");
    let _ = fs::remove_file(&tmp_out);
    {
        let mut out_f = File::create(&tmp_out)
            .map_err(|e| format!("Không tạo được tệp setup tạm {:?}: {}", tmp_out, e))?;
        out_f
            .write_all(clean_stub)
            .and_then(|_| out_f.write_all(&zip_bytes))
            .and_then(|_| out_f.write_all(&(clean_stub.len() as u64).to_le_bytes()))
            .and_then(|_| out_f.write_all(&(zip_bytes.len() as u64).to_le_bytes()))
            .and_then(|_| out_f.write_all(SFX_MAGIC))
            .and_then(|_| out_f.sync_all())
            .map_err(|e| format!("Lỗi ghi gói cài đặt tự giải nén: {}", e))?;
    }
    let _ = fs::remove_file(out_exe_path);
    fs::rename(&tmp_out, out_exe_path)
        .map_err(|e| format!("Lỗi hoàn tất tên tệp cài đặt {:?}: {}", out_exe_path, e))?;

    let total_size = fs::metadata(out_exe_path).map(|m| m.len()).unwrap_or(0);
    Ok(total_size)
}

/// Extracts `(stub_bytes, zip_payload_bytes)` from a packed self-extracting setup `.exe`.
pub fn extract_sfx_sections(exe_path: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut f = File::open(exe_path)
        .map_err(|e| format!("Không mở được tệp cài đặt {:?}: {}", exe_path, e))?;
    let total_len = f
        .metadata()
        .map_err(|e| format!("Không đọc được kích thước tệp cài đặt: {}", e))?
        .len();
    if total_len < 24 {
        return Err("Tệp cài đặt không hợp lệ (quá nhỏ).".to_string());
    }

    f.seek(SeekFrom::End(-24))
        .map_err(|e| format!("Lỗi đọc trailer tệp cài đặt: {}", e))?;
    let mut trailer = [0u8; 24];
    f.read_exact(&mut trailer)
        .map_err(|e| format!("Lỗi đọc trailer tệp cài đặt: {}", e))?;

    if &trailer[16..24] != SFX_MAGIC {
        return Err("Tệp không chứa gói dữ liệu cài đặt MonWord hợp lệ (thiếu chữ ký MWSETUP4).".to_string());
    }
    let stub_len = u64::from_le_bytes(trailer[0..8].try_into().unwrap());
    let payload_len = u64::from_le_bytes(trailer[8..16].try_into().unwrap());
    if stub_len == 0 || payload_len == 0 || stub_len.saturating_add(payload_len).saturating_add(24) != total_len {
        return Err("Cấu trúc trailer gói cài đặt bị hỏng hoặc sai kích thước.".to_string());
    }

    f.seek(SeekFrom::Start(0))
        .map_err(|e| format!("Lỗi seek phần stub: {}", e))?;
    let mut stub_bytes = vec![0u8; stub_len as usize];
    f.read_exact(&mut stub_bytes)
        .map_err(|e| format!("Lỗi đọc phần stub cài đặt: {}", e))?;

    let mut payload_bytes = vec![0u8; payload_len as usize];
    f.read_exact(&mut payload_bytes)
        .map_err(|e| format!("Lỗi đọc phần payload ZIP cài đặt: {}", e))?;

    Ok((stub_bytes, payload_bytes))
}

fn parse_trailer_offsets(raw: &[u8]) -> Option<(u64, u64)> {
    if raw.len() < 24 || &raw[raw.len() - 8..] != SFX_MAGIC {
        return None;
    }
    let t = &raw[raw.len() - 24..];
    let stub_len = u64::from_le_bytes(t[0..8].try_into().ok()?);
    let payload_len = u64::from_le_bytes(t[8..16].try_into().ok()?);
    if stub_len.saturating_add(payload_len).saturating_add(24) == raw.len() as u64 {
        Some((stub_len, payload_len))
    } else {
        None
    }
}

/// Runs fresh installation or safe in-place upgrade from `sfx_exe_path`.
/// Never modifies or deletes user data in `opts.appdata_dir` or `.chuanhoa_backup`.
pub fn run_install_from_sfx(sfx_exe_path: &Path, opts: &InstallOptions) -> Result<InstallOutcome, String> {
    let (stub_bytes, payload_bytes) = extract_sfx_sections(sfx_exe_path)?;
    run_install_from_parts(&stub_bytes, &payload_bytes, opts)
}

pub fn run_install_from_parts(
    stub_bytes: &[u8],
    payload_bytes: &[u8],
    opts: &InstallOptions,
) -> Result<InstallOutcome, String> {
    let install_dir = &opts.install_dir;
    fs::create_dir_all(install_dir)
        .map_err(|e| format!("Không thể tạo thư mục cài đặt {:?}: {}", install_dir, e))?;

    let previous_version = detect_previous_version(install_dir);
    let upgraded = previous_version.is_some();

    let cursor = std::io::Cursor::new(payload_bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| format!("Gói payload cài đặt bị hỏng (ZIP không hợp lệ): {}", e))?;

    let mut installed_files = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Lỗi giải nén thành phần #{}: {}", i, e))?;
        if entry.is_dir() {
            continue;
        }
        let rel = normalize_rel_path(entry.name())?;
        let dest = install_dir.join(&rel);
        let mut buf = Vec::with_capacity(entry.size().min(16 * 1024 * 1024) as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("Lỗi giải nén '{}': {}", rel, e))?;
        write_atomic_file(&dest, &buf)?;
        installed_files.push(rel);
    }

    // Write clean uninstaller stub (without payload trailer) into install_dir/uninstall.exe
    let uninstall_dest = install_dir.join("uninstall.exe");
    write_atomic_file(&uninstall_dest, stub_bytes)?;
    installed_files.push("uninstall.exe".to_string());

    // Remove legacy pre-rebrand binary if upgrading
    let legacy_exe = install_dir.join("ChuanHoaVanBanPro.exe");
    if legacy_exe.is_file() {
        let _ = fs::remove_file(&legacy_exe);
    }

    // Verify required runtime deliverables exist & non-empty
    for required in ["MonWord.exe", "WebView2Loader.dll"] {
        let p = install_dir.join(required);
        let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        if size == 0 {
            return Err(format!(
                "Kiểm tra sau cài đặt thất bại: thiếu tệp bắt buộc '{}' tại {:?}",
                required, install_dir
            ));
        }
    }

    installed_files.sort();
    let manifest = InstallManifest {
        app_name: "MonWord".to_string(),
        version: APP_VERSION.to_string(),
        publisher: PUBLISHER_NAME.to_string(),
        architecture: "x64".to_string(),
        installed_at: chrono::Local::now().to_rfc3339(),
        previous_version_detected: previous_version.clone(),
        upgraded,
        install_dir: install_dir.to_string_lossy().to_string(),
        appdata_dir: opts.appdata_dir.to_string_lossy().to_string(),
        user_data_untouched: true,
        installed_files: installed_files.clone(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Lỗi tuần tự hóa manifest cài đặt: {}", e))?;
    write_atomic_file(&install_dir.join("install_manifest.json"), manifest_json.as_bytes())?;

    if opts.create_shortcuts {
        create_windows_shortcuts(install_dir);
    }
    if opts.write_registry {
        register_uninstall_entry(install_dir, &installed_files)?;
    }

    Ok(InstallOutcome {
        version: APP_VERSION.to_string(),
        previous_version,
        upgraded,
        install_dir: install_dir.clone(),
        installed_files,
    })
}

/// Uninstalls MonWord program binaries and documentation from `opts.install_dir`.
/// Strictly preserves all user data in `%APPDATA%\MonWord` and `.chuanhoa_backup`.
pub fn run_uninstall(opts: &InstallOptions) -> Result<UninstallOutcome, String> {
    let install_dir = &opts.install_dir;
    let mut removed_files = Vec::new();

    if install_dir.is_dir() {
        let manifest_path = install_dir.join("install_manifest.json");
        if let Ok(raw) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<InstallManifest>(&raw) {
                for rel in manifest.installed_files {
                    if rel.eq_ignore_ascii_case("uninstall.exe") {
                        continue;
                    }
                    if let Ok(clean_rel) = normalize_rel_path(&rel) {
                        let p = install_dir.join(&clean_rel);
                        if p.is_file() && fs::remove_file(&p).is_ok() {
                            removed_files.push(clean_rel);
                        }
                    }
                }
            }
        }

        for fixed in [
            "MonWord.exe",
            "WebView2Loader.dll",
            "monword.ico",
            "README.md",
            "install_manifest.json",
            "ChuanHoaVanBanPro.exe",
        ] {
            let p = install_dir.join(fixed);
            if p.is_file() && fs::remove_file(&p).is_ok() {
                removed_files.push(fixed.to_string());
            }
        }

        let docs_dir = install_dir.join("docs");
        if docs_dir.is_dir() && fs::remove_dir_all(&docs_dir).is_ok() {
            removed_files.push("docs/".to_string());
        }

        let self_exe = env::current_exe().ok();
        let uninstall_exe = install_dir.join("uninstall.exe");
        if uninstall_exe.is_file() {
            let is_self_running = self_exe
                .as_ref()
                .map(|s| s.to_string_lossy().eq_ignore_ascii_case(&uninstall_exe.to_string_lossy()))
                .unwrap_or(false);
            if !is_self_running {
                if fs::remove_file(&uninstall_exe).is_ok() {
                    removed_files.push("uninstall.exe".to_string());
                }
                let _ = fs::remove_dir(install_dir);
            } else {
                schedule_self_cleanup(&uninstall_exe, install_dir);
            }
        } else {
            let _ = fs::remove_dir(install_dir);
        }
    }

    if opts.create_shortcuts {
        remove_windows_shortcuts();
    }
    if opts.write_registry {
        let _ = CURRENT_USER.remove_tree(UNINSTALL_REG_KEY);
    }

    Ok(UninstallOutcome {
        removed_files,
        install_dir: install_dir.clone(),
        user_data_preserved: true,
    })
}

fn detect_previous_version(install_dir: &Path) -> Option<String> {
    let manifest_path = install_dir.join("install_manifest.json");
    if let Ok(raw) = fs::read_to_string(&manifest_path) {
        if let Ok(m) = serde_json::from_str::<InstallManifest>(&raw) {
            if !m.version.trim().is_empty() {
                return Some(m.version);
            }
        }
    }
    if install_dir.join("MonWord.exe").is_file() {
        return Some("1.0.0-portable".to_string());
    }
    if install_dir.join("ChuanHoaVanBanPro.exe").is_file() {
        return Some("legacy-pre-4.0".to_string());
    }
    None
}

fn normalize_rel_path(raw: &str) -> Result<String, String> {
    if raw.is_empty()
        || raw.contains(':')
        || raw.chars().any(|c| c < ' ')
        || raw.split(['/', '\\']).any(|s| s == ".." || s == ".")
    {
        return Err(format!("Đường dẫn cài đặt không hợp lệ: '{}'", raw));
    }
    let clean = raw.trim_start_matches(['/', '\\']).replace('\\', "/");
    if clean.is_empty() {
        return Err("Đường dẫn cài đặt rỗng.".to_string());
    }
    Ok(clean)
}

fn write_atomic_file(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Không tạo được thư mục {:?}: {}", parent, e))?;
        }
    }
    let fname = dest.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dest.with_file_name(format!(".{}.mwtmpsetup.{}", fname, std::process::id()));
    let _ = fs::remove_file(&tmp);
    {
        let mut f = File::create(&tmp)
            .map_err(|e| format!("Không tạo được tệp tạm {:?}: {}", tmp, e))?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| format!("Lỗi ghi tệp tạm {:?}: {}", tmp, e))?;
    }
    let _ = fs::remove_file(dest);
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Không thể cập nhật tệp {:?} (hãy đóng MonWord đang chạy): {}", dest, e)
    })
}

fn create_windows_shortcuts(install_dir: &Path) {
    let exe_path = install_dir.join("MonWord.exe");
    let uninst_path = install_dir.join("uninstall.exe");
    let ico_path = install_dir.join("monword.ico");
    let ps = format!(
        r#"
$wsh = New-Object -ComObject WScript.Shell
$desktop = [Environment]::GetFolderPath('Desktop')
$s1 = $wsh.CreateShortcut((Join-Path $desktop 'MonWord.lnk'))
$s1.TargetPath = '{exe}'
$s1.WorkingDirectory = '{dir}'
if (Test-Path '{ico}') {{ $s1.IconLocation = '{ico},0' }}
$s1.Description = 'MonWord 4.0 - Chuan Hoa Van Ban Hanh Chinh Pro'
$s1.Save()
$smDir = Join-Path ([Environment]::GetFolderPath('Programs')) 'MonWord'
New-Item -ItemType Directory -Force -Path $smDir | Out-Null
$s2 = $wsh.CreateShortcut((Join-Path $smDir 'MonWord.lnk'))
$s2.TargetPath = '{exe}'
$s2.WorkingDirectory = '{dir}'
if (Test-Path '{ico}') {{ $s2.IconLocation = '{ico},0' }}
$s2.Save()
$s3 = $wsh.CreateShortcut((Join-Path $smDir 'Go cai dat MonWord.lnk'))
$s3.TargetPath = '{uninst}'
$s3.Arguments = '--uninstall'
$s3.WorkingDirectory = '{dir}'
$s3.Save()
"#,
        exe = ps_sq(&exe_path.to_string_lossy()),
        dir = ps_sq(&install_dir.to_string_lossy()),
        ico = ps_sq(&ico_path.to_string_lossy()),
        uninst = ps_sq(&uninst_path.to_string_lossy()),
    );
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

fn remove_windows_shortcuts() {
    let ps = r#"
$desktop = [Environment]::GetFolderPath('Desktop')
Remove-Item -Path (Join-Path $desktop 'MonWord.lnk') -Force -ErrorAction SilentlyContinue
$smDir = Join-Path ([Environment]::GetFolderPath('Programs')) 'MonWord'
if (Test-Path $smDir) { Remove-Item -Path $smDir -Recurse -Force -ErrorAction SilentlyContinue }
"#;
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", ps])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

fn register_uninstall_entry(install_dir: &Path, files: &[String]) -> Result<(), String> {
    let total_bytes: u64 = files
        .iter()
        .filter_map(|rel| fs::metadata(install_dir.join(rel)).ok().map(|m| m.len()))
        .sum();
    let est_kb = ((total_bytes / 1024).max(1)) as u32;
    let key = CURRENT_USER
        .create(UNINSTALL_REG_KEY)
        .map_err(|e| format!("Không thể ghi thông tin gỡ cài đặt vào Registry HKCU: {}", e))?;
    let exe_str = install_dir.join("MonWord.exe").to_string_lossy().to_string();
    let uninst_cmd = format!("\"{}\" --uninstall", install_dir.join("uninstall.exe").display());
    let _ = key.set_string("DisplayName", PRODUCT_NAME);
    let _ = key.set_string("DisplayVersion", APP_VERSION);
    let _ = key.set_string("Publisher", PUBLISHER_NAME);
    let _ = key.set_string("DisplayIcon", &exe_str);
    let _ = key.set_string("InstallLocation", &install_dir.to_string_lossy());
    let _ = key.set_string("UninstallString", &uninst_cmd);
    let _ = key.set_u32("NoModify", 1);
    let _ = key.set_u32("NoRepair", 1);
    let _ = key.set_u32("EstimatedSize", est_kb);
    Ok(())
}

fn schedule_self_cleanup(uninst_exe: &Path, install_dir: &Path) {
    let ps = format!(
        "Start-Sleep -Milliseconds 600; Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue; Remove-Item -LiteralPath '{}' -ErrorAction SilentlyContinue",
        ps_sq(&uninst_exe.to_string_lossy()),
        ps_sq(&install_dir.to_string_lossy()),
    );
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

fn ps_sq(s: &str) -> String {
    s.replace('\'', "''")
}
