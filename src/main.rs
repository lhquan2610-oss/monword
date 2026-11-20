#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod ipc;
mod models;
mod ota;

#[cfg(test)]
mod installer_core;
#[cfg(test)]
mod tests;

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

use ipc::{emit_js_event, handle_ipc_message, run_native_dialog, send_ipc_reply, DialogKind, IpcResponse, UserEvent};
use serde_json::Value;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::windows::WindowExtWindows;
use tao::window::{Icon, WindowBuilder};
use wry::http::{header::CONTENT_TYPE, Request, Response};
use wry::{DragDropEvent, WebViewBuilder};

fn main() -> wry::Result<()> {
    if let Some(exit_code) = try_handle_cli_flags() {
        std::process::exit(exit_code);
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window_icon = Icon::from_rgba(
        include_bytes!("../assets/monword_rgba_64.bin").to_vec(),
        64,
        64,
    )
    .ok();

    let app_title = format!("MonWord v{} — Chuẩn Hóa Văn Bản Pro", ota::effective_version());
    let window = WindowBuilder::new()
        .with_title(app_title)
        .with_window_icon(window_icon)
        .with_inner_size(LogicalSize::new(1240.0, 800.0))
        .with_min_inner_size(LogicalSize::new(980.0, 660.0))
        .build(&event_loop)
        .expect("Không thể khởi tạo cửa sổ ứng dụng");

    let owner_hwnd = window.hwnd();
    let ipc_proxy = proxy.clone();
    let dd_proxy = proxy.clone();

    let webview = WebViewBuilder::new()
        .with_custom_protocol("app".into(), move |_webview_id, request: Request<Vec<u8>>| {
            let path = request.uri().path();
            if let Some(pdf_name) = path.strip_prefix("/__preview_pdf__/") {
                let is_safe_pdf = !pdf_name.is_empty()
                    && !pdf_name.contains("..")
                    && !pdf_name.contains('/')
                    && !pdf_name.contains('\\')
                    && pdf_name.ends_with(".pdf")
                    && pdf_name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.');
                let pdf_path = engine::live_preview::preview_temp_dir().join(pdf_name);
                if is_safe_pdf && pdf_path.is_file() {
                    if let Ok(bytes) = std::fs::read(&pdf_path) {
                        return Response::builder()
                            .header(CONTENT_TYPE, "application/pdf")
                            .header("Cache-Control", "no-store")
                            .status(200)
                            .body(bytes)
                            .unwrap()
                            .map(Into::into);
                    }
                }
                return Response::builder()
                    .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                    .status(404)
                    .body(b"Preview PDF not found".to_vec())
                    .unwrap()
                    .map(Into::into);
            }
            if let Some((mime, bytes)) = ota::read_ota_asset(path) {
                Response::builder()
                    .header(CONTENT_TYPE, mime)
                    .header("Cache-Control", "no-store")
                    .status(200)
                    .body(bytes)
                    .unwrap()
                    .map(Into::into)
            } else if let Some((mime, bytes)) = get_asset(path) {
                Response::builder()
                    .header(CONTENT_TYPE, mime)
                    .status(200)
                    .body(bytes.to_vec())
                    .unwrap()
                    .map(Into::into)
            } else if let Some((mime, bytes)) = get_asset("index.html") {
                Response::builder()
                    .header(CONTENT_TYPE, mime)
                    .status(200)
                    .body(bytes.to_vec())
                    .unwrap()
                    .map(Into::into)
            } else {
                Response::builder()
                    .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                    .status(404)
                    .body(b"Asset not found".to_vec())
                    .unwrap()
                    .map(Into::into)
            }
        })
        .with_ipc_handler(move |req: Request<String>| {
            let body = req.body().clone();
            handle_ipc_message(body, ipc_proxy.clone());
        })
        .with_drag_drop_handler(move |event: DragDropEvent| {
            match event {
                DragDropEvent::Enter { .. } => {
                    emit_js_event(&dd_proxy, "drag_state", serde_json::json!({ "over": true }));
                }
                DragDropEvent::Leave => {
                    emit_js_event(&dd_proxy, "drag_state", serde_json::json!({ "over": false }));
                }
                DragDropEvent::Drop { paths, .. } => {
                    emit_js_event(&dd_proxy, "drag_state", serde_json::json!({ "over": false }));
                    let str_paths: Vec<String> = paths
                        .into_iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    let items = engine::scan_paths(&str_paths);
                    emit_js_event(
                        &dd_proxy,
                        "files_dropped",
                        serde_json::to_value(items).unwrap_or(Value::Null),
                    );
                }
                _ => {}
            }
            // Return true to prevent WebView2 from trying to navigate to dropped file URL
            true
        })
        .with_url("app://localhost/index.html")
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                engine::live_preview::cleanup_all_temp_previews();
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(user_event) => match user_event {
                UserEvent::EvalJs(script) => {
                    let _ = webview.evaluate_script(&script);
                }
                UserEvent::ShowNativeDialog { id, kind } => {
                    match run_native_dialog(owner_hwnd, kind.clone()) {
                        Ok(paths) => {
                            let data = match kind {
                                DialogKind::PickOfficeFiles | DialogKind::PickFolder => {
                                    let items = engine::scan_paths(&paths);
                                    serde_json::json!({
                                        "paths": paths,
                                        "items": items,
                                    })
                                }
                                _ => serde_json::json!({ "paths": paths }),
                            };
                            send_ipc_reply(
                                &proxy,
                                IpcResponse {
                                    id,
                                    ok: true,
                                    data,
                                    error: None,
                                },
                            );
                        }
                        Err(e) => {
                            send_ipc_reply(
                                &proxy,
                                IpcResponse {
                                    id,
                                    ok: false,
                                    data: Value::Null,
                                    error: Some(e),
                                },
                            );
                        }
                    }
                }
            },
            _ => {}
        }
    });
}

fn try_handle_cli_flags() -> Option<i32> {
    use std::io::Write;

    let args: Vec<String> = std::env::args().collect();
    let has_version = args.iter().any(|a| a == "--version" || a == "-v");
    let has_self_check = args.iter().any(|a| a == "--self-check");
    if !has_version && !has_self_check {
        return None;
    }

    let out_path = args
        .windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| w[1].clone());

    let (code, text) = if has_self_check {
        let has_index = get_asset("index.html").is_some();
        let rules_count = engine::std_engine::get_std_rules_catalog().len();
        let tpl_count = engine::template_engine::TemplateStore::default_store()
            .list_catalog()
            .map(|c| c.templates.len())
            .unwrap_or(0);
        let ok = has_index && rules_count >= 15 && tpl_count >= 20;
        let msg = if ok {
            format!(
                "MONWORD_SELF_CHECK_OK v{} | ui_embedded={} | std_rules={} | templates={}\n",
                env!("CARGO_PKG_VERSION"),
                has_index,
                rules_count,
                tpl_count
            )
        } else {
            format!(
                "MONWORD_SELF_CHECK_FAILED v{} | ui_embedded={} | std_rules={} | templates={}\n",
                env!("CARGO_PKG_VERSION"),
                has_index,
                rules_count,
                tpl_count
            )
        };
        (if ok { 0 } else { 1 }, msg)
    } else {
        (
            0,
            format!(
                "MonWord v{} (x86_64-pc-windows-gnullvm)\n",
                env!("CARGO_PKG_VERSION")
            ),
        )
    };

    let _ = std::io::stdout().write_all(text.as_bytes());
    let _ = std::io::stdout().flush();
    if let Some(p) = out_path {
        if let Err(e) = std::fs::write(&p, text.as_bytes()) {
            let _ = std::io::stderr().write_all(format!("Failed writing --out {}: {}\n", p, e).as_bytes());
            return Some(1);
        }
    }
    Some(code)
}
