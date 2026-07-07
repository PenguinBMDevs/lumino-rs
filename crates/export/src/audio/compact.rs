//! CompactEvent 渲染路径——直接从 MidiDocument 流式渲染，零 Smf 解析
//!
//! 使用 block_render 模块（抄自 nezha-xsynth）代替逐事件渲染。
//! 所有事件先展平成 `TimedCommand`，排序后按固定块渲染。

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};
use xsynth_core::{AudioStreamParams, ChannelCount};

use lumino_midi_loader::MidiDocument;
use lumino_midi_loader::ParsedMidi;

use crate::error::{ExportError, ExportResult};

use super::block_render::{RenderCommand, TimedCommand, render_events_blocked, render_tail};
use super::exporter::AudioExporter;
use super::tempo::TempoMap;
use super::types::AudioExportOptions;
use super::writer::AudioFileWriter;

/// 从 MidiDocument 提取所有事件，转换为按秒排序的 TimedCommand 列表。
fn extract_commands(document: &MidiDocument, tempo_map: &TempoMap) -> (Vec<TimedCommand>, f64) {
    let mut commands: Vec<TimedCommand> = Vec::new();
    let mut max_tick: u64 = 0;

    // 1. NoteOn/NoteOff
    for track_notes in &document.notes {
        for note in track_notes {
            let ch = note.channel as u32;
            if ch > 0xFF {
                // safety: channel should be 0-15
            }
            commands.push(TimedCommand {
                time_sec: tempo_map.tick_to_seconds(note.start_tick as u64),
                cmd: RenderCommand::NoteOn {
                    key: note.key,
                    vel: note.velocity,
                    channel: ch,
                },
            });
            commands.push(TimedCommand {
                time_sec: tempo_map.tick_to_seconds(note.end_tick as u64),
                cmd: RenderCommand::NoteOff {
                    key: note.key,
                    channel: ch,
                },
            });
            if (note.end_tick as u64) > max_tick {
                max_tick = note.end_tick as u64;
            }
        }
    }

    // 2. 控制事件 (CC / PC / PB)
    for ctrl in &document.control_events {
        let ch = ctrl.channel as u32;
        let time_sec = tempo_map.tick_to_seconds(ctrl.tick as u64);
        match ctrl.kind {
            0 => {
                // ControlChange
                let controller = (ctrl.param >> 8) as u8;
                let value = (ctrl.param & 0xFF) as u8;
                commands.push(TimedCommand {
                    time_sec,
                    cmd: RenderCommand::ControlChange {
                        controller,
                        value,
                        channel: ch,
                    },
                });
            }
            1 => {
                // ProgramChange
                commands.push(TimedCommand {
                    time_sec,
                    cmd: RenderCommand::ProgramChange {
                        program: ctrl.param as u8,
                        channel: ch,
                    },
                });
            }
            2 => {
                // PitchBend
                commands.push(TimedCommand {
                    time_sec,
                    cmd: RenderCommand::PitchBend {
                        value: ctrl.param as i16,
                        channel: ch,
                    },
                });
            }
            _ => {}
        }
    }

    // 按时间排序（nezha 方式：所有事件平铺排序）
    commands.sort_by(|a, b| {
        a.time_sec
            .partial_cmp(&b.time_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_seconds = tempo_map.tick_to_seconds(max_tick);

    // 诊断日志
    let note_on_count = commands
        .iter()
        .filter(|c| matches!(c.cmd, RenderCommand::NoteOn { .. }))
        .count();
    let note_off_count = commands
        .iter()
        .filter(|c| matches!(c.cmd, RenderCommand::NoteOff { .. }))
        .count();
    let pc_count = commands
        .iter()
        .filter(|c| matches!(c.cmd, RenderCommand::ProgramChange { .. }))
        .count();
    if note_on_count > 0 {
        let keys: Vec<u8> = commands
            .iter()
            .filter_map(|c| match c.cmd {
                RenderCommand::NoteOn { key, .. } => Some(key),
                _ => None,
            })
            .collect();
        let min_key = keys.iter().copied().min().unwrap_or(0);
        let max_key = keys.iter().copied().max().unwrap_or(0);
        tracing::info!(
            "extract_commands: {} NoteOn, {} NoteOff, {} PC, keys=[{},{}], total_events={}",
            note_on_count,
            note_off_count,
            pc_count,
            min_key,
            max_key,
            commands.len(),
        );
    }

    (commands, total_seconds)
}

/// 从已解析的 ParsedMidi 导出音频（直接使用内存中 CompactEvent 数据，零解析）
///
/// 与 `export_audio_from_bytes` 不同，此函数**不会**再次 `midly::Smf::parse()`，
/// 而是从 `MidiDocument.track_notes()` 的 `NoteEvent` 实时构造 CompactEvent，
/// 结合 `control_events`（PackedControlEvent）流式渲染，彻底消除 Smf 结构占用。
pub fn export_audio_from_parsed(
    parsed_midi: &ParsedMidi,
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

    // 直接取内存中的 MidiDocument — 零解析，零额外分配
    let document = parsed_midi.document.as_ref().ok_or_else(|| {
        ExportError::InvalidData(
            "ParsedMidi 未加载 MidiDocument，请先调用 load_parsed_midi".to_string(),
        )
    })?;
    let document: &MidiDocument = document.as_ref();

    // 使用预解析的 division（PPQN），无需从原始字节提取
    // （原始字节已在解析后释放，不再常驻内存）
    let ppqn = parsed_midi.info.division.max(1) as u32;

    // 从预提取的 tempo 变化构建速度图（已由 midly loader 扫描完毕）
    let tempo_map = TempoMap::from_changes(&document.tempo_changes, ppqn);

    let total_notes: usize = document.notes.iter().map(|v| v.len()).sum();
    tracing::info!(
        "[PATH_A] 开始音频导出(block_render): 输出={:?}, 格式={}, \
         采样率={}Hz, 音符数={}, 控制事件数={}, 音轨数={}, PPQN={}",
        output_path,
        options.format,
        options.sample_rate,
        total_notes,
        document.control_events.len(),
        document.track_count(),
        ppqn,
    );

    let start = std::time::Instant::now();

    // 加载音色库
    let audio_params = AudioStreamParams::new(options.sample_rate, ChannelCount::Stereo);
    let soundfont: Arc<dyn SoundfontBase> = Arc::new(
        SampleSoundfont::new(
            soundfont_path,
            audio_params,
            xsynth_core::soundfont::SoundfontInitOptions::default(),
        )
        .map_err(|e| ExportError::AudioWrite(format!("音色库加载失败: {}", e)))?,
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

    // 提取事件（平铺、按秒排序）
    let (commands, total_seconds) = extract_commands(document, &tempo_map);

    // 核心渲染 — 固定块渲染
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

    // 尾部衰减
    render_tail(
        &mut exporter,
        &mut writer,
        options.sample_rate,
        512,
        progress_callback.as_ref(),
        cancel_flag.as_ref(),
    )?;

    // 完成文件写入
    writer.finalize()?;

    let elapsed = start.elapsed();
    tracing::info!("音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}
