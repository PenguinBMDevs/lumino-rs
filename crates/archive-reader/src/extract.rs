//! 压缩包解压调度
//!
//! 支持以下后端解压引擎：
//! - unarc-rs: ZIP, RAR, 7z, LZH
//! - zip crate: ZIPX（unarc-rs 不支持）
//! - tar crate: TAR, TAR.GZ / TGZ, TAR.XZ / TXZ
//! - flate2 + 内容检测: 纯 GZ 压缩（单文件 MIDI）
//! - xz2 + 内容检测: 纯 XZ 压缩（单文件 MIDI）
//! - iso9660 + 原始扫描回退: ISO 镜像文件

use std::path::{Path, PathBuf};

use crate::{ArchiveEntry, ArchiveError, format::ArchiveFormat};

// 子模块
mod backends;
mod iso;
mod tar;

use backends::{
    extract_all_gz, extract_all_unarc, extract_all_xz, extract_all_zip, extract_entry_data_gz,
    extract_entry_data_unarc, extract_entry_data_xz, extract_entry_data_zip,
    list_entries_gz_with_content_check, list_entries_unarc, list_entries_xz_with_content_check,
    list_entries_zip,
};

/// 解压后的条目数据
#[derive(Debug)]
pub struct EntryData {
    /// 条目名称
    pub name: String,
    /// 条目数据（完整字节）
    pub data: Vec<u8>,
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
        // unarc-rs 处理：RAR, 7z, LZH
        ArchiveFormat::Rar | ArchiveFormat::SevenZ | ArchiveFormat::Lzh => list_entries_unarc(path),
        // ZIP（普通）/ ZIPX：unarc-rs 不支持 ZIPX，用 zip crate 回退
        ArchiveFormat::Zip => {
            let is_zipx = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zipx"));
            if is_zipx {
                list_entries_zip(path)
            } else {
                list_entries_unarc(path)
            }
        }
        // tar crate 处理：TAR, TAR.GZ, TAR.XZ（unarc-rs 会死锁）
        ArchiveFormat::Tar => tar::list_entries(path),
        ArchiveFormat::TarGz => tar::list_entries_gz(path),
        ArchiveFormat::TarXz => tar::list_entries_xz(path),
        // 纯 GZ / 纯 XZ：内容检测
        ArchiveFormat::Gz => list_entries_gz_with_content_check(path),
        ArchiveFormat::Xz => list_entries_xz_with_content_check(path),
        ArchiveFormat::Iso => iso::list_entries(path),
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

    let extracted_data = match format {
        ArchiveFormat::Rar | ArchiveFormat::SevenZ | ArchiveFormat::Lzh => {
            extract_entry_data_unarc(path, entry_name)
        }
        ArchiveFormat::Zip => {
            let is_zipx = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zipx"));
            if is_zipx {
                extract_entry_data_zip(path, entry_name)
            } else {
                extract_entry_data_unarc(path, entry_name)
            }
        }
        ArchiveFormat::Tar => tar::extract_entry_data(path, entry_name),
        ArchiveFormat::TarGz => tar::extract_entry_data_gz(path, entry_name),
        ArchiveFormat::TarXz => tar::extract_entry_data_xz(path, entry_name),
        ArchiveFormat::Gz => extract_entry_data_gz(path),
        ArchiveFormat::Xz => extract_entry_data_xz(path),
        ArchiveFormat::Iso => iso::extract_entry_data(path, entry_name),
    }?;

    Ok(EntryData {
        name: entry_name.to_string(),
        data: extracted_data,
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
        ArchiveFormat::Rar | ArchiveFormat::SevenZ | ArchiveFormat::Lzh => {
            extract_all_unarc(path, output_dir)
        }
        ArchiveFormat::Zip => {
            let is_zipx = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zipx"));
            if is_zipx {
                extract_all_zip(path, output_dir)
            } else {
                extract_all_unarc(path, output_dir)
            }
        }
        ArchiveFormat::Tar => tar::extract_all(path, output_dir),
        ArchiveFormat::TarGz => tar::extract_all_gz(path, output_dir),
        ArchiveFormat::TarXz => tar::extract_all_xz(path, output_dir),
        ArchiveFormat::Gz => extract_all_gz(path, output_dir),
        ArchiveFormat::Xz => extract_all_xz(path, output_dir),
        ArchiveFormat::Iso => iso::extract_all(path, output_dir),
    }
}

/// 提取压缩包中的指定条目到目标目录
pub fn extract_entry_to_dir(
    path: &Path,
    entry_name: &str,
    output_dir: &Path,
) -> Result<PathBuf, ArchiveError> {
    let extracted_data = extract_entry_data(path, entry_name)?;
    std::fs::create_dir_all(output_dir)?;

    let output_path = output_dir.join(&extracted_data.name);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &extracted_data.data)?;

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
        let dir = tempfile::tempdir().expect("临时目录应创建成功");
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).expect("子目录应创建成功");
        std::fs::write(sub.join("a.txt"), b"hello").expect("写入测试文件应成功");
        std::fs::write(dir.path().join("b.txt"), b"world").expect("写入测试文件应成功");

        let mut entries = Vec::new();
        collect_entries_recursive(dir.path(), "", &mut entries).expect("递归收集条目应成功");
        assert_eq!(entries.len(), 3); // sub dir + a.txt + b.txt
    }
}
