//! ZPAQ 压缩格式解压后端
//!
//! 使用 `zpaq_rs` crate (v1.0.4)。
//!
//! 注意：`archive_read_file_bytes_from_file()` 在 libzpaq 内部的文件名匹配有兼容性问题，
//! 无法正确匹配测试文件中的文件名。因此统一使用 `zpaq_extract` 提取到临时目录后操作。
//!
//! 策略：
//! - 列出条目: `zpaq_list()` 解析 stdout（文件名在 split_whitespace 第 6 个字段，index 5）
//! - 提取文件: 提取到临时目录后读取

use std::path::{Path, PathBuf};

use crate::{ArchiveEntry, ArchiveError};

/// 列出 ZPAQ 压缩包中的条目
pub(super) fn list_entries(path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let path_str = path.to_string_lossy();
    let output = zpaq_rs::zpaq_list(&path_str, &[])
        .map_err(|e| ArchiveError::LibraryError(format!("zpaq_rs list 失败: {e}")))?;

    // zpaq list 实际输出格式：
    //   zpaq v7.15 journaling archiver, compiled 1
    //   archive.zpaq: 1 versions, 1 files, ...
    //   (空行)
    //   - 2021-01-13 04:03:54     16093243 A     Erosoul.mid  <-- 文件条目
    //   (空行)
    //   16.093243 MB of 16.093243 MB (1 files) shown
    //     -> ... after dedupe
    //     -> ... compressed
    //
    // 文件条目以 "- " 开头，split_whitespace() 后：
    //   ["-", "2021-01-13", "04:03:54", "16093243", "A", "Erosoul.mid"]
    // 文件名是最后一个元素（index 5）
    let mut entries = Vec::new();
    for line in output.stdout.lines() {
        let line = line.trim();
        // 文件条目以 "- " 开头
        if !line.starts_with("- ") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        // 标准格式至少 6 个字段：- 日期 时间 大小 标志 文件名
        if parts.len() >= 6 {
            // 文件名是最后一个元素
            if let Some(name) = parts.last()
                && !name.is_empty()
            {
                entries.push(ArchiveEntry {
                    name: name.to_string(),
                    is_dir: false,
                });
            }
        }
    }

    if entries.is_empty() {
        tracing::warn!("zpaq list stdout 解析为空，原始输出:\n{}", output.stdout);
    }

    Ok(entries)
}

/// 从 ZPAQ 压缩包中提取文件数据
///
/// 使用 `zpaq_extract` 提取到临时目录后读取文件内容。
pub(super) fn extract_entry_data(path: &Path, entry_name: &str) -> Result<Vec<u8>, ArchiveError> {
    let temp_dir = tempfile::tempdir()?;
    let output_path = do_extract(path, &[entry_name], temp_dir.path())?;
    let raw_bytes = std::fs::read(&output_path)?;
    Ok(raw_bytes)
    // temp_dir 在函数退出时自动清理
}

/// 批量提取 ZPAQ 压缩包中所有文件到目录
pub(super) fn extract_all(path: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
    let temp_dir = tempfile::tempdir()?;

    // zpaq_extract 提取到当前工作目录，所以先切到 temp_dir
    let old_cwd = std::env::current_dir().map_err(ArchiveError::Io)?;
    std::env::set_current_dir(temp_dir.path()).map_err(ArchiveError::Io)?;

    let path_str = path.to_string_lossy();
    let extract_result = zpaq_rs::zpaq_extract(&path_str, &[]);

    // 无论 extract 成功与否，先恢复 CWD
    let _ = std::env::set_current_dir(&old_cwd);

    let _output = extract_result
        .map_err(|e| ArchiveError::LibraryError(format!("zpaq_rs extract 失败: {e}")))?;

    // 收集提取的文件并移动到 output_dir
    let mut extracted = Vec::new();
    collect_and_move_files(temp_dir.path(), output_dir, &mut extracted)?;

    // 清理临时目录（drop temp_dir）
    Ok(extracted)
}

/// 执行 zpaq extract 到指定目录
fn do_extract(path: &Path, files: &[&str], target_dir: &Path) -> Result<PathBuf, ArchiveError> {
    let old_cwd = std::env::current_dir().map_err(ArchiveError::Io)?;
    std::env::set_current_dir(target_dir).map_err(ArchiveError::Io)?;

    let path_str = path.to_string_lossy();
    let extract_result = zpaq_rs::zpaq_extract(&path_str, files);

    // 恢复 CWD
    let _ = std::env::set_current_dir(&old_cwd);

    extract_result.map_err(|e| ArchiveError::LibraryError(format!("zpaq_rs extract 失败: {e}")))?;

    // 找到第一个提取的文件
    let file_name = files.first().unwrap_or(&"");
    let extracted = target_dir.join(file_name);
    if extracted.exists() {
        return Ok(extracted);
    }

    // 实际提取的文件名可能和请求的不完全一致（如大小写、路径格式）
    // 扫描目录找第一个文件
    for entry in std::fs::read_dir(target_dir).map_err(ArchiveError::Io)? {
        let entry = entry.map_err(ArchiveError::Io)?;
        if entry.file_type().map_err(ArchiveError::Io)?.is_file() {
            return Ok(entry.path());
        }
    }

    Err(ArchiveError::EntryNotFound(file_name.to_string()))
}

/// 递归收集目录中的文件并移动到目标目录
fn collect_and_move_files(
    src: &Path,
    dst: &Path,
    extracted: &mut Vec<PathBuf>,
) -> Result<(), ArchiveError> {
    if !src.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_and_move_files(&path, dst, extracted)?;
        } else {
            let name = entry.file_name();
            let dest_path = dst.join(&name);
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&path, &dest_path)?;
            extracted.push(dest_path);
        }
    }
    Ok(())
}
