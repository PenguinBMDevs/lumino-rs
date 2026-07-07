//! MIDI SMF 渲染路径——通过 midly::Smf 结构渲染音频
//!
//! 使用 block_render 模块（抄自 nezha-xsynth）代替逐事件渲染。

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};
use xsynth_core::{AudioStreamParams, ChannelCount};

use crate::error::ExportResult;

use super::block_render::{RenderCommand, TimedCommand, render_events_blocked, render_tail};
use super::exporter::AudioExporter;
use super::tempo::TempoMap;
use super::types::{AudioExportOptions, ExportProgress};
use super::writer::AudioFileWriter;

/// 从 midly::Smf 提取事件，转换为按秒排序的 TimedCommand 列表。
fn extract_commands(smf: &midly::Smf, tempo_map: &TempoMap) -> (Vec<TimedCommand>, f64) {
    let mut commands: Vec<TimedCommand> = Vec::new();
    let ppqn = match smf.header.timing {
        midly::Timing::Metrical(t) => u16::from(t) as u32,
        midly::Timing::Timecode(_, _) => 480,
    };
    let _ = ppqn; // ppqn 已编码在 tempo_map 中

    let mut max_tick: u64 = 0;

    for track in &smf.tracks {
        let mut tick: u64 = 0;
        for event in track.iter() {
            tick += u32::from(event.delta) as u64;
            if tick > max_tick {
                max_tick = tick;
            }
            if let midly::TrackEventKind::Midi { channel, message } = &event.kind {
                let ch = u8::from(*channel) as u32;
                let time_sec = tempo_map.tick_to_seconds(tick);
                match *message {
                    midly::MidiMessage::NoteOn { key, vel } => {
                        commands.push(TimedCommand {
                            time_sec,
                            cmd: RenderCommand::NoteOn {
                                key,
                                vel: u8::from(vel),
                                channel: ch,
                            },
                        });
                    }
                    midly::MidiMessage::NoteOff { key, .. } => {
                        commands.push(TimedCommand {
                            time_sec,
                            cmd: RenderCommand::NoteOff { key, channel: ch },
                        });
                    }
                    midly::MidiMessage::Controller { controller, value } => {
                        commands.push(TimedCommand {
                            time_sec,
                            cmd: RenderCommand::ControlChange {
                                controller: u8::from(controller),
                                value: u8::from(value),
                                channel: ch,
                            },
                        });
                    }
                    midly::MidiMessage::ProgramChange { program } => {
                        commands.push(TimedCommand {
                            time_sec,
                            cmd: RenderCommand::ProgramChange {
                                program: u8::from(program),
                                channel: ch,
                            },
                        });
                    }
                    midly::MidiMessage::PitchBend { bend } => {
                        commands.push(TimedCommand {
                            time_sec,
                            cmd: RenderCommand::PitchBend {
                                value: bend.as_int(),
                                channel: ch,
                            },
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    // 按时间排序（nezha 方式：所有事件平铺排序）
    commands.sort_by(|a, b| {
        a.time_sec
            .partial_cmp(&b.time_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_seconds = tempo_map.tick_to_seconds(max_tick);
    (commands, total_seconds)
}

/// 设置导出器 + 文件写入器，并调用核心渲染逻辑
pub(super) fn setup_and_render(
    smf: &midly::Smf,
    soundfont_path: &Path,
    output_path: &Path,
    options: &AudioExportOptions,
    progress_callback: Option<Arc<dyn Fn(ExportProgress) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> ExportResult<()> {
    // 加载音色库
    let audio_params = AudioStreamParams::new(options.sample_rate, ChannelCount::Stereo);
    let soundfont: Arc<dyn SoundfontBase> = Arc::new(
        SampleSoundfont::new(
            soundfont_path,
            audio_params,
            xsynth_core::soundfont::SoundfontInitOptions::default(),
        )
        .map_err(|e| crate::error::ExportError::AudioWrite(format!("音色库加载失败: {}", e)))?,
    );

    // 创建导出器
    let mut exporter = AudioExporter::new(options, soundfont);

    // 创建音频文件写入器
    let mut writer = AudioFileWriter::create(
        output_path,
        options.format,
        options.sample_rate,
        options.channels,
    )?;

    // 构建 tempo map
    let ppqn = match smf.header.timing {
        midly::Timing::Metrical(t) => u16::from(t) as u32,
        midly::Timing::Timecode(_, _) => 480,
    };
    let tempo_map = TempoMap::from_smf(smf, ppqn);

    // 提取事件
    let (commands, total_seconds) = extract_commands(smf, &tempo_map);

    tracing::info!(
        "[PATH_C] 开始音频导出(SMF block_render): 输出={:?}, 事件数={}, 总时长={:.2}s",
        output_path,
        commands.len(),
        total_seconds,
    );

    // 核心渲染 — 固定块渲染
    render_events_blocked(
        &commands,
        total_seconds,
        &mut exporter,
        &mut writer,
        options.sample_rate,
        16384,
        progress_callback.as_ref(),
        cancel_flag.as_ref(),
    )?;

    // 尾部衰减
    render_tail(
        &mut exporter,
        &mut writer,
        options.sample_rate,
        16384,
        progress_callback.as_ref(),
        cancel_flag.as_ref(),
    )?;

    // 完成文件写入
    writer.finalize()?;

    Ok(())
}

/// 解析 MIDI 文件并渲染为音频
pub fn parse_and_render(
    midi_path: &Path,
    soundfont_path: &Path,
    output_path: &Path,
    options: &AudioExportOptions,
    progress_callback: Option<Arc<dyn Fn(ExportProgress) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> ExportResult<()> {
    // 解析 MIDI 文件
    let midi_bytes = std::fs::read(midi_path)
        .map_err(|e| crate::error::ExportError::Io(std::io::Error::other(e)))?;

    let smf = midly::Smf::parse(&midi_bytes)
        .map_err(|e| crate::error::ExportError::MidiParse(format!("MIDI 解析失败: {}", e)))?;

    setup_and_render(
        &smf,
        soundfont_path,
        output_path,
        options,
        progress_callback,
        cancel_flag,
    )
}
