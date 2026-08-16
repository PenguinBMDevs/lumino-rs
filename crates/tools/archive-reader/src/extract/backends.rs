//! 各后端提取实现（unarc-rs、zip crate、GZ/XZ 内容检测）
//!
//! TAR / ISO 在独立的子模块中。

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{ArchiveEntry, ArchiveError};

// ── unarc-rs 后端 ──────────────────────────────────────────

/// 使用 unarc-rs 列出压缩包条目
pub(super) fn list_entries_unarc(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let mut archive = unarc_rs::unified::ArchiveFormat::open_path(path)
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 打开失败: {e}")))?;

    let mut entries = Vec::new();
    while let Some(entry) = archive
        .next_entry()
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取条目失败: {e}")))?
    {
        let name = entry.name().to_string();
        let is_dir = name.ends_with('/');
        entries.push(ArchiveEntry { name, is_dir });
    }

    Ok(entries)
}

/// 使用 unarc-rs 提取指定条目到内存
pub(super) fn extract_entry_data_unarc(
    path: &Path,
    entry_name: &str,
) -> Result<Vec<u8>, ArchiveError> {
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
            let entry_data = archive
                .read(&entry)
                .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取数据失败: {e}")))?;
            return Ok(entry_data);
        }
    }

    Err(ArchiveError::EntryNotFound(entry_name.to_string()))
}

/// 使用 unarc-rs 批量提取所有文件到目录
pub(super) fn extract_all_unarc(
    path: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>, ArchiveError> {
    let mut archive = unarc_rs::unified::ArchiveFormat::open_path(path)
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 打开失败: {e}")))?;

    let mut extracted = Vec::new();

    while let Some(entry) = archive
        .next_entry()
        .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取条目失败: {e}")))?
    {
        let name = entry.name();
        if name.ends_with('/') {
            let dir_path = output_dir.join(name);
            std::fs::create_dir_all(&dir_path)?;
            continue;
        }

        let entry_data = archive
            .read(&entry)
            .map_err(|e| ArchiveError::LibraryError(format!("unarc-rs 读取数据失败: {e}")))?;

        let entry_path = output_dir.join(name);
        if let Some(parent) = entry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&entry_path, &entry_data)?;
        extracted.push(entry_path);
    }

    Ok(extracted)
}

// ── ZIPX 后端（zip crate，只处理 ZIPX 格式）─────────────

/// 使用 zip crate 列出 ZIPX 文件条目
pub(super) fn list_entries_zip(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ArchiveError::LibraryError(format!("zip 打开失败: {e}")))?;

    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| ArchiveError::LibraryError(format!("zip 读取条目失败: {e}")))?;
        let name = entry.name().to_string();
        let is_dir = name.ends_with('/');
        entries.push(ArchiveEntry { name, is_dir });
    }
    Ok(entries)
}

/// 从 ZIPX 中提取指定条目
pub(super) fn extract_entry_data_zip(
    path: &Path,
    entry_name: &str,
) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ArchiveError::LibraryError(format!("zip 打开失败: {e}")))?;

    let normalized_target = entry_name.replace('\\', "/");

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ArchiveError::LibraryError(format!("zip 读取条目失败: {e}")))?;
        let normalized_name = entry.name().replace('\\', "/");

        if normalized_name == normalized_target
            || normalized_name.ends_with(&format!("/{normalized_target}"))
        {
            let mut entry_data = Vec::new();
            entry
                .read_to_end(&mut entry_data)
                .map_err(|e| ArchiveError::LibraryError(format!("zip 读取数据失败: {e}")))?;
            return Ok(entry_data);
        }
    }

    Err(ArchiveError::EntryNotFound(entry_name.to_string()))
}

/// 解压 ZIPX 中所有文件到目录
pub(super) fn extract_all_zip(
    path: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ArchiveError::LibraryError(format!("zip 打开失败: {e}")))?;

    let mut extracted = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ArchiveError::LibraryError(format!("zip 读取条目失败: {e}")))?;

        let name = entry.name().to_string();
        if name.ends_with('/') {
            let dir_path = output_dir.join(&name);
            std::fs::create_dir_all(dir_path)?;
            continue;
        }

        let entry_path = output_dir.join(&name);
        if let Some(parent) = entry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut entry_data = Vec::new();
        entry
            .read_to_end(&mut entry_data)
            .map_err(|e| ArchiveError::LibraryError(format!("zip 读取数据失败: {e}")))?;
        std::fs::write(&entry_path, &entry_data)?;
        extracted.push(entry_path);
    }
    Ok(extracted)
}

// ── 纯 GZ / 纯 XZ 内容检测 ─────────────────────────────

/// 检查前 4 字节是否为 MIDI 文件魔数
pub(super) fn is_midi_magic_bytes(bytes: &[u8; 4]) -> bool {
    bytes == b"MThd" || bytes == b"RIFF"
}

/// 从文件名中提取适合做 MIDI 条目的名称
pub(super) fn virtual_midi_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("content");
    if crate::is_midi_extension(stem) {
        stem.to_string()
    } else {
        format!("{stem}.mid")
    }
}

/// 列出纯 GZ 文件条目（带内容检测回退）
///
/// 先用 unarc-rs 列出条目（GZ 可打开），如果条目名不带 .mid 扩展名，
/// 则解压检查魔数。如果魔数是 MIDI，创建虚拟条目。
pub(super) fn list_entries_gz_with_content_check(
    path: &Path,
) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    // 首先尝试 unarc-rs 列出条目（GZ 格式可以工作）
    if let Ok(entries) = list_entries_unarc(path)
        && entries.iter().any(|e| crate::is_midi_extension(&e.name))
    {
        return Ok(entries);
    }

    // 回退：解压并检查魔数
    let file = std::fs::File::open(path)?;
    let mut decoder = flate2::read::MultiGzDecoder::new(file);
    let mut magic = [0u8; 4];
    if decoder.read_exact(&mut magic).is_ok() && is_midi_magic_bytes(&magic) {
        let name = virtual_midi_name(path);
        return Ok(vec![ArchiveEntry {
            name,
            is_dir: false,
        }]);
    }

    Ok(Vec::new())
}

/// 列出纯 XZ 文件条目（unarc-rs 不支持，直接用 xz2 解压检查）
pub(super) fn list_entries_xz_with_content_check(
    path: &Path,
) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut decoder = xz2::read::XzDecoder::new(file);
    let mut magic = [0u8; 4];
    if decoder.read_exact(&mut magic).is_ok() && is_midi_magic_bytes(&magic) {
        let name = virtual_midi_name(path);
        return Ok(vec![ArchiveEntry {
            name,
            is_dir: false,
        }]);
    }

    Ok(Vec::new())
}

/// 从纯 GZ 文件中提取数据（单文件压缩，忽略 entry_name）
pub(super) fn extract_entry_data_gz(path: &Path) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut decoder = flate2::read::MultiGzDecoder::new(file);
    let mut decompressed_data = Vec::new();
    decoder
        .read_to_end(&mut decompressed_data)
        .map_err(|e| ArchiveError::LibraryError(format!("GZ 解压失败: {e}")))?;
    Ok(decompressed_data)
}

/// 从纯 XZ 文件中提取数据（单文件压缩，忽略 entry_name）
pub(super) fn extract_entry_data_xz(path: &Path) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    let mut decoder = xz2::read::XzDecoder::new(file);
    let mut decompressed_data = Vec::new();
    decoder
        .read_to_end(&mut decompressed_data)
        .map_err(|e| ArchiveError::LibraryError(format!("XZ 解压失败: {e}")))?;
    Ok(decompressed_data)
}

/// 解压纯 GZ 文件到目录
pub(super) fn extract_all_gz(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let extracted_data = extract_entry_data_gz(path)?;
    let name = virtual_midi_name(path);
    let output_path = output_dir.join(&name);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &extracted_data)?;
    Ok(vec![output_path])
}

/// 解压纯 XZ 文件到目录
pub(super) fn extract_all_xz(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let extracted_data = extract_entry_data_xz(path)?;
    let name = virtual_midi_name(path);
    let output_path = output_dir.join(&name);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &extracted_data)?;
    Ok(vec![output_path])
}
