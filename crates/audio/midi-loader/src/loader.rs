//! MIDI 加载模块
//!
//! 提供 MIDI 文件和压缩包 MIDI 的加载功能。
//!
//! 加载主链路（`load_parsed_midi`）：
//! - `.mid` / `.midi` → `mmap_file`（只读映射 + `MidiDocument::from_notes_bytes`），
//!   映射失败自动回退整份读取；原始文件字节不驻留，文档常驻内存
//! - `.lmpj` → zstd 解压 + bincode 反序列化，再从中重建 `MidiDocument`

use std::path::PathBuf;

// 子模块
pub mod archive_loading;
mod mmap_file;
mod parsed_midi;
mod types;

// 公开导出
pub use archive_loading::{
    ArchiveLoadResult, extract_and_get_path, extract_archive_to_temp_dir,
    extract_entry_with_tempdir, scan_file_for_midi,
};
pub use parsed_midi::{load_parsed_midi, load_parsed_midi_from_bytes};
pub use types::{ProgressCallback, progress_from_sender, silent_progress};

/// 加载 MIDI 文件并返回完整解析结果
///
/// 这是便捷函数，内部调用 `load_parsed_midi`
pub async fn load_midi(path: PathBuf) -> crate::LoaderResult<crate::ParsedMidi> {
    load_parsed_midi(path, None).await
}
