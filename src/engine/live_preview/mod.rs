pub mod cancel;
pub mod differ_and_print;
pub mod extractor;
pub mod paginator;
pub mod transformer;

use crate::engine::compliance_audit::sha256_util::sha256_hex;
use crate::engine::reliability::error_code::{AppErrorCode, AppStructuredError};
use crate::engine::reliability::logger::OpTimer;
use crate::engine::reliability::resource_guard;
use crate::engine::{org_profile, system_office};
use crate::models::{
    OrganizationProfile, PreviewBuildRequest, PreviewDocumentRender, PreviewSessionPayload,
    WordComPreviewVerification,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use zip::ZipArchive;

pub const PREVIEW_FIDELITY_DISCLAIMER_VI: &str = "Lưu ý độ trung thực: Bản xem trước OpenXML dựng trực tiếp khổ giấy, section, lề, phông chữ, bảng biểu, hình ảnh và ước lượng phân trang từ cấu trúc tệp; KHÔNG khẳng định khớp tuyệt đối từng dòng với Microsoft Word nếu chưa đối chiếu trực tiếp bằng Microsoft Word.";

pub fn preview_temp_dir() -> PathBuf {
    env::temp_dir().join("monword_live_preview")
}

pub fn cleanup_all_temp_previews() {
    cleanup_all_temp_previews_in(&preview_temp_dir());
}

pub fn cleanup_all_temp_previews_in(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() {
            let _ = fs::remove_file(p);
        }
    }
}

pub fn cancel_preview_render(job_id: &str) -> bool {
    cancel::cancel_job(job_id)
}

pub fn build_live_preview(
    req: &PreviewBuildRequest,
    explicit_org: Option<&OrganizationProfile>,
) -> PreviewSessionPayload {
    let started = Instant::now();
    let tok = cancel::start_job(&req.job_id);
    let word_installed = system_office::get_system_status().office_version.is_some();

    let res = build_live_preview_inner(req, explicit_org, &tok, started, word_installed);
    cancel::finish_job(&req.job_id);

    match res {
        Ok(payload) => payload,
        Err(err_msg) => PreviewSessionPayload {
            job_id: req.job_id.clone(),
            status: if err_msg == "CANCELLED" || tok.is_cancelled() {
                "cancelled".to_string()
            } else {
                format!("error: {}", err_msg)
            },
            update_mode: "error".to_string(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            fidelity_mode: "openxmlMetricLayout".to_string(),
            fidelity_disclaimer: PREVIEW_FIDELITY_DISCLAIMER_VI.to_string(),
            word_installed,
            before: None,
            after: None,
            diffs: Vec::new(),
            audit_markers: Vec::new(),
        },
    }
}

fn build_live_preview_inner(
    req: &PreviewBuildRequest,
    explicit_org: Option<&OrganizationProfile>,
    tok: &cancel::CancelToken,
    started: Instant,
    word_installed: bool,
) -> Result<PreviewSessionPayload, String> {
    let src_path = Path::new(&req.path);
    if !src_path.is_file() {
        return Err(format!("Không tìm thấy tệp xem trước: {}", req.path));
    }

    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "docx" && ext != "docm" {
        return Err(format!(
            "Xem trước OpenXML hỗ trợ trực tiếp .docx / .docm (tệp .{} hãy chuyển đổi trước).",
            ext
        ));
    }

    let (baseline, was_cached) = transformer::load_or_extract_baseline(src_path, Some(tok))?;
    if tok.is_cancelled() {
        return Err("CANCELLED".to_string());
    }

    let before_print = differ_and_print::evaluate_print_readiness(
        &baseline.pages,
        &baseline.extracted.sections,
        req.scope,
        PREVIEW_FIDELITY_DISCLAIMER_VI,
    );
    let before_render = PreviewDocumentRender {
        label: "Bản gốc".to_string(),
        doc_sha256: baseline.doc_sha256.clone(),
        section_count: baseline.extracted.sections.len(),
        total_pages: baseline.pages.len(),
        paragraph_count: baseline.extracted.paragraph_count,
        table_count: baseline.extracted.table_count,
        image_count: baseline.extracted.image_count,
        sections: baseline.extracted.sections.clone(),
        pages: baseline.pages.clone(),
        print_check: before_print,
    };

    let mut update_mode = if was_cached {
        "incrementalPatch"
    } else {
        "initial"
    }
    .to_string();

    let after_render: Option<PreviewDocumentRender> = if req.include_after {
        if let Some(after_p_str) = req
            .after_file_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let after_path = Path::new(after_p_str);
            if after_path.is_file() {
                update_mode = "fullRebuild".to_string();
                let (after_base, _) = transformer::load_or_extract_baseline(after_path, Some(tok))?;
                let after_print = differ_and_print::evaluate_print_readiness(
                    &after_base.pages,
                    &after_base.extracted.sections,
                    req.scope,
                    PREVIEW_FIDELITY_DISCLAIMER_VI,
                );
                Some(PreviewDocumentRender {
                    label: "Bản đã lưu sau xử lý".to_string(),
                    doc_sha256: after_base.doc_sha256,
                    section_count: after_base.extracted.sections.len(),
                    total_pages: after_base.pages.len(),
                    paragraph_count: after_base.extracted.paragraph_count,
                    table_count: after_base.extracted.table_count,
                    image_count: after_base.extracted.image_count,
                    sections: after_base.extracted.sections,
                    pages: after_base.pages,
                    print_check: after_print,
                })
            } else {
                None
            }
        } else if transformer::can_use_incremental_patch(req) {
            update_mode = "incrementalPatch".to_string();
            let patched = transformer::apply_incremental_overrides_to_extracted(
                &baseline.extracted,
                &req.overrides,
            );
            let after_pages = paginator::paginate_document(&patched, false, Some(tok))?;
            let after_print = differ_and_print::evaluate_print_readiness(
                &after_pages,
                &patched.sections,
                req.scope,
                PREVIEW_FIDELITY_DISCLAIMER_VI,
            );
            Some(PreviewDocumentRender {
                label: "Xem trước trực tiếp (Trong bộ nhớ)".to_string(),
                doc_sha256: baseline.doc_sha256.clone(),
                section_count: patched.sections.len(),
                total_pages: after_pages.len(),
                paragraph_count: patched.paragraph_count,
                table_count: patched.table_count,
                image_count: patched.image_count,
                sections: patched.sections,
                pages: after_pages,
                print_check: after_print,
            })
        } else {
            update_mode = "fullRebuild".to_string();
            let (_pkg, after_ext, after_sha) =
                transformer::build_transformed_package_in_memory(src_path, req, explicit_org, Some(tok))?;
            let after_pages = paginator::paginate_document(&after_ext, false, Some(tok))?;
            let after_print = differ_and_print::evaluate_print_readiness(
                &after_pages,
                &after_ext.sections,
                req.scope,
                PREVIEW_FIDELITY_DISCLAIMER_VI,
            );
            Some(PreviewDocumentRender {
                label: "Bản xem trước sau chuẩn hóa (Chưa ghi đè gốc)".to_string(),
                doc_sha256: after_sha,
                section_count: after_ext.sections.len(),
                total_pages: after_pages.len(),
                paragraph_count: after_ext.paragraph_count,
                table_count: after_ext.table_count,
                image_count: after_ext.image_count,
                sections: after_ext.sections,
                pages: after_pages,
                print_check: after_print,
            })
        }
    } else {
        None
    };

    if tok.is_cancelled() {
        return Err("CANCELLED".to_string());
    }

    let diffs = if let Some(ref after) = after_render {
        differ_and_print::compute_before_after_diffs(
            &before_render.pages,
            &after.pages,
            &before_render.sections,
            &after.sections,
        )
    } else {
        Vec::new()
    };

    let audit_markers = if req.include_audit_sync {
        let org_opt = explicit_org.cloned().or_else(|| {
            req.overrides
                .org_profile_id
                .as_deref()
                .and_then(org_profile::get_org_profile)
        });
        differ_and_print::build_audit_sync_markers(
            src_path,
            req.scope,
            &before_render.pages,
            org_opt.as_ref(),
        )
    } else {
        Vec::new()
    };

    Ok(PreviewSessionPayload {
        job_id: req.job_id.clone(),
        status: "completed".to_string(),
        update_mode,
        elapsed_ms: started.elapsed().as_millis() as u64,
        fidelity_mode: "openxmlMetricLayout".to_string(),
        fidelity_disclaimer: PREVIEW_FIDELITY_DISCLAIMER_VI.to_string(),
        word_installed,
        before: Some(before_render),
        after: after_render,
        diffs,
        audit_markers,
    })
}

/// Lazily reads a single embedded media entry (`word/media/*`) as a `data:<mime>;base64,...` URI
/// without ever keeping binary assets cached across all pages.
pub fn load_preview_media_base64(path: &Path, media_entry: &str) -> Result<String, String> {
    let norm = crate::engine::openxml_arch::validate_zip_entry_name(media_entry)?;
    if !norm.to_ascii_lowercase().starts_with("word/media/") {
        return Err(format!(
            "Đường dẫn media không hợp lệ hoặc ngoài phạm vi word/media/: {}",
            media_entry
        ));
    }

    let meta = fs::metadata(path)
        .map_err(|e| format!("Không truy cập được tệp '{}': {}", path.display(), e))?;
    resource_guard::check_compressed_file_size(meta.len(), Some(path))?;

    let file = fs::File::open(path)
        .map_err(|e| format!("Không mở được tệp '{}' để đọc ảnh: {}", path.display(), e))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Tệp không phải OpenXML hợp lệ: {}", e))?;
    resource_guard::check_zip_entry_count(archive.len())?;

    for i in 0..archive.len() {
        let mut item = archive
            .by_index(i)
            .map_err(|e| format!("Lỗi đọc entry media: {}", e))?;
        let item_norm = item.name().trim_start_matches('/').replace('\\', "/");
        if item_norm.eq_ignore_ascii_case(&norm) {
            let comp_size = item.compressed_size();
            let decl_size = item.size();
            if decl_size > 20 * 1024 * 1024 {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrResourceLimit,
                    "Ảnh vượt ngưỡng xem trước an toàn (20 MB).",
                )
                .format_user_message());
            }
            let mut running = 0u64;
            let buf = resource_guard::read_zip_entry_bounded(
                &mut item,
                &norm,
                comp_size,
                decl_size,
                &mut running,
                false,
            )?;
            if buf.len() > 20 * 1024 * 1024 {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrResourceLimit,
                    "Ảnh vượt ngưỡng xem trước an toàn (20 MB).",
                )
                .format_user_message());
            }
            let mime = if norm.to_ascii_lowercase().ends_with(".png") {
                "image/png"
            } else if norm.to_ascii_lowercase().ends_with(".jpg")
                || norm.to_ascii_lowercase().ends_with(".jpeg")
            {
                "image/jpeg"
            } else if norm.to_ascii_lowercase().ends_with(".gif") {
                "image/gif"
            } else if norm.to_ascii_lowercase().ends_with(".svg") {
                "image/svg+xml"
            } else if norm.to_ascii_lowercase().ends_with(".webp") {
                "image/webp"
            } else {
                "image/png"
            };
            return Ok(format!("data:{};base64,{}", mime, encode_base64_std(&buf)));
        }
    }

    Err(format!("Không tìm thấy ảnh '{}' trong gói tài liệu.", norm))
}

fn encode_base64_std(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(((data.len() + 2) / 3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn cleanup_old_temp_previews() {
    let dir = preview_temp_dir();
    let Ok(rd) = fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<_> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let t = m.modified().ok()?;
            Some((e.path(), t))
        })
        .collect();
    if files.len() > 20 {
        files.sort_by_key(|(_, t)| *t);
        for (old_p, _) in files.into_iter().take(10) {
            let _ = fs::remove_file(old_p);
        }
    }
}

/// Renders an exact Microsoft Word COM PDF + real Word page count into isolated temp dir
/// for Tier-2 ground-truth verification, without ever modifying the user's original file.
pub fn render_word_com_verification(
    req: &PreviewBuildRequest,
    target_variant: &str,
) -> Result<WordComPreviewVerification, String> {
    let started = Instant::now();
    let tok = cancel::start_job(&format!("{}::word", req.job_id));
    let src_path = Path::new(&req.path);
    if !src_path.is_file() {
        cancel::finish_job(&format!("{}::word", req.job_id));
        return Err(format!("Không tìm thấy tệp '{}'", req.path));
    }

    let tmp_dir = preview_temp_dir();
    fs::create_dir_all(&tmp_dir)
        .map_err(|e| format!("Không tạo được thư mục tạm xem trước: {}", e))?;
    cleanup_old_temp_previews();

    let raw_bytes = fs::read(src_path).map_err(|e| format!("Không đọc được tệp: {}", e))?;
    let hash_short: String = sha256_hex(&raw_bytes).chars().take(10).collect();
    let stamp = chrono::Local::now().timestamp_millis();
    let base_token = format!("mw_{}_{}_{}", target_variant, hash_short, stamp);
    let temp_docx_path = tmp_dir.join(format!("{}.docx", base_token));
    let pdf_file_name = format!("{}.pdf", base_token);
    let pdf_disk_path = tmp_dir.join(&pdf_file_name);

    let (baseline, _) = transformer::load_or_extract_baseline(src_path, Some(&tok))?;
    let openxml_est_pages = if target_variant == "after" {
        if let Some(after_raw) = req
            .after_file_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let after_p = Path::new(after_raw);
            fs::copy(after_p, &temp_docx_path)
                .map_err(|e| format!("Lỗi tạo bản sao tạm After: {}", e))?;
            transformer::load_or_extract_baseline(after_p, Some(&tok))?
                .0
                .pages
                .len()
        } else {
            let (pkg, ext_after, _) =
                transformer::build_transformed_package_in_memory(src_path, req, None, Some(&tok))?;
            pkg.save_to(&temp_docx_path)?;
            paginator::paginate_document(&ext_after, false, Some(&tok))?.len()
        }
    } else {
        fs::copy(src_path, &temp_docx_path)
            .map_err(|e| format!("Lỗi tạo bản sao tạm Before: {}", e))?;
        baseline.pages.len()
    };

    if tok.is_cancelled() {
        let _ = fs::remove_file(&temp_docx_path);
        cancel::finish_job(&format!("{}::word", req.job_id));
        return Ok(WordComPreviewVerification {
            job_id: req.job_id.clone(),
            cancelled: true,
            target_variant: target_variant.to_string(),
            word_page_count: 0,
            openxml_estimated_pages: openxml_est_pages,
            page_count_delta: 0,
            pdf_url_path: String::new(),
            pdf_disk_path: String::new(),
            temp_docx_path: String::new(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            note: "Đã hủy kết xuất PDF đối chiếu theo yêu cầu.".to_string(),
        });
    }

    let timer = OpTimer::start("live_preview", "verify_preview_word_pdf", Some(src_path));
    let inner_body = format!(
        r#"
  $doc = $word.Documents.Open('{src}', $false, $true)
  $doc.Repaginate()
  $pgCount = [int]$doc.ComputeStatistics(2)
  $doc.ExportAsFixedFormat('{pdf}', 17)
  Write-Output ("WORD_PREVIEW_OK|" + $pgCount)
"#,
        src = temp_docx_path.to_string_lossy().replace('\'', "''"),
        pdf = pdf_disk_path.to_string_lossy().replace('\'', "''"),
    );

    let cancel_probe = || tok.is_cancelled();
    let timeout_ms = resource_guard::current_limits().word_com_timeout_ms;
    let out_res = system_office::run_word_com_script_with_timeout(
        &inner_body,
        timeout_ms,
        Some(&cancel_probe),
    );
    cancel::finish_job(&format!("{}::word", req.job_id));

    let out = match out_res {
        Ok(o) => o,
        Err(e) => {
            if tok.is_cancelled() {
                let _ = fs::remove_file(&temp_docx_path);
                let _ = fs::remove_file(&pdf_disk_path);
                timer.finish_cancelled("Hủy kết xuất PDF đối chiếu");
                return Ok(WordComPreviewVerification {
                    job_id: req.job_id.clone(),
                    cancelled: true,
                    target_variant: target_variant.to_string(),
                    word_page_count: 0,
                    openxml_estimated_pages: openxml_est_pages,
                    page_count_delta: 0,
                    pdf_url_path: String::new(),
                    pdf_disk_path: String::new(),
                    temp_docx_path: String::new(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    note: "Đã hủy kết xuất PDF đối chiếu theo yêu cầu.".to_string(),
                });
            }
            let code = crate::engine::reliability::error_code::classify_error_message(
                &e,
                AppErrorCode::ErrWordCom,
            )
            .code
            .as_str();
            timer.finish_err(code, &e);
            return Err(e);
        }
    };

    if !pdf_disk_path.is_file() || fs::metadata(&pdf_disk_path).map(|m| m.len()).unwrap_or(0) < 100
    {
        let err_msg = AppStructuredError::new(
            AppErrorCode::ErrDocConvert,
            "Microsoft Word COM không tạo được tệp PDF đối chiếu hợp lệ.",
        )
        .format_user_message();
        timer.finish_err("ERR_DOC_CONVERT", &err_msg);
        return Err(err_msg);
    }
    timer.finish_ok("Đối chiếu PDF qua Word COM thành công");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let word_page_count: usize = stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("WORD_PREVIEW_OK|"))
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or(openxml_est_pages);

    let delta = word_page_count as isize - openxml_est_pages as isize;
    let note = if delta == 0 {
        format!(
            "Phân trang OpenXML khớp số trang Microsoft Word thực tế ({} trang).",
            word_page_count
        )
    } else {
        format!(
            "Microsoft Word dàn thành {} trang (chênh {:+} trang so với ước lượng OpenXML {} trang — hãy kiểm tra trực tiếp trên bản PDF Word vừa kết xuất).",
            word_page_count, delta, openxml_est_pages
        )
    };

    Ok(WordComPreviewVerification {
        job_id: req.job_id.clone(),
        cancelled: false,
        target_variant: target_variant.to_string(),
        word_page_count,
        openxml_estimated_pages: openxml_est_pages,
        page_count_delta: delta,
        pdf_url_path: format!("/__preview_pdf__/{}", pdf_file_name),
        pdf_disk_path: pdf_disk_path.to_string_lossy().to_string(),
        temp_docx_path: temp_docx_path.to_string_lossy().to_string(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        note,
    })
}

pub fn open_preview_in_microsoft_word(
    req: &PreviewBuildRequest,
    target_variant: &str,
) -> Result<String, String> {
    let src_path = Path::new(&req.path);
    if !src_path.is_file() {
        return Err(format!("Không tìm thấy tệp '{}'", req.path));
    }
    let tmp_dir = preview_temp_dir();
    fs::create_dir_all(&tmp_dir)
        .map_err(|e| format!("Không tạo được thư mục tạm xem trước: {}", e))?;
    cleanup_old_temp_previews();

    let stem = src_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("VanBan");
    let target_open = if target_variant == "after" {
        let out_p = tmp_dir.join(format!(
            "{}_XemTruocSau_{}.docx",
            stem,
            chrono::Local::now().timestamp_millis()
        ));
        if let Some(after_raw) = req
            .after_file_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            fs::copy(Path::new(after_raw), &out_p)
                .map_err(|e| format!("Không tạo được bản sao mở Word: {}", e))?;
        } else {
            let (pkg, _, _) = transformer::build_transformed_package_in_memory(src_path, req, None, None)?;
            pkg.save_to(&out_p)?;
        }
        out_p
    } else {
        // Open copy in temp so user never accidentally edits/locks the original input file during visual inspection
        let copy_p = tmp_dir.join(format!(
            "{}_XemTruocGoc_{}.docx",
            stem,
            chrono::Local::now().timestamp_millis()
        ));
        fs::copy(src_path, &copy_p)
            .map_err(|e| format!("Không tạo được bản sao xem trước an toàn: {}", e))?;
        copy_p
    };

    let target_str = target_open.to_string_lossy().to_string();
    Command::new("explorer.exe")
        .arg(&target_open)
        .spawn()
        .map_err(|e| format!("Không thể mở Microsoft Word: {}", e))?;

    Ok(format!(
        "Đã mở bản sao xem trước an toàn trong Microsoft Word: {}",
        target_str
    ))
}
