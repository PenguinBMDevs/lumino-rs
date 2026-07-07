//! 流式 MIDI → 音频渲染（export 层粘合）
//!
//! 纯 MIDI 解析逻辑在 `lumino-midi-loader::StreamingMidiPlayer` 中，
//! 本模块仅负责将流式事件分派到 xsynth 进行音频渲染。
//!
//! # 数据流
//!
//! ```text
//! MIDI bytes
//!     │
//!     ▼
//! lumino_midi_loader::StreamingMidiPlayer  →  Vec<TimedCommand>
//!     │                                       │
//!     ▼                                       ▼
//! block_render::render_events_blocked()  →  AudioFileWriter
//! ```

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use midly::{MidiMessage, TrackEventKind};
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};
use xsynth_core::{AudioStreamParams, ChannelCount};

use lumino_midi_loader::StreamingMidiPlayer;

use crate::audio::block_render::{RenderCommand, TimedCommand, render_events_blocked, render_tail};
use crate::audio::tempo::TempoMap;
use crate::audio::types::AudioExportOptions;
use crate::error::{ExportError, ExportResult};

/// 将 midly `TrackEventKind` 转换为 `RenderCommand`。
fn kind_to_command(kind: TrackEventKind) -> Vec<RenderCommand> {
    match kind {
        TrackEventKind::Midi { channel, message } => {
            let ch = u8::from(channel) as u32;
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    let velocity = u8::from(vel);
                    if velocity > 0 {
                        vec![RenderCommand::NoteOn {
                            key,
                            vel: velocity,
                            channel: ch,
                        }]
                    } else {
                        // 力度为 0 的 NoteOn = NoteOff
                        vec![RenderCommand::NoteOff { key, channel: ch }]
                    }
                }
                MidiMessage::NoteOff { key, .. } => {
                    vec![RenderCommand::NoteOff { key, channel: ch }]
                }
                MidiMessage::Controller { controller, value } => {
                    vec![RenderCommand::ControlChange {
                        controller: u8::from(controller),
                        value: u8::from(value),
                        channel: ch,
                    }]
                }
                MidiMessage::PitchBend { bend } => {
                    vec![RenderCommand::PitchBend {
                        value: bend.as_int(),
                        channel: ch,
                    }]
                }
                MidiMessage::ProgramChange { program } => {
                    vec![RenderCommand::ProgramChange {
                        program: u8::from(program),
                        channel: ch,
                    }]
                }
                _ => vec![],
            }
        }
        TrackEventKind::Meta(_) | TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => {
            // Tempo 已在 StreamMidiPlayer 预扫描中处理，其余跳过
            vec![]
        }
    }
}

/// 使用流式 MIDI 播放器渲染音频文件（block_render 引擎）。
///
/// 从文件字节创建 `StreamingMidiPlayer`，逐事件消费 → 展平成 `Vec<TimedCommand>`
/// → 按秒排序 → `render_events_blocked` 固定块渲染。
pub fn render_streaming(
    midi_bytes: &[u8],
    soundfont_path: &Path,
    output_path: &Path,
    options: &AudioExportOptions,
    progress_callback: Option<Arc<dyn Fn(f32) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> ExportResult<()> {
    // 验证音色库
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
        "[PATH_B] 开始流式音频导出: 输出={:?}, 格式={}, 采样率={}Hz, 文件大小={}B",
        output_path,
        options.format,
        options.sample_rate,
        midi_bytes.len(),
    );

    let start = std::time::Instant::now();

    // 1. 加载音色库
    let audio_params = AudioStreamParams::new(options.sample_rate, ChannelCount::Stereo);
    let soundfont: Arc<dyn SoundfontBase> = Arc::new(
        SampleSoundfont::new(
            soundfont_path,
            audio_params,
            xsynth_core::soundfont::SoundfontInitOptions::default(),
        )
        .map_err(|e| ExportError::AudioWrite(format!("音色库加载失败: {}", e)))?,
    );

    // 2. 创建导出器
    let mut exporter = super::exporter::AudioExporter::new(options, soundfont);

    // 3. 创建音频文件写入器
    let mut writer = super::writer::AudioFileWriter::create(
        output_path,
        options.format,
        options.sample_rate,
        options.channels,
    )?;

    // 4. 创建流式播放器 + TempoMap
    let mut player = StreamingMidiPlayer::from_bytes(midi_bytes)
        .map_err(|e| ExportError::MidiParse(format!("流式 MIDI 解析失败: {}", e)))?;
    let tempo_map = TempoMap::from_changes(player.tempo_changes(), player.ppqn());

    // 5. 展平所有事件到 TimedCommand（流式播放器保持原始轨道内事件顺序）
    let mut commands: Vec<TimedCommand> = Vec::new();
    while let Some((tick, _track_idx, kind)) = player.next_event() {
        // 检查取消
        if let Some(ref cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(ExportError::AudioWrite("导出已取消".to_string()));
        }

        let time_sec = tempo_map.tick_to_seconds(tick);
        for cmd in kind_to_command(kind) {
            commands.push(TimedCommand { time_sec, cmd });
        }
    }

    // 按时间排序（流式播放器输出已按 tick 排序，但同 tick 事件来自不同轨道，
    // 排序确保一致性）
    commands.sort_by(|a, b| {
        a.time_sec
            .partial_cmp(&b.time_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let note_on_count = commands
        .iter()
        .filter(|c| matches!(c.cmd, RenderCommand::NoteOn { .. }))
        .count();
    let pc_count = commands
        .iter()
        .filter(|c| matches!(c.cmd, RenderCommand::ProgramChange { .. }))
        .count();

    // 按通道统计 NoteOn 分布
    use std::collections::BTreeMap;
    let mut ch_map: BTreeMap<u32, usize> = BTreeMap::new();
    let mut keys_out_of_range: Vec<u8> = Vec::new();
    for cmd in &commands {
        if let RenderCommand::NoteOn { channel, key, .. } = cmd.cmd {
            *ch_map.entry(channel).or_default() += 1;
            if key >= 128 {
                keys_out_of_range.push(key);
            }
        }
    }
    // 统计 key 分布（min/max 以及超出 127 的数量）
    let keys: Vec<u8> = commands
        .iter()
        .filter_map(|c| match c.cmd {
            RenderCommand::NoteOn { key, .. } => Some(key),
            _ => None,
        })
        .collect();
    let key_min = keys.iter().copied().min().unwrap_or(0);
    let key_max = keys.iter().copied().max().unwrap_or(0);
    let ch_dist: String = ch_map
        .iter()
        .map(|(ch, n)| format!("ch{}:{}", ch, n))
        .collect::<Vec<_>>()
        .join(", ");
    tracing::info!(
        "[PATH_B] extract_commands: {} NoteOn, {} NoteOff, {} PC, total_events={}, channels=[{}], keys=[{},{}], out_of_range={}",
        note_on_count,
        commands
            .iter()
            .filter(|c| matches!(c.cmd, RenderCommand::NoteOff { .. }))
            .count(),
        pc_count,
        commands.len(),
        ch_dist,
        key_min,
        key_max,
        keys_out_of_range.len(),
    );

    // 总时长取最后一个事件的时间
    let total_seconds = commands.last().map(|e| e.time_sec).unwrap_or(0.0);

    // 6. 核心渲染 — 固定块渲染
    render_events_blocked(
        &commands,
        total_seconds,
        &mut exporter,
        &mut writer,
        options.sample_rate,
        512,
        progress_callback.as_ref(),
        cancel_flag.as_ref(),
    )?;

    // 渲染完成后检查 voice 数量（诊断）
    let voice_count = exporter.voice_count();
    tracing::info!(
        "[PATH_B] 渲染完成: {} events, {} voices, total={:.2}s",
        commands.len(),
        voice_count,
        total_seconds,
    );

    // 7. 尾部衰减
    render_tail(
        &mut exporter,
        &mut writer,
        options.sample_rate,
        512,
        progress_callback.as_ref(),
        cancel_flag.as_ref(),
    )?;

    // 8. 完成文件写入
    writer.finalize()?;

    if let Some(callback) = &progress_callback {
        callback(100.0);
    }

    let elapsed = start.elapsed();
    tracing::info!("流式音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}
