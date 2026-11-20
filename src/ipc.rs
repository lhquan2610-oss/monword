use crate::engine::{
    self, compliance_audit, header_builder, layout_intelligence, live_preview, org_profile,
    reliability::{
        backup_manager,
        error_code::{AppErrorCode, AppStructuredError},
        logger::{self, record_op},
        resource_guard, safe_save,
    },
    std_engine, system_office, template_engine,
};
use crate::models::{
    get_all_presets, AuditExportFormat, ComplianceAuditReport, ComplianceAuditRequest,
    ComplianceAutoFixAndRecheckRequest, ComplianceRulePack, HeaderActionMode, HeaderBuilderConfig,
    ImportDocxTemplateRequest, LayoutExecutionRequest, ManageAddinRequest, OrganizationProfile,
    PreviewBuildRequest, ProcessConfig, SaveTemplateRequest, StdDocScope, StdExecutionRequest,
    SystemTweaksRequest, TemplateApplyRequest,
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
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, FOS_ALLOWMULTISELECT,
    FOS_FILEMUSTEXIST, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
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
    PickTemplateDocx,
    SaveDocxFile(String),
    SaveAuditExport { default_name: String, ext: String },
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

pub fn validate_safe_filesystem_path(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().any(|c| c < ' ')
        || trimmed.starts_with(r"\\.\")
        || trimmed.starts_with(r"\\?\")
    {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrIpcInvalid,
            "Đường dẫn hệ thống không hợp lệ hoặc chứa ký tự điều khiển bị cấm.",
        )
        .format_user_message());
    }
    let last_seg = trimmed
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let stem = last_seg.split('.').next().unwrap_or("");
    if matches!(
        stem,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrIpcInvalid,
            format!("Tên thiết bị Windows '{}' bị từ chối vì lý do bảo mật.", trimmed),
        )
        .format_user_message());
    }
    Ok(())
}

pub fn validate_safe_external_url(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.len() > 2048
        || !(lower.starts_with("https://") || lower.starts_with("http://"))
        || trimmed
            .chars()
            .any(|c| c.is_whitespace() || c < ' ' || matches!(c, '"' | '\'' | '|' | '<' | '>' | '^' | '`'))
    {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrIpcInvalid,
            "URL liên kết ngoài không hợp lệ hoặc chứa ký tự không an toàn.",
        )
        .format_user_message());
    }
    Ok(())
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
        "pick_template_docx" => {
            let _ = proxy.send_event(UserEvent::ShowNativeDialog {
                id: req.id,
                kind: DialogKind::PickTemplateDocx,
            });
            return;
        }
        "pick_save_docx" => {
            let default_name = req
                .payload
                .get("defaultName")
                .and_then(|v| v.as_str())
                .unwrap_or("VanBan_ChuanHoa.docx")
                .to_string();
            let _ = proxy.send_event(UserEvent::ShowNativeDialog {
                id: req.id,
                kind: DialogKind::SaveDocxFile(default_name),
            });
            return;
        }
        "pick_save_audit_export" => {
            let default_name = req
                .payload
                .get("defaultName")
                .and_then(|v| v.as_str())
                .unwrap_or("BaoCaoKiemToan_MonWord.docx")
                .to_string();
            let ext = req
                .payload
                .get("ext")
                .and_then(|v| v.as_str())
                .unwrap_or("docx")
                .to_ascii_lowercase();
            let _ = proxy.send_event(UserEvent::ShowNativeDialog {
                id: req.id,
                kind: DialogKind::SaveAuditExport { default_name, ext },
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
            resource_guard::reset_batch_cancel();
            let paths: Vec<String> = req
                .payload
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let mut reports = Vec::new();
            for (idx, p) in paths.iter().enumerate() {
                if resource_guard::is_batch_cancelled() {
                    let msg = AppStructuredError::new(
                        AppErrorCode::ErrTaskCancelled,
                        "Đã dừng quét lỗi hàng đợi theo yêu cầu.",
                    )
                    .format_user_message();
                    record_op("ipc", "audit_files", Some(Path::new(p)), "cancelled", 0, Some("ERR_TASK_CANCELLED"), &msg);
                    return err(msg);
                }
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
                        if paths.len() <= 1 {
                            return err(format!("Lỗi kiểm tra '{}': {}", p, e));
                        }
                        let file_name = Path::new(p)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let file_type = Path::new(p)
                            .extension()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_ascii_lowercase();
                        reports.push(crate::models::AuditReport {
                            file_path: p.clone(),
                            file_name,
                            file_type,
                            score: 0,
                            section_count: 0,
                            table_count: 0,
                            paragraph_count: 0,
                            has_macros: false,
                            is_protected: false,
                            has_legacy_encoding: false,
                            issues: vec![crate::models::AuditIssue {
                                category: "Không thể đọc tệp".to_string(),
                                severity: crate::models::AuditSeverity::Error,
                                message: e,
                                count: 1,
                            }],
                        });
                    }
                }
            }
            ok(serde_json::to_value(reports).unwrap_or(Value::Null))
        }
        "process_files" => {
            resource_guard::reset_batch_cancel();
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
                None => {
                    return err(AppStructuredError::new(
                        AppErrorCode::ErrIpcInvalid,
                        "Cấu hình xử lý không hợp lệ.",
                    )
                    .format_user_message())
                }
            };

            let mut results = Vec::with_capacity(paths.len());
            for (idx, p) in paths.iter().enumerate() {
                if resource_guard::is_batch_cancelled() {
                    let cancel_msg = AppStructuredError::new(
                        AppErrorCode::ErrTaskCancelled,
                        "Tác vụ bị dừng theo yêu cầu trước khi xử lý tệp này.",
                    )
                    .format_user_message();
                    record_op("ipc", "process_files", Some(Path::new(p)), "cancelled", 0, Some("ERR_TASK_CANCELLED"), &cancel_msg);
                    results.push(crate::models::FileProcessResult {
                        input_path: p.clone(),
                        output_path: String::new(),
                        backup_path: None,
                        success: false,
                        stats_summary: String::new(),
                        logs: vec![cancel_msg.clone()],
                        error: Some(cancel_msg),
                        error_code: Some("ERR_TASK_CANCELLED".to_string()),
                        audit_before: None,
                        audit_after: None,
                    });
                    continue;
                }
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
            let orig_p = Path::new(orig);
            let res = if bak == "__initial__" {
                backup_manager::restore_initial_baseline(orig_p)
            } else if bak == "__latest__" || bak.trim().is_empty() {
                backup_manager::restore_latest_pre_edit(orig_p)
            } else {
                engine::restore_backup(orig_p, Path::new(bak))
            };
            match res {
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
            if let Err(e) = validate_safe_filesystem_path(path) {
                return err(e);
            }
            let target = Path::new(path);
            if !target.exists() {
                return err(AppStructuredError::new(
                    AppErrorCode::ErrFileRead,
                    format!("Đường dẫn không tồn tại: {}", path),
                )
                .format_user_message());
            }
            if !reveal {
                let ext = target
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    ext.as_str(),
                    "exe" | "bat" | "cmd" | "ps1" | "vbs" | "js" | "jse" | "wsf" | "wsh" | "msi" | "scr" | "com" | "hta" | "pif"
                ) {
                    return err(AppStructuredError::new(
                        AppErrorCode::ErrIpcInvalid,
                        format!("Từ chối chạy trực tiếp tệp thực thi/script (.{}). Hãy dùng chế độ Mở thư mục chứa tệp.", ext),
                    )
                    .format_user_message());
                }
            }
            let res = if reveal {
                Command::new("explorer.exe")
                    .arg(format!("/select,{}", path))
                    .spawn()
            } else {
                Command::new("explorer.exe").arg(path).spawn()
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
            if let Err(e) = validate_safe_external_url(url) {
                return err(e);
            }
            match Command::new("rundll32.exe")
                .args(["url.dll,FileProtocolHandler", url])
                .spawn()
            {
                Ok(_) => ok(serde_json::json!({ "opened": true })),
                Err(e) => err(format!("Không thể mở liên kết: {}", e)),
            }
        }
        "list_org_profiles" => {
            let profiles = org_profile::load_org_profiles();
            ok(serde_json::to_value(profiles).unwrap_or(Value::Null))
        }
        "save_org_profile" => {
            let profile: OrganizationProfile = match serde_json::from_value(req.payload.clone()) {
                Ok(p) => p,
                Err(e) => return err(format!("Dữ liệu hồ sơ tổ chức không hợp lệ: {}", e)),
            };
            match org_profile::upsert_org_profile(profile) {
                Ok(list) => ok(serde_json::to_value(list).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "delete_org_profile" => {
            let id = req.payload.get("id").and_then(|v| v.as_str()).unwrap_or("");
            match org_profile::delete_org_profile(id) {
                Ok(list) => ok(serde_json::to_value(list).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "export_org_profiles" => match org_profile::export_org_profiles_json() {
            Ok(json_str) => ok(serde_json::json!({ "json": json_str })),
            Err(e) => err(e),
        },
        "import_org_profiles" => {
            let raw = req.payload.get("json").and_then(|v| v.as_str()).unwrap_or("");
            match org_profile::import_org_profiles_json(raw) {
                Ok(list) => ok(serde_json::to_value(list).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "get_header_rules" => {
            let rules = header_builder::get_header_rules_catalog();
            ok(serde_json::to_value(rules).unwrap_or(Value::Null))
        }
        "inspect_doc_header" => {
            let path = req.payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let cfg: HeaderBuilderConfig = req
                .payload
                .get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let org_opt: Option<OrganizationProfile> = req
                .payload
                .get("orgProfile")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            match header_builder::inspect_file_header(Path::new(path), &cfg, org_opt.as_ref()) {
                Ok(ins) => ok(serde_json::to_value(ins).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "apply_doc_header" => {
            let paths: Vec<String> = req
                .payload
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let cfg: HeaderBuilderConfig = match req
                .payload
                .get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
            {
                Some(c) => c,
                None => return err("Cấu hình phần đầu văn bản không hợp lệ.".to_string()),
            };
            let mode: HeaderActionMode = req
                .payload
                .get("mode")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(HeaderActionMode::StandardizeExisting);
            let org_opt: Option<OrganizationProfile> = req
                .payload
                .get("orgProfile")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
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
                results.push(header_builder::apply_header_builder_to_file(
                    Path::new(p),
                    &cfg,
                    mode,
                    org_opt.as_ref(),
                ));
            }
            ok(serde_json::to_value(results).unwrap_or(Value::Null))
        }
        "create_new_header_docx" => {
            let dest_path = req
                .payload
                .get("destPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if dest_path.trim().is_empty() {
                return err("Thiếu đường dẫn tệp đầu ra.".to_string());
            }
            let cfg: HeaderBuilderConfig = req
                .payload
                .get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let org_opt: Option<OrganizationProfile> = req
                .payload
                .get("orgProfile")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let res = header_builder::create_standalone_header_docx(
                Path::new(dest_path),
                &cfg,
                org_opt.as_ref(),
            );
            ok(serde_json::to_value(res).unwrap_or(Value::Null))
        }
        "restore_header_initial" => {
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
            match header_builder::restore_initial_header_backup(Path::new(orig), Path::new(bak)) {
                Ok(message) => ok(serde_json::json!({ "message": message })),
                Err(e) => err(e),
            }
        }
        "get_std_rules" => {
            let rules = std_engine::get_std_rules_catalog();
            ok(serde_json::to_value(rules).unwrap_or(Value::Null))
        }
        "analyze_std_document" => {
            let path = req.payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let scope: StdDocScope = req
                .payload
                .get("scope")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let org_opt: Option<OrganizationProfile> = req
                .payload
                .get("orgProfile")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            match std_engine::analyze_file(Path::new(path), scope, org_opt.as_ref()) {
                Ok((report, _ir)) => ok(serde_json::to_value(report).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "execute_std_document" => {
            let exec_req: StdExecutionRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu chuẩn hóa không hợp lệ: {}", e)),
            };
            emit_js_event(
                proxy,
                "progress",
                serde_json::json!({
                    "phase": "process",
                    "current": 1,
                    "total": 1,
                    "file": exec_req.path,
                }),
            );
            let res = std_engine::execute_standardization(&exec_req);
            ok(serde_json::to_value(res).unwrap_or(Value::Null))
        }
        "restore_std_backup" => {
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
                Ok(message) => ok(serde_json::json!({ "message": message })),
                Err(e) => err(e),
            }
        }
        "list_templates" => {
            let store = template_engine::TemplateStore::default_store();
            match store.list_catalog() {
                Ok(cat) => ok(serde_json::to_value(cat).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "get_template" => {
            let id = req
                .payload
                .get("templateId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let store = template_engine::TemplateStore::default_store();
            match store.get_template(id) {
                Ok(tpl) => ok(serde_json::to_value(tpl).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "save_template" => {
            let save_req: SaveTemplateRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Dữ liệu lưu mẫu không hợp lệ: {}", e)),
            };
            let store = template_engine::TemplateStore::default_store();
            match store.save_template(save_req) {
                Ok(cat) => ok(serde_json::to_value(cat).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "clone_template" => {
            let source_id = req
                .payload
                .get("sourceId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new_name = req
                .payload
                .get("newName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let store = template_engine::TemplateStore::default_store();
            match store.clone_template(source_id, new_name) {
                Ok((tpl, cat)) => ok(serde_json::json!({
                    "template": tpl,
                    "catalog": cat,
                })),
                Err(e) => err(e),
            }
        }
        "delete_template" => {
            let id = req
                .payload
                .get("templateId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let store = template_engine::TemplateStore::default_store();
            match store.delete_template(id) {
                Ok(cat) => ok(serde_json::to_value(cat).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "set_default_template" => {
            let id = req
                .payload
                .get("templateId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let store = template_engine::TemplateStore::default_store();
            match store.set_default_template(id) {
                Ok(cat) => ok(serde_json::to_value(cat).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "list_template_versions" => {
            let id = req
                .payload
                .get("templateId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let store = template_engine::TemplateStore::default_store();
            match store.list_versions(id) {
                Ok(list) => ok(serde_json::to_value(list).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "restore_template_version" => {
            let id = req
                .payload
                .get("templateId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let ver = req
                .payload
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let store = template_engine::TemplateStore::default_store();
            match store.restore_version(id, ver) {
                Ok((tpl, cat)) => ok(serde_json::json!({
                    "template": tpl,
                    "catalog": cat,
                })),
                Err(e) => err(e),
            }
        }
        "import_docx_template" => {
            let imp_req: ImportDocxTemplateRequest =
                match serde_json::from_value(req.payload.clone()) {
                    Ok(r) => r,
                    Err(e) => return err(format!("Yêu cầu nhập mẫu DOCX không hợp lệ: {}", e)),
                };
            let store = template_engine::TemplateStore::default_store();
            match store.import_docx_template(imp_req) {
                Ok((tpl, cat)) => ok(serde_json::json!({
                    "template": tpl,
                    "catalog": cat,
                })),
                Err(e) => err(e),
            }
        }
        "export_docx_template" => {
            let id = req
                .payload
                .get("templateId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let dest = req
                .payload
                .get("destPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if dest.trim().is_empty() {
                return err("Chưa chỉ định đường dẫn lưu tệp DOCX mẫu.".to_string());
            }
            let store = template_engine::TemplateStore::default_store();
            match template_engine::export_docx_template(&store, id, Path::new(dest)) {
                Ok(path) => ok(serde_json::json!({ "outputPath": path })),
                Err(e) => err(e),
            }
        }
        "preview_template_application" => {
            let apply_req: TemplateApplyRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Dữ liệu xem trước mẫu không hợp lệ: {}", e)),
            };
            let store = template_engine::TemplateStore::default_store();
            match template_engine::preview_template(&store, &apply_req) {
                Ok(prev) => ok(serde_json::to_value(prev).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "apply_template_to_docx" => {
            let apply_req: TemplateApplyRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu áp dụng mẫu không hợp lệ: {}", e)),
            };
            let store = template_engine::TemplateStore::default_store();
            let res = template_engine::apply_template(&store, &apply_req);
            ok(serde_json::to_value(res).unwrap_or(Value::Null))
        }
        "restore_template_backup" => {
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
            match template_engine::restore_template_backup(Path::new(orig), Path::new(bak)) {
                Ok(message) => ok(serde_json::json!({ "message": message })),
                Err(e) => err(e),
            }
        }
        "analyze_layout_intelligence" => {
            let path = req.payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let scope: StdDocScope = req
                .payload
                .get("scope")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let org_opt: Option<OrganizationProfile> = req
                .payload
                .get("orgProfile")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .or_else(|| org_profile::load_org_profiles().into_iter().next());
            match layout_intelligence::analyze_file(Path::new(path), scope, org_opt.as_ref()) {
                Ok(report) => ok(serde_json::to_value(report).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "execute_layout_proposals" => {
            let exec_req: LayoutExecutionRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu điều chỉnh bố cục không hợp lệ: {}", e)),
            };
            let org_opt = org_profile::load_org_profiles().into_iter().next();
            let res = layout_intelligence::execute_proposals(&exec_req, org_opt.as_ref());
            ok(serde_json::to_value(res).unwrap_or(Value::Null))
        }
        "restore_layout_backup" => {
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
                Ok(message) => ok(serde_json::json!({ "message": message })),
                Err(e) => err(e),
            }
        }
        "get_compliance_rule_pack" => {
            let store = compliance_audit::ComplianceRulePackStore::default_store();
            ok(serde_json::to_value(compliance_audit::get_active_rule_pack(&store)).unwrap_or(Value::Null))
        }
        "save_compliance_rule_pack" => {
            let pack: ComplianceRulePack = match serde_json::from_value(req.payload.clone()) {
                Ok(p) => p,
                Err(e) => return err(format!("Dữ liệu Rule Pack không hợp lệ: {}", e)),
            };
            let store = compliance_audit::ComplianceRulePackStore::default_store();
            match compliance_audit::save_rule_pack(&store, &pack) {
                Ok(saved) => ok(serde_json::to_value(saved).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "reset_compliance_rule_pack" => {
            let store = compliance_audit::ComplianceRulePackStore::default_store();
            match compliance_audit::reset_rule_pack(&store) {
                Ok(pack) => ok(serde_json::to_value(pack).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "get_compliance_matrix" => {
            let scope: StdDocScope = req
                .payload
                .get("scope")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(StdDocScope::AdminNd30);
            let doc_type = req
                .payload
                .get("docType")
                .and_then(|v| v.as_str())
                .unwrap_or("quyetDinh");
            let org_opt: Option<OrganizationProfile> = req
                .payload
                .get("orgProfile")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .or_else(|| org_profile::load_org_profiles().into_iter().next());
            let store = compliance_audit::ComplianceRulePackStore::default_store();
            let rows = compliance_audit::preview_compliance_matrix(
                &store,
                scope,
                doc_type,
                org_opt.as_ref(),
            );
            ok(serde_json::to_value(rows).unwrap_or(Value::Null))
        }
        "run_compliance_audit" => {
            let mut audit_req: ComplianceAuditRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu kiểm toán tuân thủ không hợp lệ: {}", e)),
            };
            if audit_req.org_profile.is_none() {
                audit_req.org_profile = org_profile::load_org_profiles().into_iter().next();
            }
            let pack_store = compliance_audit::ComplianceRulePackStore::default_store();
            let hist_store = compliance_audit::AuditHistoryStore::default_store();
            match compliance_audit::run_compliance_audit(&audit_req, &pack_store, &hist_store) {
                Ok(rep) => ok(serde_json::to_value(rep).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "execute_compliance_autofix_recheck" => {
            let mut exec_req: ComplianceAutoFixAndRecheckRequest =
                match serde_json::from_value(req.payload.clone()) {
                    Ok(r) => r,
                    Err(e) => return err(format!("Yêu cầu Auto-Fix & Recheck không hợp lệ: {}", e)),
                };
            if exec_req.exec_request.org_profile.is_none() {
                exec_req.exec_request.org_profile = org_profile::load_org_profiles().into_iter().next();
            }
            let pack_store = compliance_audit::ComplianceRulePackStore::default_store();
            let hist_store = compliance_audit::AuditHistoryStore::default_store();
            let res = compliance_audit::execute_autofix_with_recheck(
                &exec_req,
                &pack_store,
                &hist_store,
            );
            ok(serde_json::to_value(res).unwrap_or(Value::Null))
        }
        "export_compliance_report" => {
            let report: ComplianceAuditReport = match req
                .payload
                .get("report")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
            {
                Some(r) => r,
                None => return err("Thiếu dữ liệu báo cáo để xuất.".to_string()),
            };
            let format: AuditExportFormat = req
                .payload
                .get("format")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(AuditExportFormat::Docx);
            let dest_path = req
                .payload
                .get("destPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if dest_path.trim().is_empty() {
                return err("Chưa chọn đường dẫn lưu tệp báo cáo.".to_string());
            }
            let res = compliance_audit::export_compliance_report(&report, format, Path::new(dest_path));
            ok(serde_json::to_value(res).unwrap_or(Value::Null))
        }
        "list_audit_history" => {
            let hist_store = compliance_audit::AuditHistoryStore::default_store();
            match compliance_audit::list_audit_history(&hist_store) {
                Ok(entries) => ok(serde_json::to_value(entries).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "compare_audit_history" => {
            let older_id = req.payload.get("olderAuditId").and_then(|v| v.as_str()).unwrap_or("");
            let newer_id = req.payload.get("newerAuditId").and_then(|v| v.as_str()).unwrap_or("");
            let hist_store = compliance_audit::AuditHistoryStore::default_store();
            match compliance_audit::compare_audit_history_entries(&hist_store, older_id, newer_id) {
                Ok(cmp) => ok(serde_json::to_value(cmp).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "delete_audit_history_entry" => {
            let audit_id = req.payload.get("auditId").and_then(|v| v.as_str()).unwrap_or("");
            let hist_store = compliance_audit::AuditHistoryStore::default_store();
            match hist_store.delete_entry(audit_id) {
                Ok(entries) => ok(serde_json::to_value(entries).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "clear_audit_history" => {
            let hist_store = compliance_audit::AuditHistoryStore::default_store();
            match hist_store.clear_all() {
                Ok(()) => ok(serde_json::json!({ "cleared": true })),
                Err(e) => err(e),
            }
        }
        "build_live_preview" => {
            let pv_req: PreviewBuildRequest = match serde_json::from_value(req.payload.clone()) {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu dựng xem trước A4 không hợp lệ: {}", e)),
            };
            let org_opt = pv_req
                .overrides
                .org_profile_id
                .as_deref()
                .and_then(org_profile::get_org_profile)
                .or_else(|| org_profile::load_org_profiles().into_iter().next());
            let payload = live_preview::build_live_preview(&pv_req, org_opt.as_ref());
            ok(serde_json::to_value(payload).unwrap_or(Value::Null))
        }
        "cancel_preview_render" => {
            let job_id = req.payload.get("jobId").and_then(|v| v.as_str()).unwrap_or("*");
            let cancelled = live_preview::cancel_preview_render(job_id);
            ok(serde_json::json!({ "cancelled": cancelled }))
        }
        "get_preview_media" => {
            let path = req.payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let media_entry = req
                .payload
                .get("mediaEntry")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match live_preview::load_preview_media_base64(Path::new(path), media_entry) {
                Ok(data_url) => ok(serde_json::json!({ "dataUrl": data_url })),
                Err(e) => err(e),
            }
        }
        "verify_preview_word_pdf" => {
            let pv_res: Result<PreviewBuildRequest, _> = match req.payload.get("request") {
                Some(v) => serde_json::from_value(v.clone()),
                None => serde_json::from_value(req.payload.clone()),
            };
            let pv_req = match pv_res {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu đối chiếu Word COM không hợp lệ: {}", e)),
            };
            let target_variant = req
                .payload
                .get("targetVariant")
                .and_then(|v| v.as_str())
                .unwrap_or("after");
            match live_preview::render_word_com_verification(&pv_req, target_variant) {
                Ok(ver) => ok(serde_json::to_value(ver).unwrap_or(Value::Null)),
                Err(e) => err(e),
            }
        }
        "open_preview_in_word" => {
            let pv_res: Result<PreviewBuildRequest, _> = match req.payload.get("request") {
                Some(v) => serde_json::from_value(v.clone()),
                None => serde_json::from_value(req.payload.clone()),
            };
            let pv_req = match pv_res {
                Ok(r) => r,
                Err(e) => return err(format!("Yêu cầu mở Word không hợp lệ: {}", e)),
            };
            let target_variant = req
                .payload
                .get("targetVariant")
                .and_then(|v| v.as_str())
                .unwrap_or("before");
            match live_preview::open_preview_in_microsoft_word(&pv_req, target_variant) {
                Ok(message) => ok(serde_json::json!({ "message": message })),
                Err(e) => err(e),
            }
        }
        "cancel_active_task" => {
            resource_guard::request_batch_cancel();
            let _ = live_preview::cancel_preview_render("*");
            record_op(
                "ipc",
                "cancel_active_task",
                None,
                "cancelled",
                0,
                Some("ERR_TASK_CANCELLED"),
                "Người dùng yêu cầu dừng tác vụ đang chạy",
            );
            ok(serde_json::json!({ "cancelled": true }))
        }
        "list_file_backups" => {
            let path = req
                .payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if path.trim().is_empty() {
                return err(AppStructuredError::new(
                    AppErrorCode::ErrIpcInvalid,
                    "Thiếu đường dẫn tệp cần liệt kê bản sao lưu.",
                )
                .format_user_message());
            }
            let list = backup_manager::list_backups_for_file(Path::new(path));
            ok(serde_json::to_value(list).unwrap_or(Value::Null))
        }
        "restore_backup_version" => {
            let orig = req
                .payload
                .get("originalPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mode = req
                .payload
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("latest");
            let bak = req
                .payload
                .get("backupPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if orig.trim().is_empty() {
                return err(AppStructuredError::new(
                    AppErrorCode::ErrIpcInvalid,
                    "Thiếu đường dẫn tệp gốc cần khôi phục.",
                )
                .format_user_message());
            }
            let orig_p = Path::new(orig);
            let res = match mode {
                "initial" => backup_manager::restore_initial_baseline(orig_p),
                "specific" if !bak.trim().is_empty() => {
                    backup_manager::restore_backup_checked(orig_p, Path::new(bak))
                }
                _ => backup_manager::restore_latest_pre_edit(orig_p),
            };
            match res {
                Ok(message) => ok(serde_json::json!({ "message": message })),
                Err(e) => err(e),
            }
        }
        "get_operation_logs" => {
            let limit = req
                .payload
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(150);
            let logs = logger::get_recent_logs(limit);
            ok(serde_json::to_value(logs).unwrap_or(Value::Null))
        }
        "clear_operation_logs" => match logger::clear_logs() {
            Ok(()) => ok(serde_json::json!({ "cleared": true })),
            Err(e) => err(e),
        },
        "recover_interrupted_saves" => {
            let paths: Vec<String> = req
                .payload
                .get("paths")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let mut recovered = Vec::new();
            for p_raw in &paths {
                let p = Path::new(p_raw);
                let dir = if p.is_dir() {
                    p
                } else {
                    p.parent().unwrap_or_else(|| Path::new("."))
                };
                recovered.extend(safe_save::recover_interrupted_saves_in_dir(dir));
            }
            ok(serde_json::json!({ "recovered": recovered }))
        }
        other => {
            let msg = AppStructuredError::new(
                AppErrorCode::ErrIpcInvalid,
                format!("Lệnh IPC chưa hỗ trợ: {}", other),
            )
            .format_user_message();
            record_op("ipc", other, None, "error", 0, Some("ERR_IPC_INVALID"), &msg);
            err(msg)
        }
    }
}

pub fn run_native_dialog(owner_hwnd: isize, kind: DialogKind) -> Result<Vec<String>, String> {
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let did_init = init.is_ok();

        let result = (|| -> Result<Vec<String>, String> {
            let hwnd = if owner_hwnd != 0 {
                Some(HWND(owner_hwnd as *mut core::ffi::c_void))
            } else {
                None
            };

            if let DialogKind::SaveAuditExport { default_name, ext } = &kind {
                let save_dialog: IFileSaveDialog =
                    CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL)
                        .map_err(|e| format!("Không thể mở hộp thoại lưu Windows: {}", e))?;
                let mut opts = save_dialog.GetOptions().unwrap_or_default();
                opts |= FOS_OVERWRITEPROMPT | FOS_PATHMUSTEXIST;
                let _ = save_dialog.SetOptions(opts);
                let (filters, def_ext) = match ext.as_str() {
                    "pdf" => (
                        vec![COMDLG_FILTERSPEC {
                            pszName: w!("Tài liệu PDF (*.pdf)"),
                            pszSpec: w!("*.pdf"),
                        }],
                        w!("pdf"),
                    ),
                    "csv" => (
                        vec![COMDLG_FILTERSPEC {
                            pszName: w!("Bảng lỗi CSV (*.csv)"),
                            pszSpec: w!("*.csv"),
                        }],
                        w!("csv"),
                    ),
                    "json" => (
                        vec![COMDLG_FILTERSPEC {
                            pszName: w!("Báo cáo JSON (*.json)"),
                            pszSpec: w!("*.json"),
                        }],
                        w!("json"),
                    ),
                    _ => (
                        vec![COMDLG_FILTERSPEC {
                            pszName: w!("Tài liệu Word (*.docx)"),
                            pszSpec: w!("*.docx"),
                        }],
                        w!("docx"),
                    ),
                };
                let _ = save_dialog.SetFileTypes(&filters);
                let _ = save_dialog.SetDefaultExtension(def_ext);
                let wide_name: Vec<u16> = default_name
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let _ = save_dialog.SetFileName(PCWSTR(wide_name.as_ptr()));
                let _ = save_dialog.SetTitle(w!("Xuất báo cáo kiểm toán MonWord"));

                if save_dialog.Show(hwnd).is_err() {
                    return Ok(Vec::new());
                }
                let item = save_dialog
                    .GetResult()
                    .map_err(|e| format!("Không đọc được đường dẫn lưu: {}", e))?;
                let pwstr = item
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .map_err(|e| format!("Không lấy được tên tệp lưu: {}", e))?;
                let s = pwstr.to_string().map_err(|e| e.to_string())?;
                CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                return Ok(vec![s]);
            }

            if let DialogKind::SaveDocxFile(default_name) = &kind {
                let save_dialog: IFileSaveDialog =
                    CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL)
                        .map_err(|e| format!("Không thể mở hộp thoại lưu Windows: {}", e))?;
                let mut opts = save_dialog.GetOptions().unwrap_or_default();
                opts |= FOS_OVERWRITEPROMPT | FOS_PATHMUSTEXIST;
                let _ = save_dialog.SetOptions(opts);
                let filters = [COMDLG_FILTERSPEC {
                    pszName: w!("Tài liệu Word (*.docx)"),
                    pszSpec: w!("*.docx"),
                }];
                let _ = save_dialog.SetFileTypes(&filters);
                let _ = save_dialog.SetDefaultExtension(w!("docx"));
                let wide_name: Vec<u16> = default_name
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let _ = save_dialog.SetFileName(PCWSTR(wide_name.as_ptr()));
                let _ = save_dialog.SetTitle(w!("Lưu văn bản phần đầu mới (.docx)"));

                if save_dialog.Show(hwnd).is_err() {
                    return Ok(Vec::new());
                }
                let item = save_dialog
                    .GetResult()
                    .map_err(|e| format!("Không đọc được đường dẫn lưu: {}", e))?;
                let pwstr = item
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .map_err(|e| format!("Không lấy được tên tệp lưu: {}", e))?;
                let s = pwstr.to_string().map_err(|e| e.to_string())?;
                CoTaskMemFree(Some(pwstr.0 as *const core::ffi::c_void));
                return Ok(vec![s]);
            }

            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL)
                    .map_err(|e| format!("Không thể mở hộp thoại Windows: {}", e))?;

            let mut opts = dialog.GetOptions().unwrap_or_default();
            match &kind {
                DialogKind::SaveDocxFile(_) | DialogKind::SaveAuditExport { .. } => unreachable!(),
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
                DialogKind::PickTemplateDocx => {
                    opts |= FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST;
                    let filters = [COMDLG_FILTERSPEC {
                        pszName: w!("Mẫu Word OpenXML (*.docx;*.docm)"),
                        pszSpec: w!("*.docx;*.docm"),
                    }];
                    let _ = dialog.SetFileTypes(&filters);
                    dialog.SetTitle(w!("Chọn tệp DOCX/DOCM làm mẫu văn bản")).ok();
                }
            }

            let _ = dialog.SetOptions(opts);
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
