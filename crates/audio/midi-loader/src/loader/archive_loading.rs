//! 压缩包 MIDI 加载支持
//!
//! 提供压缩包自动检测、MIDI 文件扫描和自动解压加载功能。
//! 如果压缩包中只有一个 MIDI 文件，自动解压并加载。
//! 如果多个 MIDI 文件，返回列表给调用方处理。

use std::path::{Path, PathBuf};

/// 已提取的压缩包条目；临时目录随此值销毁而自动清理。
#[derive(Debug)]
pub struct ExtractedArchiveEntry {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl ExtractedArchiveEntry {
    /// 提取后的文件路径；调用方必须同时保留此 guard。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// 压缩包扫描结果
#[derive(Debug)]
pub enum ArchiveLoadResult {
    /// 不是压缩包，直接按常规文件处理
    NotArchive,
    /// 压缩包中没有 MIDI 文件
    NoMidiFiles,
    /// 压缩包中只有一个 MIDI 文件，返回路径及其临时目录 guard。
    SingleMidiFile(ExtractedArchiveEntry),
    /// 压缩包中有多个 MIDI 文件，需要用户选择
    /// 返回文件名列表
    MultipleMidiFiles(Vec<String>),
}

/// 扫描文件：判断是否为压缩包，以及压缩包中的 MIDI 文件情况
///
/// 如果文件不是压缩包，返回 `NotArchive`。
/// 如果是压缩包，扫描其中的 .mid / .midi / .lmpj 文件。
pub fn scan_file_for_midi(path: &Path) -> ArchiveLoadResult {
    use crate::archive::{find_midi_entries, is_archive, is_midi_file};

    // 先检查是否是常规 MIDI 文件
    if is_midi_file(path) {
        return ArchiveLoadResult::NotArchive;
    }

    // 检查是否是压缩包
    if !is_archive(path) {
        return ArchiveLoadResult::NotArchive;
    }

    // 扫描压缩包
    let midi_entries = match find_midi_entries(path) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("扫描压缩包失败: {e}");
            return ArchiveLoadResult::NoMidiFiles;
        }
    };

    if midi_entries.is_empty() {
        return ArchiveLoadResult::NoMidiFiles;
    }

    if midi_entries.len() == 1 {
        // 只有一个 MIDI 文件，自动提取到临时目录并加载
        let entry_name = &midi_entries[0].name;
        match extract_entry_with_tempdir(path, entry_name) {
            Ok(extracted) => ArchiveLoadResult::SingleMidiFile(extracted),
            Err(e) => {
                tracing::warn!("自动提取 MIDI 失败: {e}");
                ArchiveLoadResult::NoMidiFiles
            }
        }
    } else {
        // 多个 MIDI 文件，返回文件名列表让用户选择
        ArchiveLoadResult::MultipleMidiFiles(midi_entries.into_iter().map(|e| e.name).collect())
    }
}

/// 将压缩包中的所有内容提取到临时目录，并返回 TempDir 和 MIDI 文件路径
///
/// 调用方需要保持 TempDir 存活（否则文件会被自动清理）。
pub fn extract_archive_to_temp_dir(
    archive_path: &Path,
) -> Result<(tempfile::TempDir, Vec<PathBuf>), crate::archive::ArchiveError> {
    use crate::archive::extract_all_to_temp;

    let (temp_dir, files) = extract_all_to_temp(archive_path)?;
    Ok((temp_dir, files))
}

/// 从压缩包中提取指定条目，返回持有临时目录生命周期的 guard。
pub fn extract_entry_with_tempdir(
    archive_path: &Path,
    entry_name: &str,
) -> Result<ExtractedArchiveEntry, crate::archive::ArchiveError> {
    use crate::archive::extract_entry_to_dir;
    use tempfile::tempdir;

    let temp_dir = tempdir()?;
    let output_path = extract_entry_to_dir(archive_path, entry_name, temp_dir.path())?;
    Ok(ExtractedArchiveEntry {
        _temp_dir: temp_dir,
        path: output_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_scan_file_not_archive() {
        let result = scan_file_for_midi(&PathBuf::from("test.mid"));
        assert!(matches!(result, ArchiveLoadResult::NotArchive));

        let result = scan_file_for_midi(&PathBuf::from("test.lmpj"));
        assert!(matches!(result, ArchiveLoadResult::NotArchive));

        let result = scan_file_for_midi(&PathBuf::from("test.txt"));
        assert!(matches!(result, ArchiveLoadResult::NotArchive));

        let result = scan_file_for_midi(&PathBuf::from("test"));
        assert!(matches!(result, ArchiveLoadResult::NotArchive));
    }

    #[test]
    fn test_is_midi_extension_integration() {
        assert!(crate::archive::is_midi_file(Path::new("test.mid")));
        assert!(crate::archive::is_midi_file(Path::new("test.midi")));
        assert!(crate::archive::is_midi_file(Path::new("test.lmpj")));
        assert!(!crate::archive::is_midi_file(Path::new("test.zip")));
    }

    #[test]
    fn test_extracted_entry_guard_cleans_temp_dir() {
        use std::io::Write as _;

        let fixture = tempfile::tempdir().expect("创建测试目录应成功");
        let archive_path = fixture.path().join("one.zip");
        let file = std::fs::File::create(&archive_path).expect("创建 ZIP 应成功");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("note.mid", zip::write::SimpleFileOptions::default())
            .expect("创建 ZIP 条目应成功");
        writer.write_all(b"MThd").expect("写入 ZIP 条目应成功");
        writer.finish().expect("完成 ZIP 应成功");

        let result = scan_file_for_midi(&archive_path);
        let ArchiveLoadResult::SingleMidiFile(extracted) = result else {
            panic!("单 MIDI 压缩包应返回提取 guard");
        };
        let temp_root = extracted._temp_dir.path().to_path_buf();
        assert!(extracted.path().exists(), "guard 存活时文件必须可读");

        drop(extracted);

        assert!(!temp_root.exists(), "guard 销毁后必须清理临时目录");
    }
}
