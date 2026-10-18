//! GPU 音频导出后端 — 基于 lumino-gpu-synth
//!
//! 提供与 CPU (xsynth) 对等的离线渲染能力，支持通过 `AudioRenderConfig` 选择后端。
//!
//! ## 两条入口，同一个解析链（2026-10-17 审计 §6.2 统一）
//!
//! | 入口 | 数据来源 | gpu-synth 解析入口 |
//! |---|---|---|
//! | `render_audio_gpu_from_document`（内存模式，首选） | `MidiDocument` → `MidiExportData` → 内存 SMF bytes | `GpuSynth::render_midi_bytes` |
//! | `render_audio_gpu_streaming`（文件回退，无文档时） | 调用方 mmap 的同一份 SMF bytes | `GpuSynth::render_midi_bytes` |
//!
//! 两条都走 `render_midi_bytes`：GPU 侧只有一个解析入口。文件回退此前用
//! `GpuSynth::render_midi_file(path)`，会在调用方已 mmap 之后**再读一次磁盘**，
//! 同一次导出把同一个文件解析两遍；现在改为复用调用方持有的字节。
//!
//! 实现按职责拆分（保持各文件 < 400 行，子模块经 `use super::*;` 复用本文件的导入）：
//! - `synth_config`（synth_config.rs）：SynthConfig 与 MidiExportData 构建
//! - `progress`（progress.rs）：暂停/中止检查与渲染进度回调
//! - `render`（render.rs）：GPU 渲染入口与结果写盘
//! - `tests`（tests.rs）：单元测试（`#[cfg(test)]`）

use lumino_midi_loader::MidiDocument;

use crate::error::{ExportError, ExportResult};

use super::config::AudioRenderConfig;
use super::sink_factory::create_output_sink;
use super::speed::{DEFAULT_SPEED_WINDOW_SECS, ExportSpeedMeter};

use super::config;
use progress::{attach_render_progress, check_control};
use synth_config::{build_export_data, build_synth_config};

mod progress;
mod render;
mod synth_config;

#[cfg(test)]
mod tests;

pub use render::gpu_backend_available;
pub use render::is_gpu_available;
pub use render::render_audio_gpu_from_document;
pub use render::render_audio_gpu_streaming;
