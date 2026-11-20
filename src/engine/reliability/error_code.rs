use serde::{Deserialize, Serialize};
use std::io::ErrorKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppErrorCode {
    ErrFileRead,
    ErrFileWrite,
    ErrFileLockedOrConflict,
    ErrOpenXmlCorrupt,
    ErrXmlMalformed,
    ErrWordCom,
    ErrComTimeout,
    ErrIpcInvalid,
    ErrResourceLimit,
    ErrZipBomb,
    ErrRenderPreview,
    ErrDocConvert,
    ErrPostSaveVerify,
    ErrBackupInvalid,
    ErrTaskCancelled,
}

impl AppErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ErrFileRead => "ERR_FILE_READ",
            Self::ErrFileWrite => "ERR_FILE_WRITE",
            Self::ErrFileLockedOrConflict => "ERR_FILE_LOCKED_OR_CONFLICT",
            Self::ErrOpenXmlCorrupt => "ERR_OPENXML_CORRUPT",
            Self::ErrXmlMalformed => "ERR_XML_MALFORMED",
            Self::ErrWordCom => "ERR_WORD_COM",
            Self::ErrComTimeout => "ERR_COM_TIMEOUT",
            Self::ErrIpcInvalid => "ERR_IPC_INVALID",
            Self::ErrResourceLimit => "ERR_RESOURCE_LIMIT",
            Self::ErrZipBomb => "ERR_ZIP_BOMB",
            Self::ErrRenderPreview => "ERR_RENDER_PREVIEW",
            Self::ErrDocConvert => "ERR_DOC_CONVERT",
            Self::ErrPostSaveVerify => "ERR_POST_SAVE_VERIFY",
            Self::ErrBackupInvalid => "ERR_BACKUP_INVALID",
            Self::ErrTaskCancelled => "ERR_TASK_CANCELLED",
        }
    }

    pub fn default_remediation_vi(&self) -> &'static str {
        match self {
            Self::ErrFileRead => {
                "Kiểm tra tệp có tồn tại, đúng quyền đọc và không nằm trên ổ đĩa/mạng đã ngắt kết nối."
            }
            Self::ErrFileWrite => {
                "Kiểm tra quyền ghi thư mục đích, dung lượng ổ đĩa còn trống và thử lưu ở chế độ Tạo bản sao."
            }
            Self::ErrFileLockedOrConflict => {
                "Đóng tệp đang mở trong Microsoft Word/Excel hoặc chọn chế độ Tạo bản sao mới."
            }
            Self::ErrOpenXmlCorrupt => {
                "Tệp không phải gói OpenXML/ZIP hợp lệ hoặc bị hỏng. Hãy mở bằng Microsoft Word và dùng Open & Repair."
            }
            Self::ErrXmlMalformed => {
                "Cấu trúc XML bên trong tài liệu bị sai cú pháp. Hãy lưu lại tệp bằng Word (.docx) trước khi xử lý."
            }
            Self::ErrWordCom => {
                "Đảm bảo Microsoft Word Desktop đã cài đặt, đóng các hộp thoại Word đang treo rồi thử lại."
            }
            Self::ErrComTimeout => {
                "Microsoft Word phản hồi quá lâu và đã được thu hồi an toàn. Mở kiểm tra trực tiếp tệp gốc bằng Word."
            }
            Self::ErrIpcInvalid => {
                "Tham số gửi từ giao diện chưa đầy đủ hoặc không hợp lệ. Tải lại cửa sổ hoặc thử thao tác lại."
            }
            Self::ErrResourceLimit => {
                "Tệp vượt ngưỡng bảo vệ dung lượng/phần tử XML. Hãy chia nhỏ tài liệu hoặc bỏ bớt ảnh dung lượng quá lớn."
            }
            Self::ErrZipBomb => {
                "Tệp có tỷ lệ nén ZIP bất thường và bị chặn để bảo vệ RAM. Kiểm tra lại nguồn gốc tệp trước khi mở."
            }
            Self::ErrRenderPreview => {
                "Không thể dựng xem trước A4 cho trang này. Thử giảm độ phức tạp hoặc mở đối chiếu bằng Word."
            }
            Self::ErrDocConvert => {
                "Chuyển đổi định dạng (.doc → .docx hoặc xuất .pdf) thất bại. Kiểm tra Word COM và đường dẫn đầu ra."
            }
            Self::ErrPostSaveVerify => {
                "Kiểm tra hậu lưu phát hiện tệp đích chưa đạt chuẩn hoặc thiếu thành phần bảo toàn. Tệp gốc đã được bảo vệ/hoàn tác."
            }
            Self::ErrBackupInvalid => {
                "Bản sao lưu bị thiếu, rỗng hoặc sai mã băm toàn vẹn nên bị từ chối khôi phục để bảo vệ dữ liệu hiện tại."
            }
            Self::ErrTaskCancelled => {
                "Tác vụ đã dừng theo yêu cầu người dùng. Các tệp chưa xử lý được giữ nguyên trạng."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStructuredError {
    pub code: AppErrorCode,
    pub cause: String,
    pub remediation: String,
}

impl AppStructuredError {
    pub fn new(code: AppErrorCode, cause: impl Into<String>) -> Self {
        Self {
            code,
            cause: cause.into(),
            remediation: code.default_remediation_vi().to_string(),
        }
    }

    pub fn format_user_message(&self) -> String {
        let trimmed = self.cause.trim();
        let clean_cause = if let Some(rest) = strip_existing_code_prefix(trimmed) {
            rest
        } else {
            trimmed
        };
        if clean_cause.contains("Hướng xử lý:") {
            format!("[{}] {}", self.code.as_str(), clean_cause)
        } else {
            format!(
                "[{}] {} — Hướng xử lý: {}",
                self.code.as_str(),
                clean_cause,
                self.remediation
            )
        }
    }
}

pub fn format_io_error(path_desc: &str, err: &std::io::Error, is_write: bool) -> String {
    let code = if is_locked_or_conflict_io(err) {
        AppErrorCode::ErrFileLockedOrConflict
    } else if is_write {
        AppErrorCode::ErrFileWrite
    } else {
        AppErrorCode::ErrFileRead
    };
    AppStructuredError::new(code, format!("{}: {}", path_desc, err)).format_user_message()
}

pub fn is_locked_or_conflict_io(err: &std::io::Error) -> bool {
    if err.kind() == ErrorKind::PermissionDenied || err.kind() == ErrorKind::AlreadyExists {
        return true;
    }
    matches!(err.raw_os_error(), Some(32) | Some(33) | Some(5) | Some(183))
}

pub fn classify_error_message(raw: &str, fallback: AppErrorCode) -> AppStructuredError {
    let trimmed = raw.trim();
    if let Some((existing_code, rest)) = parse_existing_code(trimmed) {
        return AppStructuredError::new(existing_code, rest);
    }

    let lower = trimmed.to_ascii_lowercase();
    let code = if lower.contains("zip_bomb") || lower.contains("tỷ lệ nén") {
        AppErrorCode::ErrZipBomb
    } else if lower.contains("com_timeout") || lower.contains("quá thời gian") || lower.contains("timed out") {
        AppErrorCode::ErrComTimeout
    } else if lower.contains("cancelled") || lower.contains("đã hủy") || lower.contains("hủy tác vụ") {
        AppErrorCode::ErrTaskCancelled
    } else if lower.contains("sao lưu") && (lower.contains("hỏng") || lower.contains("sha") || lower.contains("không tồn tại") || lower.contains("không hợp lệ")) {
        AppErrorCode::ErrBackupInvalid
    } else if lower.contains("vượt giới hạn") || lower.contains("vượt ngưỡng") || lower.contains("độ sâu xml") || lower.contains("số phần tử xml") {
        AppErrorCode::ErrResourceLimit
    } else if lower.contains("bảo toàn") || lower.contains("hậu lưu") || lower.contains("xác minh sau lưu") {
        AppErrorCode::ErrPostSaveVerify
    } else if lower.contains("đang mở")
        || lower.contains("sharing violation")
        || lower.contains("os error 32")
        || lower.contains("os error 33")
        || lower.contains("access is denied")
        || lower.contains("os error 5")
        || lower.contains("khóa")
    {
        AppErrorCode::ErrFileLockedOrConflict
    } else if lower.contains("xml") && (lower.contains("cú pháp") || lower.contains("parse") || lower.contains("giải mã") || lower.contains("không có phần tử gốc")) {
        AppErrorCode::ErrXmlMalformed
    } else if lower.contains("zip") || lower.contains("openxml") || lower.contains("thiếu phần bắt buộc") || lower.contains("word/document.xml") {
        AppErrorCode::ErrOpenXmlCorrupt
    } else if lower.contains("xuất kèm pdf") || lower.contains("chuyển đổi") || lower.contains(".doc ->") || lower.contains("exportasfixedformat") {
        AppErrorCode::ErrDocConvert
    } else if lower.contains("word com") || lower.contains("com worker") || lower.contains("winword") {
        AppErrorCode::ErrWordCom
    } else if lower.contains("ipc") || lower.contains("vui lòng chọn") || lower.contains("chưa chọn") {
        AppErrorCode::ErrIpcInvalid
    } else if lower.contains("xem trước") || lower.contains("preview") {
        AppErrorCode::ErrRenderPreview
    } else if lower.contains("không tìm thấy tệp") || lower.contains("không thể mở") || lower.contains("không đọc được") {
        AppErrorCode::ErrFileRead
    } else if lower.contains("không thể ghi") || lower.contains("không thể lưu") || lower.contains("không thể đổi tên") {
        AppErrorCode::ErrFileWrite
    } else {
        fallback
    };

    AppStructuredError::new(code, trimmed)
}

fn strip_existing_code_prefix(s: &str) -> Option<&str> {
    parse_existing_code(s).map(|(_, rest)| rest)
}

fn parse_existing_code(s: &str) -> Option<(AppErrorCode, &str)> {
    if !s.starts_with("[ERR_") {
        return None;
    }
    let end = s.find(']')?;
    let tag = &s[1..end];
    let rest = s[end + 1..].trim_start();
    let code = match tag {
        "ERR_FILE_READ" => AppErrorCode::ErrFileRead,
        "ERR_FILE_WRITE" => AppErrorCode::ErrFileWrite,
        "ERR_FILE_LOCKED_OR_CONFLICT" => AppErrorCode::ErrFileLockedOrConflict,
        "ERR_OPENXML_CORRUPT" => AppErrorCode::ErrOpenXmlCorrupt,
        "ERR_XML_MALFORMED" => AppErrorCode::ErrXmlMalformed,
        "ERR_WORD_COM" => AppErrorCode::ErrWordCom,
        "ERR_COM_TIMEOUT" => AppErrorCode::ErrComTimeout,
        "ERR_IPC_INVALID" => AppErrorCode::ErrIpcInvalid,
        "ERR_RESOURCE_LIMIT" => AppErrorCode::ErrResourceLimit,
        "ERR_ZIP_BOMB" => AppErrorCode::ErrZipBomb,
        "ERR_RENDER_PREVIEW" => AppErrorCode::ErrRenderPreview,
        "ERR_DOC_CONVERT" => AppErrorCode::ErrDocConvert,
        "ERR_POST_SAVE_VERIFY" => AppErrorCode::ErrPostSaveVerify,
        "ERR_BACKUP_INVALID" => AppErrorCode::ErrBackupInvalid,
        "ERR_TASK_CANCELLED" => AppErrorCode::ErrTaskCancelled,
        _ => return None,
    };
    Some((code, rest))
}
