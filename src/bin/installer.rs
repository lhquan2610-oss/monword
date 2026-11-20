#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../installer_core.rs"]
mod installer_core;

use installer_core::{
    default_install_dir, default_user_appdata_dir, pack_self_extracting_installer, run_install_from_sfx,
    run_uninstall, InstallOptions, APP_VERSION, PRODUCT_NAME, PUBLISHER_NAME,
};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: isize, lp_text: *const u16, lp_caption: *const u16, u_type: u32) -> i32;
}

const MB_OK: u32 = 0x0000_0000;
const MB_YESNO: u32 = 0x0000_0004;
const MB_ICONERROR: u32 = 0x0000_0010;
const MB_ICONQUESTION: u32 = 0x0000_0020;
const MB_ICONINFORMATION: u32 = 0x0000_0040;
const IDYES: i32 = 6;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn show_msg(title: &str, body: &str, flags: u32) -> i32 {
    let w_title = to_wide(title);
    let w_body = to_wide(body);
    unsafe { MessageBoxW(0, w_body.as_ptr(), w_title.as_ptr(), flags) }
}

fn write_output(out_path: Option<&str>, text: &str) {
    let _ = std::io::stdout().write_all(text.as_bytes());
    let _ = std::io::stdout().flush();
    if let Some(p) = out_path {
        let _ = std::fs::write(p, text.as_bytes());
    }
}

fn arg_value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == key)
        .map(|w| w[1].as_str())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_file = arg_value(&args, "--out");

    // 1. Pack mode for build-portable.ps1:
    // monword-installer.exe --pack-sfx --stub <exe> --dir <payload_dir> --out <setup.exe>
    if args.iter().any(|a| a == "--pack-sfx") {
        let stub = match arg_value(&args, "--stub") {
            Some(s) => PathBuf::from(s),
            None => {
                eprintln!("Missing --stub <path>");
                std::process::exit(1);
            }
        };
        let payload_dir = match arg_value(&args, "--dir") {
            Some(s) => PathBuf::from(s),
            None => {
                eprintln!("Missing --dir <payload_dir>");
                std::process::exit(1);
            }
        };
        let out_setup = match arg_value(&args, "--out") {
            Some(s) => PathBuf::from(s),
            None => {
                eprintln!("Missing --out <setup.exe>");
                std::process::exit(1);
            }
        };
        let candidates: Vec<(&str, PathBuf)> = vec![
            ("MonWord.exe", payload_dir.join("MonWord.exe")),
            ("WebView2Loader.dll", payload_dir.join("WebView2Loader.dll")),
            ("monword.ico", payload_dir.join("monword.ico")),
            ("README.md", payload_dir.join("README.md")),
            (
                "docs/HUONG_DAN_CAI_DAT_SU_DUNG.md",
                payload_dir.join("docs").join("HUONG_DAN_CAI_DAT_SU_DUNG.md"),
            ),
            (
                "docs/HUONG_DAN_SAO_LUU_KHOI_PHUC.md",
                payload_dir.join("docs").join("HUONG_DAN_SAO_LUU_KHOI_PHUC.md"),
            ),
            (
                "docs/RELEASE_NOTES.md",
                payload_dir.join("docs").join("RELEASE_NOTES.md"),
            ),
            (
                "docs/KNOWN_LIMITATIONS.md",
                payload_dir.join("docs").join("KNOWN_LIMITATIONS.md"),
            ),
        ];
        let pairs: Vec<(&str, &std::path::Path)> = candidates
            .iter()
            .filter(|(_, p)| p.is_file())
            .map(|(rel, p)| (*rel, p.as_path()))
            .collect();
        match pack_self_extracting_installer(&stub, &pairs, &out_setup) {
            Ok(bytes) => {
                println!(
                    "PACKED_SFX_OK {} ({} bytes, {} entries)",
                    out_setup.display(),
                    bytes,
                    pairs.len()
                );
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("PACKED_SFX_ERR: {}", e);
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--version" || a == "-v") {
        write_output(
            out_file,
            &format!("MonWord Setup v{} ({})\n", APP_VERSION, PUBLISHER_NAME),
        );
        std::process::exit(0);
    }

    let silent = args.iter().any(|a| a == "--silent");
    let no_shortcuts = args.iter().any(|a| a == "--no-shortcuts");
    let no_registry = args.iter().any(|a| a == "--no-registry");
    let self_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            if !silent {
                show_msg(PRODUCT_NAME, &format!("Không xác định được tệp thực thi: {}", e), MB_OK | MB_ICONERROR);
            }
            std::process::exit(1);
        }
    };
    let is_uninstall = args.iter().any(|a| a == "--uninstall")
        || self_exe
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_ascii_lowercase().starts_with("uninstall"))
            .unwrap_or(false);

    let install_dir = arg_value(&args, "--dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if is_uninstall {
                self_exe
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(default_install_dir)
            } else {
                default_install_dir()
            }
        });
    let appdata_dir = arg_value(&args, "--appdata-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_user_appdata_dir);

    let opts = InstallOptions {
        install_dir: install_dir.clone(),
        appdata_dir,
        create_shortcuts: !no_shortcuts,
        write_registry: !no_registry,
    };

    if is_uninstall {
        if !silent {
            let prompt = format!(
                "Bạn có chắc chắn muốn gỡ cài đặt {} khỏi:\n{}\n\nLưu ý: Hồ sơ cơ quan, mẫu văn bản tùy chỉnh và mọi bản sao lưu (.chuanhoa_backup) của bạn sẽ được GIỮ NGUYÊN.",
                PRODUCT_NAME,
                install_dir.display()
            );
            if show_msg("Gỡ cài đặt MonWord", &prompt, MB_YESNO | MB_ICONQUESTION) != IDYES {
                std::process::exit(0);
            }
        }
        match run_uninstall(&opts) {
            Ok(out) => {
                let msg = format!(
                    "UNINSTALL_OK dir={} removed={} user_data_preserved={}\n",
                    out.install_dir.display(),
                    out.removed_files.len(),
                    out.user_data_preserved
                );
                write_output(out_file, &msg);
                if !silent {
                    show_msg(
                        "Gỡ cài đặt hoàn tất",
                        &format!(
                            "Đã gỡ {} thành phần chương trình tại:\n{}\n\nToàn bộ hồ sơ và bản sao lưu của bạn vẫn an toàn.",
                            out.removed_files.len(),
                            out.install_dir.display()
                        ),
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
                std::process::exit(0);
            }
            Err(e) => {
                write_output(out_file, &format!("UNINSTALL_ERR: {}\n", e));
                if !silent {
                    show_msg("Lỗi gỡ cài đặt", &e, MB_OK | MB_ICONERROR);
                }
                std::process::exit(1);
            }
        }
    }

    if !silent {
        let prompt = format!(
            "Trình cài đặt {} v{}\nNhà phát hành: {}\n\nThư mục cài đặt:\n{}\n\nKhi nâng cấp từ phiên bản cũ, toàn bộ hồ sơ tổ chức, mẫu tùy chỉnh và sao lưu (.chuanhoa_backup) đều được giữ nguyên.\n\nBạn có muốn tiếp tục cài đặt?",
            PRODUCT_NAME,
            APP_VERSION,
            PUBLISHER_NAME,
            install_dir.display()
        );
        if show_msg("Cài đặt MonWord v4.0.0 (Windows x64)", &prompt, MB_YESNO | MB_ICONQUESTION) != IDYES {
            std::process::exit(0);
        }
    }

    match run_install_from_sfx(&self_exe, &opts) {
        Ok(res) => {
            let msg = format!(
                "INSTALL_OK version={} upgraded={} prev={:?} dir={} files={}\n",
                res.version,
                res.upgraded,
                res.previous_version,
                res.install_dir.display(),
                res.installed_files.len()
            );
            write_output(out_file, &msg);
            if !silent {
                let launch = show_msg(
                    "Cài đặt MonWord hoàn tất",
                    &format!(
                        "Đã cài đặt thành công MonWord v{} ({} tệp) tại:\n{}\n\nBạn có muốn khởi chạy MonWord ngay bây giờ không?",
                        res.version,
                        res.installed_files.len(),
                        res.install_dir.display()
                    ),
                    MB_YESNO | MB_ICONINFORMATION,
                );
                if launch == IDYES {
                    let _ = Command::new(res.install_dir.join("MonWord.exe")).spawn();
                }
            }
            std::process::exit(0);
        }
        Err(e) => {
            write_output(out_file, &format!("INSTALL_ERR: {}\n", e));
            if !silent {
                show_msg("Lỗi cài đặt MonWord", &e, MB_OK | MB_ICONERROR);
            }
            std::process::exit(1);
        }
    }
}
