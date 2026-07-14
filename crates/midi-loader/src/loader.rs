//! MIDI 加载模块
//!
//! 提供 MIDI 文件的加载功能。
//!
//! 统一使用 cache-only 加载模式：
//! - scan_midi_file 轻量扫描（峰值 < 10MB）
//! - MidiCache::from_notes_file 提取音符并构建分层缓存

use std::path::PathBuf;

// 子模块
mod parsed_midi;
mod types;

// 公开导出
pub use parsed_midi::{load_parsed_midi, load_parsed_midi_from_bytes};
pub use types::{ProgressCallback, progress_from_sender, silent_progress};

/// 加载 MIDI 文件并返回完整解析结果
///
/// 这是便捷函数，内部调用 `load_parsed_midi`
pub async fn load_midi(path: PathBuf) -> crate::LoaderResult<crate::ParsedMidi> {
    load_parsed_midi(path, None).await
}
