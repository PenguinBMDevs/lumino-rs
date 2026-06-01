//! 音频导出功能
//!
//! 使用 xsynth-core 将 MIDI 文件渲染为 WAV/FLAC 音频文件。

// 子模块声明——扁平结构，不嵌套子目录
mod compact;
mod exporter;
mod smf;
mod stream;
mod tempo;
mod types;
mod writer;

#[cfg(test)]
mod tests;

// 公共类型重导出（保持向后兼容）
pub use compact::export_audio_from_parsed;
pub use exporter::AudioExporter;
pub use stream::{export_audio, export_audio_from_bytes};
pub use types::*;
pub use writer::AudioFileWriter;

/// MIDI 事件解析器
///
/// 零尺寸类型，用作 MIDI 事件处理方法的命名空间。
/// impl 块分布在子模块中：
/// - `smf.rs`: SMF 渲染路径（render_smf, setup_and_render, parse_and_render）
/// - `compact.rs`: CompactEvent 渲染路径（render_compact_events）
pub struct MidiEventParser;
