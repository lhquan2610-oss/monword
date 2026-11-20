use crate::engine::reliability::error_code::{format_io_error, AppErrorCode, AppStructuredError};
use crate::engine::reliability::resource_guard::{
    check_compressed_file_size, check_zip_entry_count, read_zip_entry_bounded,
};
use crate::engine::reliability::safe_save::save_zip_archive_atomic;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub compression: CompressionMethod,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ZipPartMeta {
    pub name: String,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
}

#[derive(Debug, Clone)]
pub struct SelectivePackageView {
    pub part_metas: Vec<ZipPartMeta>,
    pub xml_parts: HashMap<String, String>,
}

#[allow(dead_code)]
impl SelectivePackageView {
    pub fn get_xml(&self, name: &str) -> Option<&str> {
        let norm = normalize_entry_name(name);
        if let Some(s) = self.xml_parts.get(&norm) {
            return Some(s.as_str());
        }
        self.xml_parts
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&norm))
            .map(|(_, v)| v.as_str())
    }

    pub fn has_entry(&self, name: &str) -> bool {
        let norm = normalize_entry_name(name);
        self.part_metas
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(&norm))
    }
}

#[derive(Debug, Clone)]
pub struct OpenXmlPackage {
    pub entries: Vec<ArchiveEntry>,
}

const OLE_CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const UTF16_ENCRYPTED_PACKAGE: [u8; 32] = [
    b'E', 0, b'n', 0, b'c', 0, b'r', 0, b'y', 0, b'p', 0, b't', 0, b'e', 0, b'd', 0, b'P', 0,
    b'a', 0, b'c', 0, b'k', 0, b'a', 0, b'g', 0, b'e', 0,
];
const UTF16_ENCRYPTION_INFO: [u8; 28] = [
    b'E', 0, b'n', 0, b'c', 0, b'r', 0, b'y', 0, b'p', 0, b't', 0, b'i', 0, b'o', 0, b'n', 0,
    b'I', 0, b'n', 0, b'f', 0, b'o', 0,
];

/// Validates that a ZIP entry name has no path traversal (`..`), drive prefixes, control characters,
/// or leading absolute/UNC separators.
pub fn validate_zip_entry_name(raw_name: &str) -> Result<String, String> {
    if raw_name.is_empty()
        || raw_name.chars().any(|c| c < ' ')
        || raw_name.contains(':')
        || raw_name.starts_with("//")
        || raw_name.starts_with("\\\\")
        || raw_name
            .split(['/', '\\'])
            .any(|seg| seg == ".." || seg == ".")
    {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            format!(
                "Phát hiện đường dẫn entry ZIP bất thường/không an toàn: '{}'.",
                raw_name
            ),
        )
        .format_user_message());
    }
    let norm = normalize_entry_name(raw_name);
    if norm.is_empty() {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrOpenXmlCorrupt,
            "Phát hiện tên entry ZIP rỗng không hợp lệ.",
        )
        .format_user_message());
    }
    Ok(norm)
}

/// Inspects binary header bytes of a file to distinguish password-encrypted Office files and legacy
/// OLE Compound File Binary containers from plain OpenXML ZIP packages before calling ZIP or Word COM.
pub fn inspect_ole_cfb_bytes(bytes: &[u8], expected_openxml: bool) -> Result<(), String> {
    if bytes.len() < OLE_CFB_MAGIC.len() || bytes[..8] != OLE_CFB_MAGIC {
        return Ok(());
    }

    let has_encrypted_pkg = bytes
        .windows(UTF16_ENCRYPTED_PACKAGE.len())
        .any(|w| w == UTF16_ENCRYPTED_PACKAGE)
        || bytes
            .windows(UTF16_ENCRYPTION_INFO.len())
            .any(|w| w == UTF16_ENCRYPTION_INFO);
    if has_encrypted_pkg {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrFileLockedOrConflict,
            "Tệp được đặt mật khẩu mã hóa (Encrypt with Password). MonWord không tự động giải mã mật khẩu người dùng — hãy mở bằng Microsoft Word, gỡ mật khẩu bảo vệ rồi thử lại.",
        )
        .format_user_message());
    }

    // Check [MS-DOC] FibBase wIdent (0xA5EC / 0xA5DC) at 512-byte sector boundaries for fEncrypted (0x0100) / fObfuscated (0x0800)
    let mut off = 512usize;
    while off + 16 <= bytes.len() {
        let w_ident = u16::from_le_bytes([bytes[off], bytes[off + 1]]);
        if w_ident == 0xA5EC || w_ident == 0xA5DC {
            let flags = u16::from_le_bytes([bytes[off + 10], bytes[off + 11]]);
            if (flags & 0x0100) != 0 || (flags & 0x0800) != 0 {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrFileLockedOrConflict,
                    "Tệp Word (.doc) đang được bảo vệ bằng mật khẩu mở tệp. MonWord không tự động giải mã mật khẩu người dùng — hãy mở bằng Microsoft Word và gỡ mật khẩu trước.",
                )
                .format_user_message());
            }
            break;
        }
        off += 512;
    }

    if expected_openxml {
        return Err(AppStructuredError::new(
            AppErrorCode::ErrDocConvert,
            "Tệp có định dạng nhị phân Office 97-2003 (.doc/.xls) nhưng mang đuôi OpenXML. Hãy đổi đúng đuôi .doc và bật chuyển đổi Word COM hoặc lưu lại thành .docx trong Word.",
        )
        .format_user_message());
    }
    Ok(())
}

/// Inspects up to the first 256 KB of a file on disk for OLE CFB / password-encryption headers.
pub fn inspect_ole_cfb_file(path: &Path, expected_openxml: bool) -> Result<(), String> {
    let f = File::open(path)
        .map_err(|e| format_io_error(&format!("Không thể mở tệp {:?}", path), &e, false))?;
    let mut buf = Vec::with_capacity(64 * 1024);
    f.take(262_144)
        .read_to_end(&mut buf)
        .map_err(|e| format_io_error(&format!("Không thể đọc tiêu đề tệp {:?}", path), &e, false))?;
    inspect_ole_cfb_bytes(&buf, expected_openxml)
}

impl OpenXmlPackage {
    pub fn open(path: &Path) -> Result<Self, String> {
        let meta = fs::metadata(path).map_err(|e| {
            format_io_error(&format!("Không thể truy cập tệp {:?}", path), &e, false)
        })?;
        check_compressed_file_size(meta.len(), Some(path))?;
        inspect_ole_cfb_file(path, true)?;

        let file = File::open(path).map_err(|e| {
            format_io_error(&format!("Không thể mở tệp {:?}", path), &e, false)
        })?;
        let mut archive = ZipArchive::new(file).map_err(|e| {
            AppStructuredError::new(
                AppErrorCode::ErrOpenXmlCorrupt,
                format!("File không phải cấu trúc OpenXML hợp lệ: {}", e),
            )
            .format_user_message()
        })?;
        check_zip_entry_count(archive.len())?;

        let mut entries = Vec::with_capacity(archive.len().min(512));
        let mut seen_lower = HashSet::with_capacity(archive.len().min(512));
        let mut running_uncompressed = 0u64;
        for i in 0..archive.len() {
            let mut item = archive.by_index(i).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrOpenXmlCorrupt,
                    format!("Lỗi đọc entry ZIP #{}: {}", i, e),
                )
                .format_user_message()
            })?;
            if item.is_dir() {
                continue;
            }
            let entry_name = validate_zip_entry_name(item.name())?;
            if !seen_lower.insert(entry_name.to_ascii_lowercase()) {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrOpenXmlCorrupt,
                    format!("Gói ZIP chứa entry trùng lặp bất thường: '{}'.", entry_name),
                )
                .format_user_message());
            }
            let comp_size = item.compressed_size();
            let decl_size = item.size();
            let low = entry_name.to_ascii_lowercase();
            let is_xml_like =
                (low.ends_with(".xml") || low.ends_with(".rels")) && !low.starts_with("word/embeddings/");
            let data = read_zip_entry_bounded(
                &mut item,
                &entry_name,
                comp_size,
                decl_size,
                &mut running_uncompressed,
                is_xml_like,
            )?;
            let compression = match item.compression() {
                CompressionMethod::Stored => CompressionMethod::Stored,
                _ => CompressionMethod::Deflated,
            };
            entries.push(ArchiveEntry {
                name: entry_name,
                data,
                compression,
            });
        }

        Ok(Self { entries })
    }

    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        check_compressed_file_size(bytes.len() as u64, None)?;
        inspect_ole_cfb_bytes(bytes, true)?;
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).map_err(|e| {
            AppStructuredError::new(
                AppErrorCode::ErrOpenXmlCorrupt,
                format!("Dữ liệu không phải OpenXML ZIP: {}", e),
            )
            .format_user_message()
        })?;
        check_zip_entry_count(archive.len())?;

        let mut entries = Vec::with_capacity(archive.len().min(512));
        let mut seen_lower = HashSet::with_capacity(archive.len().min(512));
        let mut running_uncompressed = 0u64;
        for i in 0..archive.len() {
            let mut item = archive.by_index(i).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrOpenXmlCorrupt,
                    format!("Lỗi đọc entry #{}: {}", i, e),
                )
                .format_user_message()
            })?;
            if item.is_dir() {
                continue;
            }
            let entry_name = validate_zip_entry_name(item.name())?;
            if !seen_lower.insert(entry_name.to_ascii_lowercase()) {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrOpenXmlCorrupt,
                    format!("Gói ZIP chứa entry trùng lặp bất thường: '{}'.", entry_name),
                )
                .format_user_message());
            }
            let comp_size = item.compressed_size();
            let decl_size = item.size();
            let low = entry_name.to_ascii_lowercase();
            let is_xml_like =
                (low.ends_with(".xml") || low.ends_with(".rels")) && !low.starts_with("word/embeddings/");
            let data = read_zip_entry_bounded(
                &mut item,
                &entry_name,
                comp_size,
                decl_size,
                &mut running_uncompressed,
                is_xml_like,
            )?;
            let compression = match item.compression() {
                CompressionMethod::Stored => CompressionMethod::Stored,
                _ => CompressionMethod::Deflated,
            };
            entries.push(ArchiveEntry {
                name: entry_name,
                data,
                compression,
            });
        }
        Ok(Self { entries })
    }

    pub fn has_entry(&self, name: &str) -> bool {
        let norm = normalize_entry_name(name);
        self.entries
            .iter()
            .any(|e| normalize_entry_name(&e.name).eq_ignore_ascii_case(&norm))
    }

    pub fn get_bytes(&self, name: &str) -> Option<&[u8]> {
        let norm = normalize_entry_name(name);
        self.entries
            .iter()
            .find(|e| normalize_entry_name(&e.name).eq_ignore_ascii_case(&norm))
            .map(|e| e.data.as_slice())
    }

    pub fn get_xml(&self, name: &str) -> Option<String> {
        self.get_bytes(name)
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s.to_string())
    }

    pub fn set_xml(&mut self, name: &str, content: String) {
        self.set_bytes(name, content.into_bytes(), CompressionMethod::Deflated);
    }

    pub fn set_bytes(&mut self, name: &str, data: Vec<u8>, compression: CompressionMethod) {
        let norm = normalize_entry_name(name);
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| normalize_entry_name(&e.name).eq_ignore_ascii_case(&norm))
        {
            existing.data = data;
            return;
        }
        self.entries.push(ArchiveEntry {
            name: norm,
            data,
            compression,
        });
    }

    pub fn remove_entry(&mut self, name: &str) -> bool {
        let norm = normalize_entry_name(name);
        let before = self.entries.len();
        self.entries
            .retain(|e| !normalize_entry_name(&e.name).eq_ignore_ascii_case(&norm));
        self.entries.len() < before
    }

    pub fn is_word(&self) -> bool {
        self.has_entry("word/document.xml")
    }

    pub fn is_excel(&self) -> bool {
        self.has_entry("xl/workbook.xml")
    }

    pub fn save_to(&self, dest_path: &Path) -> Result<(), String> {
        save_zip_archive_atomic(
            dest_path,
            |tmp_file| {
                let mut writer = ZipWriter::new(tmp_file);
                for entry in &self.entries {
                    let method = if entry.compression == CompressionMethod::Stored {
                        CompressionMethod::Stored
                    } else {
                        CompressionMethod::Deflated
                    };
                    let options = SimpleFileOptions::default().compression_method(method);
                    writer
                        .start_file(&entry.name, options)
                        .map_err(|e| format!("Lỗi ghi ZIP header '{}': {}", entry.name, e))?;
                    writer
                        .write_all(&entry.data)
                        .map_err(|e| format!("Lỗi ghi dữ liệu '{}': {}", entry.name, e))?;
                }
                writer
                    .finish()
                    .map_err(|e| format!("Lỗi hoàn tất đóng gói ZIP: {}", e))?;
                Ok(())
            },
            self.entries.len(),
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            for entry in &self.entries {
                let method = if entry.compression == CompressionMethod::Stored {
                    CompressionMethod::Stored
                } else {
                    CompressionMethod::Deflated
                };
                let options = SimpleFileOptions::default().compression_method(method);
                writer
                    .start_file(&entry.name, options)
                    .map_err(|e| format!("Lỗi ghi ZIP header '{}': {}", entry.name, e))?;
                writer
                    .write_all(&entry.data)
                    .map_err(|e| format!("Lỗi ghi dữ liệu '{}': {}", entry.name, e))?;
            }
            writer
                .finish()
                .map_err(|e| format!("Lỗi hoàn tất đóng gói ZIP: {}", e))?;
        }
        Ok(cursor.into_inner())
    }

    /// Converts an in-memory `OpenXmlPackage` into a `SelectivePackageView` for analysis/validation
    /// without re-reading from disk.
    pub fn to_selective_view(&self) -> Result<SelectivePackageView, String> {
        let mut part_metas = Vec::with_capacity(self.entries.len());
        let mut xml_parts = HashMap::new();

        for entry in &self.entries {
            let norm = normalize_entry_name(&entry.name);
            let lower = norm.to_ascii_lowercase();
            let size = entry.data.len() as u64;
            part_metas.push(ZipPartMeta {
                name: norm.clone(),
                uncompressed_size: size,
                compressed_size: size,
            });

            let need_xml = lower == "[content_types].xml"
                || lower == "_rels/.rels"
                || ((lower.starts_with("word/") || lower.starts_with("docprops/") || lower.starts_with("xl/"))
                    && !lower.starts_with("word/embeddings/")
                    && !lower.starts_with("xl/embeddings/")
                    && (lower.ends_with(".xml") || lower.ends_with(".rels")));

            if need_xml {
                let raw = if entry.data.starts_with(&[0xEF, 0xBB, 0xBF]) {
                    &entry.data[3..]
                } else {
                    entry.data.as_slice()
                };
                let cleaned = std::str::from_utf8(raw)
                    .map_err(|e| format!("Tệp XML '{}' không hợp lệ UTF-8: {}", norm, e))?
                    .to_string();
                xml_parts.insert(norm, cleaned);
            }
        }

        if !xml_parts.keys().any(|k| {
            k.eq_ignore_ascii_case("word/document.xml") || k.eq_ignore_ascii_case("xl/workbook.xml")
        }) {
            return Err("Tài liệu thiếu phần bắt buộc word/document.xml hoặc xl/workbook.xml.".to_string());
        }

        Ok(SelectivePackageView {
            part_metas,
            xml_parts,
        })
    }

    /// Reads only ZIP entry metadata plus essential OpenXML / relationship XML parts into memory,
    /// skipping binary payloads (`word/media/*`, `word/embeddings/*`, `word/vbaProject.bin`) so large files
    /// can be analyzed with minimal RAM overhead.
    pub fn open_selective_for_analysis(path: &Path) -> Result<SelectivePackageView, String> {
        let meta = fs::metadata(path).map_err(|e| {
            format_io_error(&format!("Không thể truy cập tệp {:?}", path), &e, false)
        })?;
        check_compressed_file_size(meta.len(), Some(path))?;
        inspect_ole_cfb_file(path, true)?;

        let file = File::open(path).map_err(|e| {
            format_io_error(&format!("Không thể mở tệp {:?}", path), &e, false)
        })?;
        let mut archive = ZipArchive::new(file).map_err(|e| {
            AppStructuredError::new(
                AppErrorCode::ErrOpenXmlCorrupt,
                format!("File không phải cấu trúc OpenXML ZIP hợp lệ: {}", e),
            )
            .format_user_message()
        })?;
        check_zip_entry_count(archive.len())?;

        let mut part_metas = Vec::with_capacity(archive.len().min(512));
        let mut seen_lower = HashSet::with_capacity(archive.len().min(512));
        let mut xml_parts = HashMap::new();
        let mut running_uncompressed = 0u64;

        for i in 0..archive.len() {
            let mut item = archive.by_index(i).map_err(|e| {
                AppStructuredError::new(
                    AppErrorCode::ErrOpenXmlCorrupt,
                    format!("Lỗi đọc metadata entry ZIP #{}: {}", i, e),
                )
                .format_user_message()
            })?;
            if item.is_dir() {
                continue;
            }
            let norm = validate_zip_entry_name(item.name())?;
            let lower = norm.to_ascii_lowercase();
            if !seen_lower.insert(lower.clone()) {
                return Err(AppStructuredError::new(
                    AppErrorCode::ErrOpenXmlCorrupt,
                    format!("Gói ZIP chứa entry trùng lặp bất thường: '{}'.", norm),
                )
                .format_user_message());
            }
            let comp_size = item.compressed_size();
            let decl_size = item.size();
            crate::engine::reliability::resource_guard::check_compression_ratio(
                &norm,
                decl_size,
                comp_size,
            )?;

            part_metas.push(ZipPartMeta {
                name: norm.clone(),
                uncompressed_size: decl_size,
                compressed_size: comp_size,
            });

            let need_xml = lower == "[content_types].xml"
                || lower == "_rels/.rels"
                || ((lower.starts_with("word/") || lower.starts_with("docprops/") || lower.starts_with("xl/"))
                    && !lower.starts_with("word/embeddings/")
                    && !lower.starts_with("xl/embeddings/")
                    && (lower.ends_with(".xml") || lower.ends_with(".rels")));

            if need_xml {
                let data = read_zip_entry_bounded(
                    &mut item,
                    &norm,
                    comp_size,
                    decl_size,
                    &mut running_uncompressed,
                    true,
                )?;
                let cleaned = if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
                    String::from_utf8(data[3..].to_vec())
                } else {
                    String::from_utf8(data)
                }
                .map_err(|e| {
                    AppStructuredError::new(
                        AppErrorCode::ErrXmlMalformed,
                        format!("Tệp XML '{}' không hợp lệ UTF-8: {}", norm, e),
                    )
                    .format_user_message()
                })?;
                xml_parts.insert(norm, cleaned);
            }
        }

        if !xml_parts
            .keys()
            .any(|k| k.eq_ignore_ascii_case("word/document.xml") || k.eq_ignore_ascii_case("xl/workbook.xml"))
        {
            return Err(AppStructuredError::new(
                AppErrorCode::ErrOpenXmlCorrupt,
                "Tài liệu thiếu phần bắt buộc word/document.xml hoặc xl/workbook.xml.",
            )
            .format_user_message());
        }

        Ok(SelectivePackageView {
            part_metas,
            xml_parts,
        })
    }
}

fn normalize_entry_name(name: &str) -> String {
    name.trim_start_matches('/').replace('\\', "/")
}
