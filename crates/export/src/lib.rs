//! 文件导出模块
//!
//! 提供 MIDI、工程文件等多种格式的导出能力。
//! 所有导出函数均为同步阻塞调用，适合在后台线程中执行。
//!
//! # 主要入口
//!
//! | 格式 | 函数 | 说明 |
//! |------|------|------|
//! | MIDI | [`export_midi`] / [`export_midi_to_bytes`] | 导出标准 MIDI 文件 |
//! | 工程 | [`save_to_archive`] / [`save_to_folder`] | 保存 Lumino 工程文件 |
//!
//! # 转换器
//!
//! [`converter`] 模块提供格式间同步转换的便捷函数，
//! 如 [`export_midi_from_parsed_midi_sync`] 等。

pub mod audio;
pub mod converter;
pub mod error;
pub mod format;
pub mod midi;
pub mod project;
pub mod video;

// ── 音频渲染 ──

/// 音频编码器类型
pub use audio::codec::AudioCodec;
/// 音频渲染配置
pub use audio::config::AudioRenderConfig;
/// 进度回调类型
pub use audio::config::ProgressCallback;
/// 音频渲染（基于 xsynth）—— 流式模式
pub use audio::render_audio;
/// 音频渲染（基于 xsynth）—— 内存模式
pub use audio::render_audio_from_document;

// ── 视频导出 ──

/// 视频导出模块（基于 FFmpeg）
pub use video::{FfmpegEncoder, VideoExportConfig, VideoExportError};

// ── 格式转换 ──

/// 格式间同步转换工具函数
pub use converter::{copy_file_sync, export_midi_from_parsed_midi_sync};

// ── 错误类型 ──

/// 导出错误类型与 Result 别名
pub use error::{ExportError, ExportResult};

// ── MIDI 导出 ──

/// MIDI 文件导出（写入磁盘）
pub use midi::export_midi;
/// MIDI 文件导出到内存字节流
pub use midi::export_midi_to_bytes;

// ── 工程文件 ──

/// 工程文件核心类型
pub use project::{
    LoadedFileEntry, LoadedFormat, LuminoProject, TrackSlot, TrackVisibilitySer,
    data_formats::{LmctlData, LmnamesData, LmsigData, LmtempData},
    metadata::ProjectMetadata,
    track::{LmtrackData, TrackMeta},
};
/// 从磁盘加载 Lumino 工程文件（兼容旧版 LMPJ）
pub use project::{load_project, project_to_parsed_midi};
/// 保存工程为压缩包/文件夹
pub use project::{save_to_archive, save_to_folder};
