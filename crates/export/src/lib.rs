//! 文件导出模块
//!
//! 提供 MIDI、DMS、工程文件等多种格式的导出能力。
//! 所有导出函数均为同步阻塞调用，适合在后台线程中执行。
//!
//! # 主要入口
//!
//! | 格式 | 函数 | 说明 |
//! |------|------|------|
//! | MIDI | [`export_midi`] / [`export_midi_to_bytes`] | 导出标准 MIDI 文件 |
//! | DMS | [`export_dms`] / [`export_dms_to_bytes`] | 导出 DMS (Domino Music Sequencer) 格式 |
//! | 工程 | [`save_to_archive`] / [`save_to_folder`] | 保存 Lumino 工程文件 |
//! | LMPJ | [`save`] / [`save_sync`] | 导出 LMPJ 项目包 |
//!
//! # 转换器
//!
//! [`converter`] 模块提供格式间同步转换的便捷函数，
//! 如 [`export_midi_from_dms_sync`]、[`export_dms_from_midi_sync`] 等。

pub mod audio;
pub mod converter;
pub mod dms;
pub mod error;
pub mod format;
pub mod lmpj;
pub mod midi;
pub mod project;
pub mod video;

// ── 音频渲染 ──

/// 音频渲染（基于 xsynth）—— 流式模式
pub use audio::render_audio;
/// 音频渲染（基于 xsynth）—— 内存模式
pub use audio::render_audio_from_document;

// ── 视频导出 ──

/// 视频导出模块（基于 FFmpeg）
pub use video::{FfmpegEncoder, VideoExportConfig, VideoExportError};

// ── 格式转换 ──

/// 格式间同步转换工具函数
pub use converter::{
    copy_file_sync, export_dms_from_midi_sync, export_midi_from_dms_sync,
    export_midi_from_parsed_midi_sync,
};

// ── DMS 导出 ──

/// DMS 格式导出（简短别名）
pub use dms::export_dms;
/// DMS 格式导出到内存字节流
pub use dms::export_dms_to_bytes;

// ── 错误类型 ──

/// 导出错误类型与 Result 别名
pub use error::{ExportError, ExportResult};

// ── LMPJ 项目包 ──

/// LMPJ 项目包保存（异步版本）
pub use lmpj::save;
/// LMPJ 项目包保存（同步版本）
pub use lmpj::save_sync;

// ── MIDI 导出 ──

/// MIDI 文件导出（写入磁盘）
pub use midi::export_midi;
/// MIDI 文件导出到内存字节流
pub use midi::export_midi_to_bytes;

// ── 工程文件 ──

/// 从磁盘加载 Lumino 工程文件
pub use project::load::load_project;
/// 保存工程为压缩包
pub use project::save::{save_to_archive, save_to_folder};
/// 工程文件核心类型
pub use project::{
    LoadedFileEntry, LoadedFormat, LuminoProject, TrackSlot,
    data_formats::{LmctlData, LmnamesData, LmsigData, LmtempData},
    metadata::ProjectMetadata,
    track::{LmtrackData, TrackMeta, TrackVisibilitySer},
};
