//! MIDI 加载模块
//!
//! 提供 MIDI 文件和 DMS 文件的加载功能。
//!
//! 子模块：
//! - `types`: 进度回调类型定义
//! - `cache`: 缓存相关功能
//! - `midi_info`: MIDI 信息加载
//! - `parsed_midi`: 解析后的 MIDI 加载
//! - `dms`: DMS 文件加载

use std::path::PathBuf;

// 子模块
mod cache;
mod dms;
mod midi_info;
mod parsed_midi;
mod types;

// 公开导出
pub use dms::load_dms;
pub use midi_info::{load_midi_info_with_cache, load_midi_info_with_progress};
pub use parsed_midi::load_parsed_midi;
pub use types::{ProgressCallback, progress_from_sender, silent_progress};

/// 加载 MIDI 文件并返回完整解析结果
///
/// 这是便捷函数，内部调用 `load_parsed_midi`
pub async fn load_midi(path: PathBuf) -> crate::Result<crate::ParsedMidi> {
    load_parsed_midi(path, None).await
}
