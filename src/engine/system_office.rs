use crate::engine::formatter;
use crate::engine::openxml_arch::OpenXmlPackage;
use crate::engine::reliability::error_code::{AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::resource_guard;
use crate::models::{AddinItem, FormatConfig, ManageAddinRequest, SystemStatus, SystemTweaksRequest};
use std::env;
use std::fs;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use windows_registry::CURRENT_USER;

const CREATE_NO_WINDOW: u32 = 0x08000000;
static COM_SEQ_COUNTER: AtomicU64 = AtomicU64::new(1);

fn detect_office_version() -> Option<String> {
    for ver in ["16.0", "15.0", "14.0"] {
        let path = format!("Software\\Microsoft\\Office\\{}\\Word", ver);
        if CURRENT_USER.open(&path).is_ok() {
            return Some(ver.to_string());
        }
    }
    None
}

fn appdata_dir() -> PathBuf {
    env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn localappdata_dir() -> PathBuf {
    env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn word_startup_dir() -> PathBuf {
    appdata_dir().join("Microsoft").join("Word").join("STARTUP")
}

pub fn excel_addins_dir() -> PathBuf {
    appdata_dir().join("Microsoft").join("AddIns")
}

pub fn user_fonts_dir() -> PathBuf {
    localappdata_dir().join("Microsoft").join("Windows").join("Fonts")
}

pub fn normal_dotm_path() -> PathBuf {
    appdata_dir()
        .join("Microsoft")
        .join("Templates")
        .join("Normal.dotm")
}

pub fn get_system_status() -> SystemStatus {
    let ver = detect_office_version();
    let ver_str = ver.as_deref().unwrap_or("16.0");

    let pv_key = format!(
        "Software\\Microsoft\\Office\\{}\\Word\\Security\\ProtectedView",
        ver_str
    );
    let word_protected_view = CURRENT_USER
        .open(&pv_key)
        .ok()
        .map(|k| {
            let att = k.get_u32("DisableAttachmentsInPV").unwrap_or(0);
            let net = k.get_u32("DisableInternetFilesInPV").unwrap_or(0);
            att == 0 && net == 0
        })
        .unwrap_or(true);

    let gen_key = format!("Software\\Microsoft\\Office\\{}\\Common\\General", ver_str);
    let word_start_screen_disabled = CURRENT_USER
        .open(&gen_key)
        .ok()
        .and_then(|k| k.get_u32("DisableBootToOfficeStart").ok())
        .map(|v| v == 1)
        .unwrap_or(false);

    let opt_key = format!("Software\\Microsoft\\Office\\{}\\Word\\Options", ver_str);
    let word_spell_check_disabled = CURRENT_USER
        .open(&opt_key)
        .ok()
        .and_then(|k| k.get_u32("NoSpellCheck").ok())
        .map(|v| v == 1)
        .unwrap_or(false);

    let sec_key = format!("Software\\Microsoft\\Office\\{}\\Word\\Security", ver_str);
    let vba_warnings_level = CURRENT_USER
        .open(&sec_key)
        .ok()
        .and_then(|k| k.get_u32("VBAWarnings").ok());

    let nd = normal_dotm_path();

    SystemStatus {
        office_version: ver,
        word_protected_view,
        word_start_screen_disabled,
        word_spell_check_disabled,
        vba_warnings_level,
        normal_dotm_path: if nd.is_file() {
            Some(nd.to_string_lossy().to_string())
        } else {
            None
        },
        word_startup_dir: word_startup_dir().to_string_lossy().to_string(),
        excel_addins_dir: excel_addins_dir().to_string_lossy().to_string(),
        user_fonts_dir: user_fonts_dir().to_string_lossy().to_string(),
    }
}

pub fn apply_system_tweaks(req: &SystemTweaksRequest) -> Result<Vec<String>, String> {
    let ver = detect_office_version().unwrap_or_else(|| "16.0".to_string());
    let mut logs = Vec::new();

    if let Some(disable_pv) = req.disable_protected_view {
        let val = if disable_pv { 1 } else { 0 };
        for app in ["Word", "Excel", "PowerPoint"] {
            let key_path = format!(
                "Software\\Microsoft\\Office\\{}\\{}\\Security\\ProtectedView",
                ver, app
            );
            if let Ok(key) = CURRENT_USER.create(&key_path) {
                let _ = key.set_u32("DisableAttachmentsInPV", val);
                let _ = key.set_u32("DisableInternetFilesInPV", val);
                let _ = key.set_u32("DisableUnsafeLocationsInPV", val);
            }
        }
        logs.push(if disable_pv {
            "Đã tắt Protected View cho Word / Excel / PowerPoint.".to_string()
        } else {
            "Đã bật lại Protected View bảo vệ cho Office.".to_string()
        });
    }

    if let Some(disable_start) = req.disable_start_screen {
        let key_path = format!("Software\\Microsoft\\Office\\{}\\Common\\General", ver);
        if let Ok(key) = CURRENT_USER.create(&key_path) {
            let _ = key.set_u32("DisableBootToOfficeStart", if disable_start { 1 } else { 0 });
        }
        logs.push(if disable_start {
            "Đã tắt màn hình Start Screen khi mở Office.".to_string()
        } else {
            "Đã bật lại màn hình Start Screen.".to_string()
        });
    }

    if let Some(disable_spell) = req.disable_spell_check {
        let val = if disable_spell { 1 } else { 0 };
        let word_opt = format!("Software\\Microsoft\\Office\\{}\\Word\\Options", ver);
        if let Ok(key) = CURRENT_USER.create(&word_opt) {
            let _ = key.set_u32("NoSpellCheck", val);
            let _ = key.set_u32("HideGrammaticalErrors", val);
            let _ = key.set_u32("HideSpellingErrors", val);
        }
        let ppt_opt = format!("Software\\Microsoft\\Office\\{}\\PowerPoint\\Options", ver);
        if let Ok(key) = CURRENT_USER.create(&ppt_opt) {
            let _ = key.set_u32("NoSpellCheck", val);
        }
        logs.push(if disable_spell {
            "Đã tắt gạch chân kiểm tra chính tả tiếng Anh trên Word/PowerPoint.".to_string()
        } else {
            "Đã bật lại kiểm tra chính tả mặc định.".to_string()
        });
    }

    if let Some(enable_vba) = req.enable_vba_macros {
        // 1 = Enable all macros (high risk), 2 = Disable with notification (safe default)
        let val = if enable_vba { 1 } else { 2 };
        for app in ["Word", "Excel"] {
            let sec_key = format!("Software\\Microsoft\\Office\\{}\\{}\\Security", ver, app);
            if let Ok(key) = CURRENT_USER.create(&sec_key) {
                let _ = key.set_u32("VBAWarnings", val);
            }
        }
        logs.push(if enable_vba {
            "CẢNH BÁO: Đã chuyển VBAWarnings=1 (Cho phép chạy Macro Word/Excel). Chỉ bật khi tin cậy file nội bộ!".to_string()
        } else {
            "Đã đưa VBAWarnings về mức an toàn (Chặn Macro có thông báo).".to_string()
        });
    }

    if req.apply_normal_dotm_nd30 == Some(true) {
        let nd_path = normal_dotm_path();
        if nd_path.is_file() {
            let bak = nd_path.with_extension("dotm.bak");
            let _ = fs::copy(&nd_path, &bak);
            let mut pkg = OpenXmlPackage::open(&nd_path)?;
            let mut fmt_logs = Vec::new();
            let cfg = FormatConfig::default();
            formatter::apply_formatting(&mut pkg, &cfg, &mut fmt_logs)?;
            pkg.save_to(&nd_path)?;
            logs.push(format!(
                "Đã cập nhật Normal.dotm theo chuẩn NĐ30 (Times New Roman 14pt, lề 2-2-3-2 cm) và sao lưu tại {:?}.",
                bak
            ));
        } else {
            return Err("Không tìm thấy tệp Normal.dotm (hãy mở Word ít nhất 1 lần trước).".to_string());
        }
    }

    Ok(logs)
}

pub fn list_addins(app: &str) -> Vec<AddinItem> {
    let is_excel = app.eq_ignore_ascii_case("excel");
    let base_dir = if is_excel {
        excel_addins_dir()
    } else {
        word_startup_dir()
    };
    let disabled_dir = base_dir.join("_Disabled");
    let allowed_exts: &[&str] = if is_excel {
        &["xlam", "xla", "xll"]
    } else {
        &["dotm", "dot", "wll"]
    };

    let mut items = Vec::new();
    collect_addins_from_dir(&base_dir, app, true, allowed_exts, &mut items);
    collect_addins_from_dir(&disabled_dir, app, false, allowed_exts, &mut items);
    items.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    items
}

fn collect_addins_from_dir(
    dir: &Path,
    app: &str,
    enabled: bool,
    allowed_exts: &[&str],
    out: &mut Vec<AddinItem>,
) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !allowed_exts.contains(&ext.as_str()) {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or(&file_name)
            .to_string();
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(AddinItem {
            app: app.to_string(),
            name,
            file_name,
            path: path.to_string_lossy().to_string(),
            enabled,
            size_bytes,
        });
    }
}

pub fn manage_addin(req: &ManageAddinRequest) -> Result<String, String> {
    let is_excel = req.app.eq_ignore_ascii_case("excel");
    let base_dir = if is_excel {
        excel_addins_dir()
    } else {
        word_startup_dir()
    };
    let disabled_dir = base_dir.join("_Disabled");

    match req.action.as_str() {
        "add" => {
            let src_str = req
                .path
                .as_deref()
                .ok_or_else(|| "Thiếu đường dẫn tệp Add-in.".to_string())?;
            let src = Path::new(src_str);
            if !src.is_file() {
                return Err("Tệp Add-in không tồn tại.".to_string());
            }
            fs::create_dir_all(&base_dir).map_err(|e| format!("Không thể tạo thư mục Add-in: {}", e))?;
            let fname = src
                .file_name()
                .ok_or_else(|| "Tên tệp không hợp lệ".to_string())?;
            let dest = base_dir.join(fname);
            fs::copy(src, &dest).map_err(|e| format!("Lỗi sao chép Add-in: {}", e))?;
            Ok(format!("Đã cài đặt Add-in '{}'.", fname.to_string_lossy()))
        }
        "toggle" => {
            let target_str = req
                .path
                .as_deref()
                .ok_or_else(|| "Thiếu đường dẫn Add-in.".to_string())?;
            let target = Path::new(target_str);
            if !target.is_file() {
                return Err("Không tìm thấy tệp Add-in.".to_string());
            }
            let fname = target
                .file_name()
                .ok_or_else(|| "Tên tệp không hợp lệ".to_string())?;
            let is_disabled = target
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n == "_Disabled")
                .unwrap_or(false);

            let dest = if is_disabled {
                fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;
                base_dir.join(fname)
            } else {
                fs::create_dir_all(&disabled_dir).map_err(|e| e.to_string())?;
                disabled_dir.join(fname)
            };
            fs::rename(target, &dest).map_err(|e| format!("Lỗi bật/tắt Add-in (có thể Word/Excel đang mở): {}", e))?;
            Ok(if is_disabled {
                format!("Đã BẬT Add-in '{}'.", fname.to_string_lossy())
            } else {
                format!("Đã TẮT Add-in '{}'.", fname.to_string_lossy())
            })
        }
        "delete" => {
            let target_str = req
                .path
                .as_deref()
                .ok_or_else(|| "Thiếu đường dẫn Add-in.".to_string())?;
            let target = Path::new(target_str);
            if target.is_file() {
                fs::remove_file(target)
                    .map_err(|e| format!("Không thể xóa Add-in (đóng Word/Excel trước): {}", e))?;
            }
            Ok("Đã xóa Add-in khỏi hệ thống.".to_string())
        }
        other => Err(format!("Hành động Add-in không hợp lệ: {}", other)),
    }
}

pub fn install_user_fonts(font_paths: &[String]) -> Result<Vec<String>, String> {
    let dest_dir = user_fonts_dir();
    fs::create_dir_all(&dest_dir).map_err(|e| format!("Không tạo được thư mục Fonts người dùng: {}", e))?;
    let font_reg = CURRENT_USER
        .create("Software\\Microsoft\\Windows NT\\CurrentVersion\\Fonts")
        .map_err(|e| format!("Không mở được Registry Fonts người dùng: {}", e))?;

    let mut installed = Vec::new();
    for raw in font_paths {
        let src = Path::new(raw);
        if !src.is_file() {
            continue;
        }
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            continue;
        }
        let Some(fname) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let dest = dest_dir.join(fname);
        fs::copy(src, &dest).map_err(|e| format!("Lỗi sao chép font {}: {}", fname, e))?;
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or(fname);
        let reg_name = format!("{} (TrueType)", stem);
        let _ = font_reg.set_string(&reg_name, dest.to_string_lossy().as_ref());
        installed.push(fname.to_string());
    }
    Ok(installed)
}

/// Optional Word COM Background Worker:
/// - Converts legacy `.doc` -> `.docx`
/// - Updates TOC/Fields and exports `.pdf` when requested
/// Protected by strict timeout, cancellation, and DCOM orphan `WINWORD.EXE` cleanup.
pub fn word_com_convert_doc_to_docx(src_doc: &Path, dest_docx: &Path) -> Result<(), String> {
    let timer = OpTimer::start("system_office", "word_com_convert_doc_to_docx", Some(src_doc));
    if let Err(e) = crate::engine::openxml_arch::inspect_ole_cfb_file(src_doc, false) {
        let code = crate::engine::reliability::error_code::classify_error_message(
            &e,
            AppErrorCode::ErrFileLockedOrConflict,
        )
        .code
        .as_str();
        timer.finish_err(code, &e);
        return Err(e);
    }
    let src_escaped = ps_escape(&src_doc.to_string_lossy());
    let dest_escaped = ps_escape(&dest_docx.to_string_lossy());

    let inner_body = format!(
        r#"
    $doc = $word.Documents.Open('{src}', $false, $true)
    $doc.SaveAs2('{dest}', 16)
"#,
        src = src_escaped,
        dest = dest_escaped
    );

    let timeout_ms = resource_guard::current_limits().word_com_timeout_ms;
    match run_word_com_script_with_timeout(&inner_body, timeout_ms, Some(&resource_guard::is_batch_cancelled)) {
        Ok(_) => {
            if !dest_docx.is_file() || fs::metadata(dest_docx).map(|m| m.len()).unwrap_or(0) == 0 {
                let err_msg = AppStructuredError::new(
                    AppErrorCode::ErrDocConvert,
                    "Word COM không tạo được tệp .docx hợp lệ sau chuyển đổi.",
                )
                .format_user_message();
                timer.finish_err("ERR_DOC_CONVERT", &err_msg);
                return Err(err_msg);
            }
            timer.finish_ok("Chuyển đổi .doc -> .docx qua Word COM thành công");
            Ok(())
        }
        Err(e) => {
            let code = crate::engine::reliability::error_code::classify_error_message(
                &e,
                AppErrorCode::ErrDocConvert,
            )
            .code
            .as_str();
            let structured = if code == "ERR_COM_TIMEOUT" || code == "ERR_TASK_CANCELLED" {
                e
            } else {
                AppStructuredError::new(AppErrorCode::ErrDocConvert, e).format_user_message()
            };
            timer.finish_err(code, &structured);
            Err(structured)
        }
    }
}

pub fn word_com_refresh_and_export_pdf(
    docx_path: &Path,
    update_toc: bool,
    pdf_dest: Option<&Path>,
) -> Result<(), String> {
    if !update_toc && pdf_dest.is_none() {
        return Ok(());
    }
    let timer = OpTimer::start("system_office", "word_com_refresh_export", Some(docx_path));
    let src_escaped = ps_escape(&docx_path.to_string_lossy());
    let update_block = if update_toc {
        "$doc.Fields.Update() | Out-Null; foreach ($t in $doc.TablesOfContents) { $t.Update() }; $doc.Save()"
    } else {
        ""
    };
    let pdf_block = if let Some(pdf) = pdf_dest {
        format!(
            "$doc.ExportAsFixedFormat('{}', 17)",
            ps_escape(&pdf.to_string_lossy())
        )
    } else {
        String::new()
    };

    let inner_body = format!(
        r#"
    $doc = $word.Documents.Open('{src}')
    {update_block}
    {pdf_block}
"#,
        src = src_escaped,
        update_block = update_block,
        pdf_block = pdf_block
    );

    let timeout_ms = resource_guard::current_limits().word_com_timeout_ms;
    match run_word_com_script_with_timeout(&inner_body, timeout_ms, Some(&resource_guard::is_batch_cancelled)) {
        Ok(_) => {
            if let Some(pdf) = pdf_dest {
                if !pdf.is_file() || fs::metadata(pdf).map(|m| m.len()).unwrap_or(0) < 100 {
                    let msg = AppStructuredError::new(
                        AppErrorCode::ErrDocConvert,
                        "Word COM kết thúc nhưng không tạo ra tệp PDF đầu ra hợp lệ.",
                    )
                    .format_user_message();
                    timer.finish_err("ERR_DOC_CONVERT", &msg);
                    return Err(msg);
                }
            }
            timer.finish_ok("Cập nhật mục lục / xuất PDF qua Word COM thành công");
            Ok(())
        }
        Err(e) => {
            let code = crate::engine::reliability::error_code::classify_error_message(
                &e,
                AppErrorCode::ErrDocConvert,
            )
            .code
            .as_str();
            let structured = if code == "ERR_COM_TIMEOUT" || code == "ERR_TASK_CANCELLED" {
                e
            } else {
                AppStructuredError::new(
                    if pdf_dest.is_some() {
                        AppErrorCode::ErrDocConvert
                    } else {
                        AppErrorCode::ErrWordCom
                    },
                    e,
                )
                .format_user_message()
            };
            timer.finish_err(code, &structured);
            Err(structured)
        }
    }
}

pub fn run_word_com_script_with_timeout(
    try_body: &str,
    timeout_ms: u64,
    cancel_check: Option<&dyn Fn() -> bool>,
) -> Result<std::process::Output, String> {
    let seq = COM_SEQ_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid_marker = env::temp_dir().join(format!(
        "monword_com_pid_{}_{}.txt",
        std::process::id(),
        seq
    ));
    let _ = fs::remove_file(&pid_marker);
    let marker_esc = ps_escape(&pid_marker.to_string_lossy());

    let wrapped_script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
$doc = $null
try {{
    $beforeWinWord = @(Get-Process WINWORD -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
    $word = New-Object -ComObject Word.Application
    $afterWinWord = @(Get-Process WINWORD -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
    $newPids = @($afterWinWord | Where-Object {{ $beforeWinWord -notcontains $_ }})
    if ($newPids.Count -gt 0) {{
        [System.IO.File]::WriteAllText('{marker}', ($newPids -join ','))
    }}
    $word.Visible = $false
    $word.DisplayAlerts = 0
    $word.AutomationSecurity = 3
    {body}
}} finally {{
    if ($doc -ne $null) {{ try {{ $doc.Close($false) }} catch {{}}; [System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($doc) | Out-Null }}
    if ($word -ne $null) {{ try {{ $word.Quit() }} catch {{}}; [System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) | Out-Null }}
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
}}
"#,
        marker = marker_esc,
        body = try_body
    );

    let res = run_hidden_powershell_with_timeout(
        &wrapped_script,
        timeout_ms,
        cancel_check,
        Some(&pid_marker),
    );
    let _ = fs::remove_file(&pid_marker);
    res
}

pub fn run_hidden_powershell_with_timeout(
    script: &str,
    timeout_ms: u64,
    cancel_check: Option<&dyn Fn() -> bool>,
    pid_marker: Option<&Path>,
) -> Result<std::process::Output, String> {
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| {
            AppStructuredError::new(
                AppErrorCode::ErrWordCom,
                format!("Không thể khởi chạy tiến trình PowerShell / COM Worker: {}", e),
            )
            .format_user_message()
        })?;

    let child_pid = child.id();
    let stdout_handle = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let started = Instant::now();
    let effective_timeout = timeout_ms.max(150);

    loop {
        let cancelled = cancel_check.map(|f| f()).unwrap_or(false);
        if cancelled {
            let _ = child.kill();
            kill_powershell_and_com_pids(child_pid, pid_marker);
            let _ = child.wait();
            return Err(AppStructuredError::new(
                AppErrorCode::ErrTaskCancelled,
                "Tác vụ Microsoft Word COM đã được hủy và dọn sạch tiến trình con.",
            )
            .format_user_message());
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_handle
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                let stderr = stderr_handle
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                if !status.success() {
                    let err_txt = String::from_utf8_lossy(&stderr).trim().to_string();
                    return Err(AppStructuredError::new(
                        AppErrorCode::ErrWordCom,
                        format!(
                            "Word COM Worker trả lỗi: {}",
                            if err_txt.is_empty() {
                                "Exit code khác 0".to_string()
                            } else {
                                err_txt
                            }
                        ),
                    )
                    .format_user_message());
                }
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if (started.elapsed().as_millis() as u64) >= effective_timeout {
                    let _ = child.kill();
                    kill_powershell_and_com_pids(child_pid, pid_marker);
                    let _ = child.wait();
                    return Err(AppStructuredError::new(
                        AppErrorCode::ErrComTimeout,
                        format!(
                            "Tiến trình Microsoft Word COM vượt thời gian tối đa {} ms và đã bị buộc dừng để tránh treo ứng dụng.",
                            effective_timeout
                        ),
                    )
                    .format_user_message());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                kill_powershell_and_com_pids(child_pid, pid_marker);
                let _ = child.wait();
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrWordCom,
                    format!("Lỗi theo dõi tiến trình Word COM: {}", e),
                )
                .format_user_message());
            }
        }
    }
}

fn kill_powershell_and_com_pids(powershell_pid: u32, pid_marker: Option<&Path>) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &powershell_pid.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Some(marker) = pid_marker {
        if let Ok(raw) = fs::read_to_string(marker) {
            for pid_str in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if pid_str.chars().all(|c| c.is_ascii_digit()) {
                    let _ = Command::new("taskkill")
                        .args(["/F", "/PID", pid_str])
                        .creation_flags(CREATE_NO_WINDOW)
                        .output();
                }
            }
        }
    }
}

fn ps_escape(s: &str) -> String {
    s.replace('\'', "''")
}
