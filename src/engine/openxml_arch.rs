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

#[derive(Debug, Clone)]
pub struct OpenXmlPackage {
    pub entries: Vec<ArchiveEntry>,
}

impl OpenXmlPackage {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Không thể mở file {:?}: {}", path, e))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("File không phải cấu trúc OpenXML hợp lệ: {}", e))?;

        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let mut item = archive
                .by_index(i)
                .map_err(|e| format!("Lỗi đọc entry ZIP #{}: {}", i, e))?;
            if item.is_dir() {
                continue;
            }
            let mut data = Vec::with_capacity(item.size() as usize);
            item.read_to_end(&mut data)
                .map_err(|e| format!("Lỗi giải nén '{}': {}", item.name(), e))?;
            let compression = match item.compression() {
                CompressionMethod::Stored => CompressionMethod::Stored,
                _ => CompressionMethod::Deflated,
            };
            entries.push(ArchiveEntry {
                name: item.name().to_string(),
                data,
                compression,
            });
        }

        Ok(Self { entries })
    }

    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive =
            ZipArchive::new(cursor).map_err(|e| format!("Dữ liệu không phải OpenXML ZIP: {}", e))?;
        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let mut item = archive
                .by_index(i)
                .map_err(|e| format!("Lỗi đọc entry #{}: {}", i, e))?;
            if item.is_dir() {
                continue;
            }
            let mut data = Vec::with_capacity(item.size() as usize);
            item.read_to_end(&mut data)
                .map_err(|e| format!("Lỗi đọc dữ liệu '{}': {}", item.name(), e))?;
            let compression = match item.compression() {
                CompressionMethod::Stored => CompressionMethod::Stored,
                _ => CompressionMethod::Deflated,
            };
            entries.push(ArchiveEntry {
                name: item.name().to_string(),
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
        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Không thể tạo thư mục lưu {:?}: {}", parent, e))?;
            }
        }

        let file_name = dest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output.docx");
        let tmp_path = dest_path.with_file_name(format!(".{}.{}.tmp", file_name, std::process::id()));

        {
            let tmp_file = File::create(&tmp_path)
                .map_err(|e| format!("Không thể tạo file tạm {:?}: {}", tmp_path, e))?;
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
        }

        if dest_path.exists() {
            fs::remove_file(dest_path)
                .map_err(|e| format!("Không thể ghi đè file đích (có thể file đang mở trong Word/Excel): {}", e))?;
        }
        fs::rename(&tmp_path, dest_path)
            .map_err(|e| format!("Không thể lưu file tới {:?}: {}", dest_path, e))?;

        Ok(())
    }
}

fn normalize_entry_name(name: &str) -> String {
    name.trim_start_matches('/').replace('\\', "/")
}
