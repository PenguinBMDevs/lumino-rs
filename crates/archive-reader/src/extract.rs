//! 压缩包解压逻辑
//!
//! 支持以下后端解压引擎：
//! - unarc-rs: ZIP, RAR, 7z, TAR, GZ, XZ, LZH, TGZ (及复合压缩格式)
//! - iso9660: ISO 镜像文件
//! - zpaq_rs: ZPAQ 压缩格式

use std::path::{Path, PathBuf};

use crate::{ArchiveEntry, ArchiveError, format::ArchiveFormat};

// 子模块
mod iso;
mod zpaq;

/// 解压后的条目数据
#[derive(Debug)]
pub struct EntryData {
    /// 条目名称
    pub name: String,
    /// 条目数据（完整字节）
    pub data: Vec<u8>,
}

// ── unarc-rs 后端 ──────────────────────────────────────────

/// 使用 unarc-rs 列出压缩包条目
fn list_entries_unarc(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let mut archive = unarc_rs::unified::ArchiveFormat::open_path(path)
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 打开失败: {e}")))?;

    let mut result = Vec::new();
    while let Some(entry) = archive
        .next_entry()
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取条目失败: {e}")))?
    {
        let name = entry.name().to_string();
        // 以 '/' 结尾的视为目录
        let is_dir = name.ends_with('/');
        result.push(ArchiveEntry { name, is_dir });
    }

    Ok(result)
}

/// 使用 unarc-rs 提取指定条目到内存
fn extract_entry_data_unarc(path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let mut archive = unarc_rs::unified::ArchiveFormat::open_path(path)
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 打开失败: {e}")))?;

    let normalized_target = entry_name.replace('\\', "/").to_ascii_lowercase();

    while let Some(entry) = archive
        .next_entry()
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取条目失败: {e}")))?
    {
        let normalized_name = entry.name().replace('\\', "/").to_ascii_lowercase();

        if normalized_name == normalized_target
            || normalized_name.ends_with(&format!("/{normalized_target}"))
        {
            let data = archive
                .read(&entry)
                .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取数据失败: {e}")))?;
            return Ok(data);
        }
    }

    Err(ArchiveError::EntryNotFound(entry_name.to_string()))
}

/// 使用 unarc-rs 批量提取所有文件到目录
fn extract_all_unarc(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let mut archive = unarc_rs::unified::ArchiveFormat::open_path(path)
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 打开失败: {e}")))?;

    let mut extracted = Vec::new();

    while let Some(entry) = archive
        .next_entry()
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取条目失败: {e}")))?
    {
        let name = entry.name();
        if name.ends_with('/') {
            // 目录条目
            let dir_path = output_dir.join(name);
            std::fs::create_dir_all(&dir_path)?;
            continue;
        }

        let data = archive
            .read(&entry)
            .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取数据失败: {e}")))?;

        let entry_path = output_dir.join(name);
        if let Some(parent) = entry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&entry_path, &data)?;
        extracted.push(entry_path);
    }

    Ok(extracted)
}

// ── 公共 API ───────────────────────────────────────────────

/// 列出压缩包中所有条目
pub fn list_entries(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let format = crate::format::detect_format(path).ok_or_else(|| {
        ArchiveError::UnsupportedFormat(
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("未知")
                .to_string(),
        )
    })?;

    match format {
        ArchiveFormat::Zip
        | ArchiveFormat::Rar
        | ArchiveFormat::SevenZ
        | ArchiveFormat::Tar
        | ArchiveFormat::Gz
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzh => list_entries_unarc(path),
        ArchiveFormat::Iso => iso::list_entries(path),
        ArchiveFormat::Zpaq => zpaq::list_entries(path),
    }
}

/// 提取压缩包中的指定条目到内存
pub fn extract_entry_data(path: &Path, entry_name: &str) -> Result<EntryData, ArchiveError> {
    let format = crate::format::detect_format(path).ok_or_else(|| {
        ArchiveError::UnsupportedFormat(
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("未知")
                .to_string(),
        )
    })?;

    let data = match format {
        ArchiveFormat::Zip
        | ArchiveFormat::Rar
        | ArchiveFormat::SevenZ
        | ArchiveFormat::Tar
        | ArchiveFormat::Gz
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzh => extract_entry_data_unarc(path, entry_name),
        ArchiveFormat::Iso => iso::extract_entry_data(path, entry_name),
        ArchiveFormat::Zpaq => zpaq::extract_entry_data(path, entry_name),
    }?;

    Ok(EntryData {
        name: entry_name.to_string(),
        data,
    })
}

/// 提取压缩包中所有内容到临时目录
///
/// 返回 (TempDir, 提取的文件路径列表)。
/// TempDir 在 drop 时会自动清理。
pub fn extract_all_to_temp(path: &Path) -> Result<(tempfile::TempDir, Vec<PathBuf>), ArchiveError> {
    let temp_dir = tempfile::tempdir()?;
    let files = extract_all_to_dir(path, temp_dir.path())?;
    Ok((temp_dir, files))
}

/// 提取压缩包中所有内容到指定目录
pub fn extract_all_to_dir(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let format = crate::format::detect_format(path).ok_or_else(|| {
        ArchiveError::UnsupportedFormat(
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("未知")
                .to_string(),
        )
    })?;

    std::fs::create_dir_all(output_dir)?;

    match format {
        ArchiveFormat::Zip
        | ArchiveFormat::Rar
        | ArchiveFormat::SevenZ
        | ArchiveFormat::Tar
        | ArchiveFormat::Gz
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzh => extract_all_unarc(path, output_dir),
        ArchiveFormat::Iso => iso::extract_all(path, output_dir),
        ArchiveFormat::Zpaq => zpaq::extract_all(path, output_dir),
    }
}

/// 提取压缩包中的指定条目到目标目录
pub fn extract_entry_to_dir(
    path: &Path,
    entry_name: &str,
    output_dir: &Path,
) -> Result<PathBuf, ArchiveError> {
    let data = extract_entry_data(path, entry_name)?;
    std::fs::create_dir_all(output_dir)?;

    let output_path = output_dir.join(&data.name);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &data.data)?;

    Ok(output_path)
}

/// 提取压缩包中的指定条目到临时目录
///
/// 返回 (TempDir, 文件路径)。TempDir 在 drop 时会自动清理。
pub fn extract_entry_to_temp(
    path: &Path,
    entry_name: &str,
) -> Result<(tempfile::TempDir, PathBuf), ArchiveError> {
    let temp_dir = tempfile::tempdir()?;
    let output_path = extract_entry_to_dir(path, entry_name, temp_dir.path())?;
    Ok((temp_dir, output_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 递归收集目录中的条目（仅供测试）
    fn collect_entries_recursive(
        dir: &Path,
        prefix: &str,
        entries: &mut Vec<ArchiveEntry>,
    ) -> Result<(), ArchiveError> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_string();
                let full_name = if prefix.is_empty() {
                    name_str.clone()
                } else {
                    format!("{prefix}/{name_str}")
                };

                if path.is_dir() {
                    entries.push(ArchiveEntry {
                        name: full_name.clone(),
                        is_dir: true,
                    });
                    collect_entries_recursive(&path, &full_name, entries)?;
                } else {
                    entries.push(ArchiveEntry {
                        name: full_name,
                        is_dir: false,
                    });
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_collect_entries_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("a.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"world").unwrap();

        let mut entries = Vec::new();
        collect_entries_recursive(dir.path(), "", &mut entries).unwrap();
        assert_eq!(entries.len(), 3); // sub dir + a.txt + b.txt
    }
}
