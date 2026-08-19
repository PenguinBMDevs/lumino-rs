//! TAR 格式解压后端
//!
//! 使用 `tar` crate 处理 TAR 文件，替代 unarc-rs（后者处理大文件 TAR 条目会陷入死循环）。
//! - .tar：标准 TAR
//! - .tar.gz / .tgz：GZ 压缩的 TAR → `flate2::read::GzDecoder` + `tar::Archive`
//! - .tar.xz / .txz：XZ 压缩的 TAR → `xz2::read::XzDecoder` + `tar::Archive`

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::archive::{ArchiveEntry, ArchiveError};

// ── 内部辅助 ────────────────────────────────────────────────

/// 从已打开的 tar 存档中提取条目名称列表
fn list_from<R: Read>(archive: &mut tar::Archive<R>) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let mut entries = Vec::new();
    let archive_entries = archive
        .entries()
        .map_err(|e| ArchiveError::LibraryError(format!("tar 读取条目失败: {e}")))?;

    for entry in archive_entries {
        let entry =
            entry.map_err(|e| ArchiveError::LibraryError(format!("tar 条目解析失败: {e}")))?;
        let name = entry
            .path()
            .map_err(|e| ArchiveError::LibraryError(format!("tar 路径解析失败: {e}")))?
            .to_string_lossy()
            .to_string();
        let is_dir = entry.header().entry_type().is_dir();
        entries.push(ArchiveEntry { name, is_dir });
    }

    Ok(entries)
}

/// 从已打开的 tar 存档中提取指定文件数据
fn extract_from<R: Read>(
    archive: &mut tar::Archive<R>,
    entry_name: &str,
) -> Result<Vec<u8>, ArchiveError> {
    let normalized_target = entry_name.replace('\\', "/");

    let entries = archive
        .entries()
        .map_err(|e| ArchiveError::LibraryError(format!("tar 读取条目失败: {e}")))?;

    for entry in entries {
        let mut entry =
            entry.map_err(|e| ArchiveError::LibraryError(format!("tar 条目解析失败: {e}")))?;

        let name = entry
            .path()
            .map_err(|e| ArchiveError::LibraryError(format!("tar 路径解析失败: {e}")))?
            .to_string_lossy()
            .to_string();
        let normalized_name = name.replace('\\', "/");

        if normalized_name == normalized_target
            || normalized_name.ends_with(&format!("/{normalized_target}"))
        {
            let mut entry_data = Vec::new();
            entry
                .read_to_end(&mut entry_data)
                .map_err(|e| ArchiveError::LibraryError(format!("tar 读取数据失败: {e}")))?;
            return Ok(entry_data);
        }
    }

    Err(ArchiveError::EntryNotFound(entry_name.to_string()))
}

/// 递归收集解压目录下的所有文件
fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), ArchiveError> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                collect_files(&path, files)?;
            } else {
                files.push(path);
            }
        }
    }
    Ok(())
}

// ── 公共 API（pub(super) 供 extract.rs 调用）───────────────

/// 列出 TAR 文件中的条目（未压缩）
pub(super) fn list_entries(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut archive = tar::Archive::new(file);
    list_from(&mut archive)
}

/// 列出 GZ 压缩 TAR 文件中的条目（.tar.gz / .tgz）
pub(super) fn list_entries_gz(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    list_from(&mut archive)
}

/// 列出 XZ 压缩 TAR 文件中的条目（.tar.xz / .txz）
pub(super) fn list_entries_xz(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    list_from(&mut archive)
}

/// 从 TAR 文件中提取指定条目
pub(super) fn extract_entry_data(path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut archive = tar::Archive::new(file);
    extract_from(&mut archive, entry_name)
}

/// 从 GZ 压缩 TAR 文件中提取指定条目
pub(super) fn extract_entry_data_gz(
    path: &Path,
    entry_name: &str,
) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    extract_from(&mut archive, entry_name)
}

/// 从 XZ 压缩 TAR 文件中提取指定条目
pub(super) fn extract_entry_data_xz(
    path: &Path,
    entry_name: &str,
) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    extract_from(&mut archive, entry_name)
}

/// 提取 TAR 中所有文件到目录
pub(super) fn extract_all(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut archive = tar::Archive::new(file);
    archive
        .unpack(output_dir)
        .map_err(|e| ArchiveError::LibraryError(format!("tar 解压失败: {e}")))?;
    let mut extracted = Vec::new();
    collect_files(output_dir, &mut extracted)?;
    Ok(extracted)
}

/// 提取 GZ 压缩 TAR 中所有文件到目录
pub(super) fn extract_all_gz(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(output_dir)
        .map_err(|e| ArchiveError::LibraryError(format!("tar.gz 解压失败: {e}")))?;
    let mut extracted = Vec::new();
    collect_files(output_dir, &mut extracted)?;
    Ok(extracted)
}

/// 提取 XZ 压缩 TAR 中所有文件到目录
pub(super) fn extract_all_xz(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(output_dir)
        .map_err(|e| ArchiveError::LibraryError(format!("tar.xz 解压失败: {e}")))?;
    let mut extracted = Vec::new();
    collect_files(output_dir, &mut extracted)?;
    Ok(extracted)
}
