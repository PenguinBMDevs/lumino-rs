//! ISO 9660 镜像文件解压后端
//!
//! 使用 `iso9660` crate (v0.1.1) 解析 ISO 镜像中的文件系统。
//! API 通过 `ISO9660::new(file)` → `iso.root` → `root.contents()` 遍历目录树，
//! 使用 `ISOFile::read()` → `ISOFileReader`（实现 `Read`）读取文件内容。

use std::io::Read;
use std::path::Path;

use crate::{ArchiveEntry, ArchiveError};

/// 列出 ISO 镜像中的所有条目
pub(super) fn list_entries(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let iso = iso9660::ISO9660::new(file)
        .map_err(|e| ArchiveError::LibraryError(format!("iso9660 解析失败: {e}")))?;

    let mut result = Vec::new();
    list_directory(&iso.root, "", &mut result)?;
    Ok(result)
}

/// 递归遍历 ISO 目录，收集所有条目
fn list_directory<T: iso9660::ISO9660Reader>(
    dir: &iso9660::ISODirectory<T>,
    prefix: &str,
    entries: &mut Vec<ArchiveEntry>,
) -> Result<(), ArchiveError> {
    for entry_result in dir.contents() {
        let entry = entry_result
            .map_err(|e| ArchiveError::LibraryError(format!("iso9660 读取条目失败: {e}")))?;

        let name = entry.identifier().to_string();
        let full_name = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };

        match entry {
            iso9660::DirectoryEntry::Directory(d) => {
                entries.push(ArchiveEntry {
                    name: full_name.clone(),
                    is_dir: true,
                });
                // 跳过 "."（当前目录）和 ".."（上级目录），避免无限递归
                if d.identifier != "." && d.identifier != ".." {
                    list_directory(&d, &full_name, entries)?;
                }
            }
            iso9660::DirectoryEntry::File(_f) => {
                entries.push(ArchiveEntry {
                    name: full_name,
                    is_dir: false,
                });
            }
        }
    }
    Ok(())
}

/// 从 ISO 镜像中提取文件数据
pub(super) fn extract_entry_data(path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let iso = iso9660::ISO9660::new(file)
        .map_err(|e| ArchiveError::LibraryError(format!("iso9660 解析失败: {e}")))?;

    let target = entry_name.trim_matches('/');

    // iso.open() 可从根目录按路径查找（内部递归遍历目录调用 find）
    match iso.open(target) {
        Ok(Some(iso9660::DirectoryEntry::File(iso_file))) => {
            let mut reader = iso_file.read();
            let mut data = Vec::new();
            reader
                .read_to_end(&mut data)
                .map_err(|e| ArchiveError::LibraryError(format!("iso9660 读取文件失败: {e}")))?;
            Ok(data)
        }
        Ok(Some(_)) => Err(ArchiveError::EntryNotFound(entry_name.to_string())),
        Ok(None) => Err(ArchiveError::EntryNotFound(entry_name.to_string())),
        Err(e) => Err(ArchiveError::LibraryError(format!("iso9660 查找失败: {e}"))),
    }
}

/// 批量提取 ISO 镜像中所有文件到目录
pub(super) fn extract_all(
    path: &Path,
    output_dir: &Path,
) -> Result<Vec<std::path::PathBuf>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let iso = iso9660::ISO9660::new(file)
        .map_err(|e| ArchiveError::LibraryError(format!("iso9660 解析失败: {e}")))?;

    let mut extracted = Vec::new();
    extract_all_from_dir(&iso.root, "", output_dir, &mut extracted)?;
    Ok(extracted)
}

/// 递归提取 ISO 目录中的所有文件
fn extract_all_from_dir<T: iso9660::ISO9660Reader>(
    dir: &iso9660::ISODirectory<T>,
    prefix: &str,
    output_dir: &std::path::Path,
    extracted: &mut Vec<std::path::PathBuf>,
) -> Result<(), ArchiveError> {
    for entry_result in dir.contents() {
        let entry = entry_result
            .map_err(|e| ArchiveError::LibraryError(format!("iso9660 读取条目失败: {e}")))?;

        let name = entry.identifier().to_string();
        let full_name = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };

        match entry {
            iso9660::DirectoryEntry::Directory(d) => {
                // 跳过 "." 和 ".."
                if d.identifier != "." && d.identifier != ".." {
                    extract_all_from_dir(&d, &full_name, output_dir, extracted)?;
                }
            }
            iso9660::DirectoryEntry::File(f) => {
                let mut reader = f.read();
                let mut data = Vec::new();
                reader.read_to_end(&mut data).map_err(|e| {
                    ArchiveError::LibraryError(format!("iso9660 读取文件失败: {e}"))
                })?;

                let entry_path = output_dir.join(&full_name);
                if let Some(parent) = entry_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&entry_path, &data)?;
                extracted.push(entry_path);
            }
        }
    }
    Ok(())
}
