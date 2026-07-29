//! ISO 9660 镜像文件解压后端
//!
//! 使用 `iso9660` crate (v0.1.1) 解析 ISO 镜像中的文件系统。
//! API 通过 `ISO9660::new(file)` → `iso.root` → `root.contents()` 遍历目录树，
//! 使用 `ISOFile::read()` → `ISOFileReader`（实现 `Read`）读取文件内容。

use std::io::Read;
use std::path::Path;

use crate::{ArchiveEntry, ArchiveError};

/// 列出 ISO 镜像中的所有条目
///
/// 优先使用 iso9660 crate 解析文件系统。如果解析失败，
/// 回退到在原始 ISO 数据中扫描 MIDI 魔数 "MThd"。
pub(super) fn list_entries(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    match iso9660::ISO9660::new(file) {
        Ok(iso) => {
            let mut result = Vec::new();
            list_directory(&iso.root, "", &mut result)?;
            Ok(result)
        }
        Err(e) => {
            // iso9660 crate 解析失败，回退到原始数据扫描
            tracing::warn!("iso9660 解析失败 ({e})，回退到原始数据扫描");
            fallback_raw_scan(path)
        }
    }
}

/// 回退方案：在原始 ISO 数据中扫描 MIDI 魔数 "MThd"
///
/// ISO 文件系统解析失败时，直接扫描整个文件中的 "MThd" 魔数字节，
/// 如果找到则返回一个虚拟的 MIDI 文件条目。
fn fallback_raw_scan(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // 搜索 "MThd" 魔数（标准 MIDI 文件头）
    let midi_magic = b"MThd";
    for (i, window) in buffer.windows(4).enumerate() {
        if window == midi_magic {
            let name = "content.mid".to_string();
            tracing::info!("ISO 回退扫描: 在偏移 {i} 处发现 MIDI 魔数");
            return Ok(vec![ArchiveEntry {
                name,
                is_dir: false,
            }]);
        }
    }

    tracing::warn!(
        "ISO 回退扫描: 未发现 MIDI 魔数 (扫描 {} 字节)",
        buffer.len()
    );
    Ok(Vec::new())
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
///
/// 优先使用 iso9660 crate。如果文件系统解析失败，
/// 回退到在原始数据中扫描 MIDI 魔数并提取。
pub(super) fn extract_entry_data(path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    match iso9660::ISO9660::new(file) {
        Ok(iso) => {
            let target = entry_name.trim_matches('/');
            match iso.open(target) {
                Ok(Some(iso9660::DirectoryEntry::File(iso_file))) => {
                    let mut reader = iso_file.read();
                    let mut data = Vec::new();
                    reader.read_to_end(&mut data).map_err(|e| {
                        ArchiveError::LibraryError(format!("iso9660 读取文件失败: {e}"))
                    })?;
                    Ok(data)
                }
                Ok(Some(_)) => Err(ArchiveError::EntryNotFound(entry_name.to_string())),
                Ok(None) => Err(ArchiveError::EntryNotFound(entry_name.to_string())),
                Err(e) => Err(ArchiveError::LibraryError(format!("iso9660 查找失败: {e}"))),
            }
        }
        Err(_e) => {
            // iso9660 解析失败，回退到原始数据扫描提取
            tracing::warn!("iso9660 解析失败，回退到原始数据扫描提取");
            fallback_raw_extract(path)
        }
    }
}

/// 回退方案：在原始 ISO 数据中扫描 MIDI 魔数并提取完整 MIDI 数据
///
/// 找到 "MThd" 后尝试解析 MIDI 结构获取完整内容长度。
fn fallback_raw_extract(path: &Path) -> Result<Vec<u8>, ArchiveError> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // 搜索 "MThd" 魔数
    if let Some(offset) = buffer.windows(4).position(|w| w == b"MThd") {
        let data = &buffer[offset..];

        // 验证 MIDI 头：MThd(4) + 长度(4) + 6字节头
        if data.len() < 14 {
            return Err(ArchiveError::EntryNotFound("content".to_string()));
        }

        // MIDI 文件是一个头块后跟多个音轨块
        let mut end = 14; // 跳过 MThd 头

        // 解析所有 MTrk 块
        while end + 8 <= data.len() {
            if &data[end..end + 4] == b"MTrk" {
                let track_len = u32::from_be_bytes([
                    data[end + 4],
                    data[end + 5],
                    data[end + 6],
                    data[end + 7],
                ]) as usize;
                end += 8 + track_len;
            } else {
                break;
            }
        }

        let mut result = data[..end].to_vec();
        // 去除尾部零字节（ISO 填充）
        while result.last() == Some(&0) {
            result.pop();
        }
        Ok(result)
    } else {
        Err(ArchiveError::EntryNotFound("content".to_string()))
    }
}

/// 批量提取 ISO 镜像中所有文件到目录
pub(super) fn extract_all(
    path: &Path,
    output_dir: &Path,
) -> Result<Vec<std::path::PathBuf>, ArchiveError> {
    let file = std::fs::File::open(path)?;
    match iso9660::ISO9660::new(file) {
        Ok(iso) => {
            let mut extracted = Vec::new();
            extract_all_from_dir(&iso.root, "", output_dir, &mut extracted)?;
            Ok(extracted)
        }
        Err(_e) => {
            // 回退：扫描并提取 MIDI
            tracing::warn!("iso9660 解析失败，回退到原始数据提取");
            let data = fallback_raw_extract(path)?;
            let output_path = output_dir.join("content.mid");
            std::fs::create_dir_all(output_dir)?;
            std::fs::write(&output_path, &data)?;
            Ok(vec![output_path])
        }
    }
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
