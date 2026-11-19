use crate::engine::{self, system_office};
use crate::models::{
    get_all_presets, ManageAddinRequest, ProcessConfig, SystemTweaksRequest,
};
use crate::ota;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use tao::event_loop::EventLoopProxy;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

#[derive(Debug, Clone)]
pub enum UserEvent {
    EvalJs(String),
    ShowNativeDialog {
        id: String,
        kind: DialogKind,
    },
}

#[derive(Debug, Clone)]
pub enum DialogKind {
    PickOfficeFiles,
    PickFolder,
    PickAddinFile(String),
    PickFontFiles,
}

#[derive(Debug, Deserialize)]
pub struct IpcRequest {
    pub id: String,
    pub cmd: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcResponse {
    pub id: String,
    pub ok: bool,
    pub data: Value,
    pub error: Option<String>,
}

pub fn emit_js_event(proxy: &EventLoopProxy<UserEvent>, event_name: &str, data: Value) {
    let payload_json = serde_json::to_string(&data).unwrap_or_else(|_| "null".to_string());
    let script = format!(
        "window.__APP_BRIDGE__ && window.__APP_BRIDGE__.onEvent({:?}, {});",
        event_name, payload_json
    );
    let _ = proxy.send_event(UserEvent::EvalJs(script));
}

pub fn send_ipc_reply(proxy: &EventLoopProxy<UserEvent>, resp: IpcResponse) {
    let resp_json = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
    let script = format!(
        "window.__APP_BRIDGE__ && window.__APP_BRIDGE__.onResponse({});",
        resp_json
    );
    let _ = proxy.send_event(UserEvent::EvalJs(script));
}

pub fn handle_ipc_message(body: String, proxy: EventLoopProxy<UserEvent>) {
    let req: IpcRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            let _ = e;
            return;
        }
    };

    // Route native file/folder dialogs onto main UI thread (with HWND owner)
    match req.cmd.as_str() {
        "pick_files" => {
            let _ = proxy.send_event(UserEvent::ShowNativeDialog {
                id: req.id,
                kind: DialogKind::PickOfficeFiles,
            });
            return;
        }
        "pick_folder" => {
            let _ = proxy.send_event(UserEvent::ShowNativeDialog {
                id: req.id,
                kind: DialogKind::PickFolder,
            });
            return;
        }
        "pick_addin_file" => {
            let app = req
                .payload
                .get("app")
                .and_then(|v| v.as_str())
                .unwrap_or("word")
                .to_string();
            let _ = proxy.send_event(UserEvent::ShowNativeDialog {
                id: req.id,
                kind: DialogKind::PickAddinFile(app),
            });
            return;
        }
        "pick_font_files" => {
            let _ = proxy.send_event(UserEvent::ShowNativeDialog {
                id: req.id,
                kind: DialogKind::PickFontFiles,
            });
            return;
        }
        _ => {}
    }

    // Run CPU / IO / Office work off the UI event loop thread
    std::thread::spawn(move || {
        let reply = dispatch_background_command(&req, &proxy);
        send_ipc_reply(&proxy, reply);
    });
}

fn dispatch_background_command(req: &IpcRequest, proxy: &EventLoopProxy<UserEvent>) -> IpcResponse {
    let id = req.id.clone();
    let ok = |data: Value| IpcResponse {
        id: id.clone(),
        ok: true,
        data,
        error: None,
    };
    let err = |msg: String| IpcResponse {
        id: id.clone(),
        ok: false,
        data: Value::Null,
        error: Some(msg),
    };

    match req.cmd.as_str() {
        "get_presets" => {
            let presets = get_all_presets();
            ok(serde_json::to_value(presets).unwrap_or(Value::Null))
        }
        "scan_paths" => {
            let paths: Vec<String> = req
                .payload
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let items = engine::scan_paths(&paths);
            ok(serde_json::to_value(items).unwrap_or(Value::Null))
        }
        "audit_files" => {
            let paths: Vec<String> = req
                .payload
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let mut reports = Vec::new();
            for (idx, p) in paths.iter().enumerate() {
                emit_js_event(
                    proxy,
                    "progress",
                    serde_json::json!({
                        "phase": "audit",
                        "current": idx + 1,
                        "total": paths.len(),
                        "file": p,
                    }),
                );
                match engine::audit_single_file(Path::new(p)) {
                    Ok(r) => reports.push(r),
                    Err(e) => {
                        return err(format!("Lỗi kiểm tra '{}': {}", p, e));
                    }
                }
            }
            ok(serde_json::to_value(reports).unwrap_or(Value::Null))
        }
        "process_files" => {
            let paths: Vec<String> = req
                .payload
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let config: ProcessConfig = match req
                .payload
                .get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
            {
                Some(c) => c,
                None => return err("Cấu hình xử lý không hợp lệ.".to_string()),
            };

            let mut results = Vec::with_capacity(paths.len());
            for (idx, p) in paths.iter().enumerate() {
                emit_js_event(
                    proxy,
                    "progress",
                    serde_json::json!({
                        "phase": "process",
                        "current": idx + 1,
                        "total": paths.len(),
                        "file": p,
                    }),
                );
                let res = engine::process_single_file(Path::new(p), &config);
                results.push(res);
            }
            ok(serde_json::to_value(results).unwrap_or(Value::Null))
        }
        "restore_backup" => {
            let orig = req
                .payload
                .get("originalPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let bak = req
                .payload
                .get("backupPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match engine::restore_backup(Path::new(orig), Path::new(bak)) {
                Ok(msg) => ok(serde_json::json!({ "message": msg })),
                Err(e) => err(e),
            }
        }
        "open_path" => {
            let path = req
                .payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reveal = req
                .payload
                .get("reveal")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if path.is_empty() {
                return err("Đường dẫn trống.".to_string());
            }
            let res = if reveal {
                Command::new("explorer.exe")
                    .arg(format!("/select,{}", path))
                    .spawn()
            } else {
                Command::new("cmd")
                    .args(["/C", "start", "", path])
                    .spawn()
            };
            match res {
                Ok(_) => ok(serde_json::json!({ "opened": true })),
                Err(e) => err(format!("Không thể mở đường dẫn: {}", e)),
            }
        }
        "get_system_status" => {
            let status = system_office::get_system_status();
            ok(serde_json::to_value(status).unwrap_or(Value::Null))
        }
        "apply_system_tweaks" => {
            let tweaks: SystemTweaksRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(t) => t,
                Err(e) => return err(format!("Dữ liệu cấu hình hệ thống không hợp lệ: {}", e)),
            };
            match system_office::apply_system_tweaks(&tweaks) {
                Ok(logs) => ok(serde_json::json!({
                    "logs": logs,
                    "status": system_office::get_system_status(),
                })),
                Err(e) => err(e),
            }
        }
        "list_addins" => {
            let app = req
                .payload
                .get("app")
                .and_then(|v| v.as_str())
                .unwrap_or("word");
            let items = system_office::list_addins(app);
            ok(serde_json::to_value(items).unwrap_or(Value::Null))
        }
        "manage_addin" => {
            let manage_req: ManageAddinRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu quản lý Add-in không hợp lệ: {}", e)),
            };
            let app = manage_req.app.clone();
            match system_office::manage_addin(&manage_req) {
                Ok(msg) => ok(serde_json::json!({
                    "message": msg,
                    "items": system_office::list_addins(&app),
                })),
                Err(e) => err(e),
            }
        }
        "install_fonts" => {
            let paths: Vec<String> = req
                .payload
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            match system_office::install_user_fonts(&paths) {
                Ok(installed) => ok(serde_json::json!({ "installed": installed })),
                Err(e) => err(e),
            }
        }
        "get_app_info" => ok(ota::get_app_info()),
        "check_ota_update" => ok(ota::check_remote_ota()),
        "apply_live_ota" => match ota::apply_desktop_live_ota(proxy) {
            Ok(data) => ok(data),
            Err(e) => err(e),
        },
        "reset_ota_cache" => match ota::reset_ota_cache() {
            Ok(data) => ok(data),
            Err(e) => err(e),
        },
        "open_external_url" => {
            let url = req
                .payload
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return err("URL không hợp lệ.".to_string());
            }
            match Command::new("cmd").args(["/C", "start", "", url]).spawn() {
                Ok(_) => ok(serde_json::json!({ "opened": true })),
                Err(e) => err(format!("Không thể mở liên kết: {}", e)),
            }
        }
        other => err(format!("Lệnh IPC chưa hỗ trợ: {}", other)),
    }
}

pub fn run_native_dialog(owner_hwnd: isize, kind: DialogKind) -> Result<Vec<String>, String> {
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let did_init = init.is_ok();

        let result = (|| -> Result<Vec<String>, String> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL)
                    .map_err(|e| format!("Không thể mở hộp thoại Windows: {}", e))?;

            let mut opts = dialog.GetOptions().unwrap_or_default();
            match &kind {
                DialogKind::PickFolder => {
                    opts |= FOS_PICKFOLDERS | FOS_PATHMUSTEXIST;
                    dialog.SetTitle(w!("Chọn thư mục chứa văn bản Office")).ok();
                }
                DialogKind::PickOfficeFiles => {
                    opts |= FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST;
                    let filters = [
                        COMDLG_FILTERSPEC {
                            pszName: w!("Tài liệu Office (*.docx;*.docm;*.doc;*.xlsx;*.xlsm)"),
                            pszSpec: w!("*.docx;*.docm;*.doc;*.xlsx;*.xlsm"),
                        },
                        COMDLG_FILTERSPEC {
                            pszName: w!("Tất cả tệp (*.*)"),
                            pszSpec: w!("*.*"),
                        },
                    ];
                    let _ = dialog.SetFileTypes(&filters);
                    dialog.SetTitle(w!("Chọn tệp Word / Excel cần chuẩn hóa")).ok();
                }
                DialogKind::PickAddinFile(app) => {
                    opts |= FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST;
                    let spec: PCWSTR = if app.eq_ignore_ascii_case("excel") {
                        w!("*.xlam;*.xla;*.xll")
                    } else {
                        w!("*.dotm;*.dot;*.wll")
                    };
                    let filters = [COMDLG_FILTERSPEC {
                        pszName: w!("Office Add-in"),
                        pszSpec: spec,
                    }];
                    let _ = dialog.SetFileTypes(&filters);
                    dialog.SetTitle(w!("Chọn tệp Add-in để cài đặt")).ok();
                }
                DialogKind::PickFontFiles => {
                    opts |= FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST;
                    let filters = [COMDLG_FILTERSPEC {
                        pszName: w!("Font chữ (*.ttf;*.otf;*.ttc)"),
                        pszSpec: w!("*.ttf;*.otf;*.ttc"),
                    }];
                    let _ = dialog.SetFileTypes(&filters);
                    dialog.SetTitle(w!("Chọn các tệp Font chữ cần cài đặt")).ok();
                }
            }

            let _ = dialog.SetOptions(opts);
            let hwnd = if owner_hwnd != 0 {
                Some(HWND(owner_hwnd as *mut core::ffi::c_void))
            } else {
                None
            };

            if dialog.Show(hwnd).is_err() {
                // User cancelled dialog
                return Ok(Vec::new());
            }

            let mut selected = Vec::new();
            let results = dialog
                .GetResults()
                .map_err(|e| format!("Không thể đọc danh sách tệp đã chọn: {}", e))?;
            let count = results.GetCount().unwrap_or(0);
            for i in 0..count {
                if let Ok(item) = results.GetItemAt(i) {
                    if let Ok(pwstr) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                        if let Ok(s) = pwstr.to_string() {
                            selected.push(s);
                        }
                        CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                    }
                }
            }
            Ok(selected)
        })();

        if did_init {
            CoUninitialize();
        }
        result
    }
}
