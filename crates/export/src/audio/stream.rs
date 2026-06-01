//! 音频导出入口函数——从文件路径/字节流导出

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::error::{ExportError, ExportResult};

use super::MidiEventParser;
use super::types::AudioExportOptions;

/// 导出音频文件（从文件路径）
///
/// # 参数
/// - `midi_path`: MIDI 文件路径
/// - `soundfont_path`: SF2 音色库路径
/// - `output_path`: 输出音频文件路径
/// - `options`: 导出选项
/// - `progress_callback`: 进度回调 (0.0 - 100.0)
/// - `cancel_flag`: 取消标志
///
/// # 返回
/// 成功返回 `Ok(())`，失败返回 `Err(ExportError)`
pub fn export_audio(
    midi_path: &Path,
    soundfont_path: &Path,
    output_path: &Path,
    options: &AudioExportOptions,
    progress_callback: Option<Arc<dyn Fn(f32) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> ExportResult<()> {
    // 验证输入文件
    if !midi_path.exists() {
        return Err(ExportError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("MIDI 文件不存在: {:?}", midi_path),
        )));
    }

    if !soundfont_path.exists() {
        return Err(ExportError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("音色库文件不存在: {:?}", soundfont_path),
        )));
    }

    // 创建输出目录
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ExportError::Io(std::io::Error::other(e)))?;
    }

    tracing::info!(
        "开始音频导出: MIDI={:?}, SF2={:?}, 输出={:?}, 格式={}, 采样率={}Hz",
        midi_path,
        soundfont_path,
        output_path,
        options.format,
        options.sample_rate
    );

    let start = std::time::Instant::now();

    MidiEventParser::parse_and_render(
        midi_path,
        soundfont_path,
        output_path,
        options,
        progress_callback,
        cancel_flag,
    )?;

    let elapsed = start.elapsed();
    tracing::info!("音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}

/// 从 MIDI 原始字节直接导出音频（不与 ParsedMidi 关联，避免 MidiDocument 持续占用内存）
///
/// 调用前可先释放 `ParsedMidi` / `Arc<ParsedMidi>`，消除 `MidiDocument` 与 `midly::Smf`
/// 两份 MIDI 表示共存导致的峰值内存膨胀。
///
/// # 参数
/// - `midi_bytes`: MIDI 文件的原始字节
/// - `soundfont_path`: SF2 音色库路径
/// - `output_path`: 输出音频文件路径
/// - `options`: 导出选项
/// - `progress_callback`: 进度回调 (0.0 - 100.0)
/// - `cancel_flag`: 取消标志
pub fn export_audio_from_bytes(
    midi_bytes: &[u8],
    soundfont_path: &Path,
    output_path: &Path,
    options: &AudioExportOptions,
    progress_callback: Option<Arc<dyn Fn(f32) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> ExportResult<()> {
    // 验证音色库文件
    if !soundfont_path.exists() {
        return Err(ExportError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("音色库文件不存在: {:?}", soundfont_path),
        )));
    }

    // 创建输出目录
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ExportError::Io(std::io::Error::other(e)))?;
    }

    tracing::info!(
        "开始音频导出(从字节): 输出={:?}, 格式={}, 采样率={}Hz",
        output_path,
        options.format,
        options.sample_rate
    );

    let start = std::time::Instant::now();

    // 解析 MIDI 字节为 Smf
    let smf = midly::Smf::parse(midi_bytes)
        .map_err(|e| ExportError::MidiParse(format!("MIDI 解析失败: {}", e)))?;

    MidiEventParser::setup_and_render(
        &smf,
        soundfont_path,
        output_path,
        options,
        progress_callback,
        cancel_flag,
    )?;

    let elapsed = start.elapsed();
    tracing::info!("音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}
