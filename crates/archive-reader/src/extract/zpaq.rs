//! ZPAQ 压缩格式解压后端
//!
//! 使用 `zpaq_rs` crate (v1.0.4)。
//! - 列出条目: `zpaq_list()` 解析 stdout
//! - 提取文件: `archive_read_file_bytes_from_file()`

use std::path::Path;

use crate::{ArchiveEntry, ArchiveError};

/// 列出 ZPAQ 压缩包中的条目
pub(super) fn list_entries(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let path_str = path.to_string_lossy();
    let output = zpaq_rs::zpaq_list(&path_str, &[])
        .map_err(|e| ArchiveError::LibraryError(format!("zpaq_rs list 失败: {e}")))?;

    // zpaq list 输出格式：
    //   版本  文件名  大小  日期  时间  CRC
    // 第 0 行: 表头，第 1+ 行: 文件条目
    let mut result = Vec::new();
    for line in output.stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // 格式: "版本 文件名 大小 日期 时间 CRC"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1].to_string();
            if !name.is_empty() {
                result.push(ArchiveEntry {
                    name,
                    is_dir: false,
                });
            }
        }
    }

    Ok(result)
}

/// 从 ZPAQ 压缩包中提取文件数据
pub(super) fn extract_entry_data(path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let path_str = path.to_string_lossy();
    let data = zpaq_rs::archive_read_file_bytes_from_file(&path_str, entry_name)
        .map_err(|e| ArchiveError::LibraryError(format!("zpaq_rs 读取文件失败: {e}")))?;
    Ok(data)
}

/// 批量提取 ZPAQ 压缩包中所有文件到目录
pub(super) fn extract_all(
    path: &Path,
    output_dir: &Path,
) -> Result<Vec<std::path::PathBuf>, ArchiveError> {
    let entries = list_entries(path)?;
    let mut extracted = Vec::new();

    for entry in &entries {
        if entry.is_dir {
            continue;
        }

        let data = extract_entry_data(path, &entry.name)?;
        let entry_path = output_dir.join(&entry.name);
        if let Some(parent) = entry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&entry_path, &data)?;
        extracted.push(entry_path);
    }

    Ok(extracted)
}
