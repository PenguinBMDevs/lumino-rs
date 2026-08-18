//! 压缩包内 MIDI 文件自动解压读取器
//!
//! 支持多种压缩格式的解压和 MIDI 文件扫描：
//! - ZIP / ZIPX (unarc-rs)
//! - RAR (unarc-rs)
//! - 7z (unarc-rs)
//! - TAR / TGZ / TAR.GZ (unarc-rs)
//! - GZ (unarc-rs)
//! - XZ (unarc-rs)
//! - LZH (unarc-rs)
//! - ISO 9660 (iso9660)
//!
//! 原 `lumino-archive-reader` crate 的内容已并入 `lumino-midi-loader`。

mod extract;
mod format;

use std::path::Path;

pub use extract::{EntryData, extract_all_to_temp, extract_entry_to_dir, extract_entry_to_temp};
pub use format::{ArchiveFormat, detect_format, is_archive, is_midi_extension};

/// 压缩包内条目信息
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// 条目名称（相对路径）
    pub name: String,
    /// 是否为目录
    pub is_dir: bool,
}

/// 错误类型
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// 不支持的压缩格式
    #[error("不支持的压缩格式: {0}")]
    UnsupportedFormat(String),

    /// 文件不是有效的压缩包
    #[error("文件不是有效的压缩包: {0}")]
    NotAnArchive(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 解压错误
    #[error("解压失败: {0}")]
    Extraction(String),

    /// 条目未找到
    #[error("压缩包中未找到条目: {0}")]
    EntryNotFound(String),

    /// 内部解压库错误
    #[error("解压库错误: {0}")]
    LibraryError(String),
}

/// 扫描压缩包中的文件列表
///
/// 返回压缩包中的所有条目（文件和目录）。
pub fn scan_archive(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let _format = detect_format(path).ok_or_else(|| {
        ArchiveError::UnsupportedFormat(
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("未知")
                .to_string(),
        )
    })?;

    extract::list_entries(path)
}

/// 在压缩包中查找所有 MIDI 文件条目
///
/// 返回压缩包中所有扩展名为 .mid / .midi / .lmpj 的条目。
pub fn find_midi_entries(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let entries = scan_archive(path)?;
    Ok(entries
        .into_iter()
        .filter(|e| !e.is_dir && is_midi_extension(&e.name))
        .collect())
}

/// 创建临时目录（程序关闭时自动清理）
///
/// 返回的 TempDir 会在 drop 时自动删除。
pub fn create_temp_dir() -> Result<tempfile::TempDir, ArchiveError> {
    Ok(tempfile::tempdir()?)
}

/// 验证文件是否为有效的 MIDI 文件（扩展名检查）
///
/// 允许的扩展名: .mid, .midi, .lmpj
pub fn is_midi_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(is_midi_extension_inner)
        .unwrap_or(false)
}

fn is_midi_extension_inner(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "mid" | "midi" | "lmpj")
}

/// 检查文件是否为支持加载的格式（MIDI 或压缩包）
pub fn is_supported_file(path: &Path) -> bool {
    is_midi_file(path) || is_archive(path)
}
