//! 基于固定块的事件渲染引擎（抄自 nezha-xsynth）
//!
//! # 核心思路
//!
//! 与逐事件 `render_batch` 不同，本模块将时间划分为固定块（512 samples），
//! 每块内先批量发送所有落到本块的事件，再渲染一块音频。
//!
//! 优点：
//! - **无 per-track 遍历** — 所有事件展平按时间排序，单次遍历
//! - **无 shared current_tick** — 不依赖渲染循环维护时间状态
//! - **事件顺序天然正确** — 同时间的事件按排序顺序处理，PC 在 NoteOn 之前
//! - **渲染语义清晰** — 每次 `read_samples` 产生固定量的音频数据
//!
//! 参考: nezha/crates/nezha-xsynth/src/render.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent, ControlEvent};
use xsynth_core::channel_group::SynthEvent;

use crate::error::ExportResult;

use super::exporter::AudioExporter;
use super::writer::AudioFileWriter;

/// 渲染命令——nezha 的 SynthCommand 等价物。
#[derive(Clone, Debug)]
pub(crate) enum RenderCommand {
    NoteOn {
        key: u8,
        vel: u8,
        channel: u32,
    },
    NoteOff {
        key: u8,
        channel: u32,
    },
    ControlChange {
        controller: u8,
        value: u8,
        channel: u32,
    },
    ProgramChange {
        program: u8,
        channel: u32,
    },
    PitchBend {
        value: i16,
        channel: u32,
    },
}

/// 带时间的事件。
#[derive(Clone, Debug)]
pub(crate) struct TimedCommand {
    pub time_sec: f64,
    pub cmd: RenderCommand,
}

/// 将命令发送到 xsynth。
pub(super) fn send_command(exporter: &mut AudioExporter, cmd: &RenderCommand) {
    match *cmd {
        RenderCommand::NoteOn { key, vel, channel } => {
            exporter.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key, vel }),
            ));
        }
        RenderCommand::NoteOff { key, channel } => {
            exporter.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key }),
            ));
        }
        RenderCommand::ControlChange {
            controller,
            value,
            channel,
        } => {
            exporter.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                    controller, value,
                ))),
            ));
        }
        RenderCommand::ProgramChange { program, channel } => {
            exporter.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(program)),
            ));
        }
        RenderCommand::PitchBend { value, channel } => {
            let normalized = value as f32 / 8192.0;
            exporter.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                    normalized,
                ))),
            ));
        }
    }
}

/// 使用固定块渲染一段事件序列。
///
/// # 参数
/// - `events`: 已按 `time_sec` 排序的渲染命令
/// - `total_seconds`: 总时长（用于进度条）
/// - `exporter`: 已初始化的 xsynth 导出器
/// - `writer`: 音频文件写入器
/// - `sample_rate`: 采样率
/// - `block_samples`: 每个固定块的样本数（默认 512）
/// - `progress_callback`: 进度回调
/// - `cancel_flag`: 取消标志
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_events_blocked(
    events: &[TimedCommand],
    total_seconds: f64,
    exporter: &mut AudioExporter,
    writer: &mut AudioFileWriter,
    sample_rate: u32,
    block_samples: usize,
    progress_callback: Option<&Arc<dyn Fn(f32) + Send + Sync>>,
    cancel_flag: Option<&Arc<AtomicBool>>,
) -> ExportResult<()> {
    if events.is_empty() && total_seconds <= 0.0 {
        return Ok(());
    }

    let events_end_time = events.last().map(|e| e.time_sec).unwrap_or(0.0);
    let block_sec = block_samples as f64 / sample_rate as f64;
    let mut block_start = 0.0_f64;
    let mut event_idx = 0;

    while block_start < events_end_time {
        // 检查取消
        if let Some(cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(crate::error::ExportError::AudioWrite(
                "导出已取消".to_string(),
            ));
        }

        let block_end = (block_start + block_sec).min(events_end_time);
        let delta = block_end - block_start;

        // 发送落在本块内的所有事件
        // 注意：用 <= 而非 <，否则 time_sec 恰好等于 events_end_time 的事件（通常是最后一个 NoteOff）
        // 永远不会被发送，导致对应的音符永久保持发声（"只有最后一个 key 在演奏" 症状）。
        while event_idx < events.len() && events[event_idx].time_sec <= block_end {
            send_command(exporter, &events[event_idx].cmd);
            event_idx += 1;
        }

        // 渲染本块
        if delta > 0.0 {
            exporter.render_batch(delta);
            let samples = exporter.take_samples();
            if !samples.is_empty() {
                writer.write_samples(&samples)?;
            }
        }

        // 进度
        if let Some(callback) = progress_callback {
            let progress = ((block_end / total_seconds.max(1.0)) * 100.0).min(99.0) as f32;
            callback(progress);
        }

        block_start = block_end;
    }

    Ok(())
}

/// 渲染尾部衰减（AllNotesOff + ResetControl + 静音检测）。
///
/// 参考 nezha 的 tail 处理：
/// 1. 先渲染 2 秒让音符自然衰减
/// 2. 发送 AllNotesOff + ResetControl
/// 3. 继续渲染最多 5 秒，静音时提前退出
pub(crate) fn render_tail(
    exporter: &mut AudioExporter,
    writer: &mut AudioFileWriter,
    sample_rate: u32,
    block_samples: usize,
    progress_callback: Option<&Arc<dyn Fn(f32) + Send + Sync>>,
    cancel_flag: Option<&Arc<AtomicBool>>,
) -> ExportResult<()> {
    let block_sec = block_samples as f64 / sample_rate as f64;

    // 1. 释放尾部：2 秒自然衰减
    let release_time = 2.0_f64;
    let mut remaining = release_time;
    while remaining > 0.0 {
        if let Some(cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(crate::error::ExportError::AudioWrite(
                "导出已取消".to_string(),
            ));
        }

        let delta = remaining.min(block_sec);
        exporter.render_batch(delta);
        let samples = exporter.take_samples();
        if !samples.is_empty() {
            writer.write_samples(&samples)?;
        }
        remaining -= delta;
    }

    // 2. AllNotesOff + ResetControl
    exporter.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
        ChannelAudioEvent::AllNotesOff,
    )));
    exporter.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
        ChannelAudioEvent::ResetControl,
    )));

    // 3. 静音检测：最多 5 秒
    let mut tail_remaining = 5.0_f64;
    while tail_remaining > 0.0 {
        if let Some(cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(crate::error::ExportError::AudioWrite(
                "导出已取消".to_string(),
            ));
        }

        let delta = tail_remaining.min(block_sec * 4.0); // 更大块加速尾音检测
        exporter.render_batch(delta);
        let samples = exporter.take_samples();
        if !samples.is_empty() {
            writer.write_samples(&samples)?;
        }
        tail_remaining -= delta;

        // 静音检测
        if samples.iter().all(|s| s.abs() <= 0.0001) {
            break;
        }
    }

    if let Some(callback) = progress_callback {
        callback(100.0);
    }

    Ok(())
}
