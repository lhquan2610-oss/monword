use crate::ipc::{emit_js_event, UserEvent};
use regex::Regex;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tao::event_loop::EventLoopProxy;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const EMBEDDED_APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_CODENAME: &str = "MonWord 4.0 Enterprise";
pub const LIVE_OTA_ORIGIN: &str = "https://monword.vercel.app";
pub const GITHUB_RAW_VERSION_URL: &str =
    "https://raw.githubusercontent.com/lequan2512/monword/main/ui/public/version.json";
pub const GITHUB_RELEASES_URL: &str = "https://github.com/lequan2512/monword/releases/latest";

pub fn normalize_version(v: &str) -> String {
    v.trim().trim_start_matches(['v', 'V']).to_string()
}

pub fn compare_semver(a: &str, b: &str) -> i32 {
    let parse = |s: &str| -> Vec<i32> {
        normalize_version(s)
            .split(['.', '-'])
            .map(|part| part.parse::<i32>().unwrap_or(0))
            .collect()
    };
    let pa = parse(a);
    let pb = parse(b);
    let len = pa.len().max(pb.len()).max(3);
    for i in 0..len {
        let va = *pa.get(i).unwrap_or(&0);
        let vb = *pb.get(i).unwrap_or(&0);
        if va > vb {
            return 1;
        }
        if va < vb {
            return -1;
        }
    }
    0
}

pub fn normalize_ota_rel_path(raw_path: &str) -> Option<String> {
    let stripped = raw_path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches('/');
    let rel = if stripped.is_empty() {
        "index.html"
    } else {
        stripped
    };

    if rel.contains('\\') || rel.contains(':') {
        return None;
    }

    let mut clean_parts = Vec::new();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return None;
        }
        clean_parts.push(seg);
    }

    if clean_parts.is_empty() {
        None
    } else {
        Some(clean_parts.join("/"))
    }
}

pub fn get_ota_dist_dir() -> PathBuf {
    if let Ok(local_app) = env::var("LOCALAPPDATA") {
        if !local_app.trim().is_empty() {
            return PathBuf::from(local_app).join("MonWord").join("ota-dist");
        }
    }
    env::temp_dir().join("MonWord").join("ota-dist")
}

pub fn get_active_ota_version() -> Option<String> {
    let ota_dir = get_ota_dist_dir();
    let ver_path = ota_dir.join("version.json");
    let index_path = ota_dir.join("index.html");
    if !ver_path.is_file() || !index_path.is_file() {
        return None;
    }
    let raw = fs::read_to_string(&ver_path).ok()?;
    let cleaned = raw.trim_start_matches('\u{feff}');
    let data: Value = serde_json::from_str(cleaned).ok()?;
    let ota_ver = normalize_version(data.get("version")?.as_str()?);
    if !ota_ver.is_empty() && compare_semver(&ota_ver, EMBEDDED_APP_VERSION) > 0 {
        Some(ota_ver)
    } else {
        None
    }
}

pub fn effective_version() -> String {
    get_active_ota_version().unwrap_or_else(|| EMBEDDED_APP_VERSION.to_string())
}

pub fn read_ota_asset(raw_path: &str) -> Option<(&'static str, Vec<u8>)> {
    if get_active_ota_version().is_none() {
        return None;
    }
    let rel = normalize_ota_rel_path(raw_path)?;
    let full = get_ota_dist_dir().join(&rel);
    if !full.is_file() {
        return None;
    }
    let bytes = fs::read(full).ok()?;
    Some((detect_mime(&rel), bytes))
}

pub fn get_app_info() -> Value {
    let ota_ver = get_active_ota_version();
    json!({
        "name": "MonWord",
        "publisher": "Hệ sinh thái Văn phòng Mon (Mon Office Ecosystem)",
        "arch": "Windows x64",
        "codename": APP_CODENAME,
        "embeddedVersion": EMBEDDED_APP_VERSION,
        "effectiveVersion": effective_version(),
        "otaActive": ota_ver.is_some(),
        "otaVersion": ota_ver,
        "otaDir": get_ota_dist_dir().to_string_lossy(),
        "webUrl": LIVE_OTA_ORIGIN,
    })
}

pub fn check_remote_ota() -> Value {
    let current = effective_version();
    let ota_active = get_active_ota_version().is_some();
    let urls = [
        format!("{}/version.json", LIVE_OTA_ORIGIN),
        GITHUB_RAW_VERSION_URL.to_string(),
    ];

    for url in &urls {
        if let Ok(text) = fetch_text_with_ps(url, 5) {
            let trimmed = text.trim_start_matches('\u{feff}');
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                if let Some(ver_str) = v.get("version").and_then(|x| x.as_str()) {
                    let latest = normalize_version(ver_str);
                    let has_update = compare_semver(&latest, &current) > 0;
                    return json!({
                        "currentVersion": current,
                        "latestVersion": latest,
                        "codename": v.get("codename").and_then(|x| x.as_str()).unwrap_or(APP_CODENAME),
                        "hasUpdate": has_update,
                        "releaseDate": v.get("releaseDate").and_then(|x| x.as_str()).unwrap_or("19/11/2026"),
                        "releaseNotes": v.get("releaseNotes").cloned().unwrap_or_else(|| json!([])),
                        "downloadUrl": v.get("downloadUrl").and_then(|x| x.as_str()).unwrap_or(GITHUB_RELEASES_URL),
                        "isDirectExe": false,
                        "otaActive": ota_active,
                        "history": v.get("history").cloned().unwrap_or_else(|| json!([])),
                        "source": url,
                        "offline": false,
                    });
                }
            }
        }
    }

    // Graceful response when offline or cloud site is not deployed yet
    json!({
        "currentVersion": current,
        "latestVersion": current,
        "codename": APP_CODENAME,
        "hasUpdate": false,
        "releaseDate": "19/11/2026",
        "releaseNotes": [],
        "downloadUrl": GITHUB_RELEASES_URL,
        "isDirectExe": false,
        "otaActive": ota_active,
        "history": [],
        "offline": true,
    })
}

pub fn apply_desktop_live_ota(proxy: &EventLoopProxy<UserEvent>) -> Result<Value, String> {
    let emit_pct = |pct: u32, status: &str| {
        emit_js_event(
            proxy,
            "update_progress",
            json!({
                "percent": pct,
                "status": status,
            }),
        );
    };

    emit_pct(10, "Đang kết nối máy chủ Live OTA (monword.vercel.app)...");
    let version_text = fetch_text_with_ps(&format!("{}/version.json", LIVE_OTA_ORIGIN), 8)
        .map_err(|e| format!("Không thể tải version.json từ {}: {}", LIVE_OTA_ORIGIN, e))?;
    let v_json: Value = serde_json::from_str(version_text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("Manifest version.json không hợp lệ: {}", e))?;
    let target_ver = v_json
        .get("version")
        .and_then(|x| x.as_str())
        .map(normalize_version)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Manifest không có số phiên bản hợp lệ.".to_string())?;

    emit_pct(30, &format!("Đang tải giao diện MonWord v{}...", target_ver));
    let html_text = fetch_text_with_ps(&format!("{}/index.html", LIVE_OTA_ORIGIN), 10)
        .map_err(|e| format!("Không thể tải index.html OTA: {}", e))?;

    let ota_root = get_ota_dist_dir();
    let staging = ota_root
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("ota-staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(staging.join("assets"))
        .map_err(|e| format!("Không thể tạo thư mục tạm Live OTA: {}", e))?;

    fs::write(staging.join("index.html"), html_text.as_bytes())
        .map_err(|e| format!("Lỗi ghi index.html: {}", e))?;
    fs::write(staging.join("version.json"), version_text.as_bytes())
        .map_err(|e| format!("Lỗi ghi version.json: {}", e))?;

    // Extract Vite hashed bundle references from index.html
    let re_asset = Regex::new(r#"(?:src|href)=["']/?(assets/[A-Za-z0-9._/-]+\.(?:js|css|wasm|mjs))["']"#)
        .expect("asset regex valid");
    let mut assets: Vec<String> = re_asset
        .captures_iter(&html_text)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();
    assets.sort();
    assets.dedup();

    let total_steps = (assets.len() + 2).max(1);
    for (idx, rel) in assets.iter().enumerate() {
        let pct = 35 + ((idx + 1) * 55 / total_steps) as u32;
        emit_pct(pct, &format!("Đang đồng bộ mô-đun ({}/{})...", idx + 1, assets.len()));
        if let Some(clean_rel) = normalize_ota_rel_path(rel) {
            let dest = staging.join(&clean_rel);
            if let Some(parent) = dest.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let url = format!("{}/{}", LIVE_OTA_ORIGIN, clean_rel);
            download_binary_with_ps(&url, &dest, 15)?;
        }
    }

    // Best-effort sync static icons
    emit_pct(94, "Đang đồng bộ biểu tượng & manifest...");
    for static_rel in ["monword-icon.png", "monword-logo.png", "manifest.webmanifest"] {
        let dest = staging.join(static_rel);
        let url = format!("{}/{}", LIVE_OTA_ORIGIN, static_rel);
        let _ = download_binary_with_ps(&url, &dest, 8);
    }

    let _ = fs::remove_dir_all(&ota_root);
    fs::rename(&staging, &ota_root)
        .map_err(|e| format!("Không thể kích hoạt gói Live OTA: {}", e))?;

    emit_pct(100, &format!("Đã cài xong Live OTA v{}!", target_ver));

    // Auto-reload WebView window shortly so user immediately runs the new bundle
    let reload_proxy = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(650));
        let _ = reload_proxy.send_event(UserEvent::EvalJs("window.location.reload();".to_string()));
    });

    Ok(json!({
        "savedPath": "Live OTA",
        "version": target_ver,
        "otaDir": ota_root.to_string_lossy(),
    }))
}

pub fn reset_ota_cache() -> Result<Value, String> {
    let ota_dir = get_ota_dist_dir();
    if ota_dir.exists() {
        fs::remove_dir_all(&ota_dir).map_err(|e| format!("Không thể xóa bộ nhớ OTA: {}", e))?;
    }
    Ok(json!({
        "reset": true,
        "effectiveVersion": EMBEDDED_APP_VERSION,
    }))
}

fn fetch_text_with_ps(url: &str, timeout_sec: u32) -> Result<String, String> {
    let temp_out = env::temp_dir().join(format!(
        "monword_ota_txt_{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let res = download_binary_with_ps(url, &temp_out, timeout_sec);
    let text = if res.is_ok() {
        fs::read_to_string(&temp_out).map_err(|e| e.to_string())
    } else {
        Err(res.err().unwrap_or_else(|| "Download error".to_string()))
    };
    let _ = fs::remove_file(&temp_out);
    text
}

fn download_binary_with_ps(url: &str, dest: &Path, timeout_sec: u32) -> Result<(), String> {
    let safe_url = url.replace('\'', "''");
    let safe_dest = dest.to_string_lossy().replace('\'', "''");
    let ps_script = format!(
        "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         $ProgressPreference = 'SilentlyContinue'; \
         Invoke-WebRequest -Uri '{}' -OutFile '{}' -TimeoutSec {} -UseBasicParsing -Headers @{{'Cache-Control'='no-cache'}}",
        safe_url, safe_dest, timeout_sec
    );
    let encoded = encode_powershell_utf16(&ps_script);
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Lỗi chạy PowerShell OTA: {}", e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Kết nối máy chủ OTA thất bại".to_string()
        } else {
            stderr
        });
    }
    Ok(())
}

fn encode_powershell_utf16(script: &str) -> String {
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.push((unit & 0xff) as u8);
        bytes.push((unit >> 8) as u8);
    }
    base64_encode(&bytes)
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn detect_mime(rel_path: &str) -> &'static str {
    let lower = rel_path.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".json") || lower.ends_with(".webmanifest") {
        "application/json; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_semver_and_normalize() {
        assert_eq!(compare_semver("1.0.0", "1.0.0"), 0);
        assert_eq!(compare_semver("v1.1.0", "1.0.9"), 1);
        assert_eq!(compare_semver("1.0.0", "1.0.1"), -1);
    }

    #[test]
    fn test_normalize_ota_rel_path_blocks_traversal() {
        assert_eq!(normalize_ota_rel_path("/"), Some("index.html".to_string()));
        assert_eq!(
            normalize_ota_rel_path("/assets/app-123.js"),
            Some("assets/app-123.js".to_string())
        );
        assert_eq!(normalize_ota_rel_path("/../secret.txt"), None);
        assert_eq!(normalize_ota_rel_path("/a\\b"), None);
        assert_eq!(normalize_ota_rel_path("/C:/Windows/win.ini"), None);
    }
}
