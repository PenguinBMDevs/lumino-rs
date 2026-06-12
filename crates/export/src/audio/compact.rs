//! CompactEvent 渲染路径——直接从 MidiDocument 流式渲染，零 Smf 解析

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent, ControlEvent};
use xsynth_core::channel_group::SynthEvent;
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};
use xsynth_core::{AudioStreamParams, ChannelCount};

use lumino_midi_io::compact::EventKind;
use lumino_midi_loader::MidiDocument;
use lumino_midi_loader::ParsedMidi;

use crate::error::{ExportError, ExportResult};

use super::MidiEventParser;
use super::exporter::AudioExporter;
use super::tempo::{TempoMap, extract_ppqn_from_bytes};
use super::types::AudioExportOptions;
use super::writer::AudioFileWriter;

impl MidiEventParser {
    /// 使用内存中的 CompactEvent + PackedControlEvent 直接流式渲染（零 Smf 解析）
    ///
    /// 这是 `export_audio_from_parsed` 的核心路径，彻底消除 `midly::Smf` 结构的分配。
    /// 数据直接从 `MidiDocument`（已由 midly loader 解析为 CompactEvent）流式消费。
    pub(super) fn render_compact_events(
        document: &MidiDocument,
        tempo_map: &TempoMap,
        exporter: &mut AudioExporter,
        writer: &mut AudioFileWriter,
        progress_callback: Option<&Arc<dyn Fn(f32) + Send + Sync>>,
        cancel_flag: Option<&Arc<AtomicBool>>,
    ) -> ExportResult<()> {
        let total_ticks = document.total_ticks as u64;
        let mut current_tick: u64 = 0;
        let mut last_progress = 0.0;

        // 按音轨顺序处理（与 SMF 路径行为一致）
        for track_id in 0..document.track_count() as u16 {
            let (start, end) = document.track_events_range(track_id);
            let track_events = &document.events[start..end];

            // 筛选本轨的控制事件（control_events 全程按 tick 排序）
            let track_controls: Vec<&midly::loader::PackedControlEvent> = document
                .control_events
                .iter()
                .filter(|ce| ce.track == track_id)
                .collect();

            let mut ev_idx = 0;
            let mut ctrl_idx = 0;

            while ev_idx < track_events.len() || ctrl_idx < track_controls.len() {
                // 检查取消标志
                if let Some(cancel) = cancel_flag
                    && cancel.load(Ordering::Relaxed)
                {
                    return Err(ExportError::AudioWrite("导出已取消".to_string()));
                }

                // 从事件源和控制源中取最小 tick
                let next_tick = {
                    let ev_tick = track_events
                        .get(ev_idx)
                        .map(|e| e.delta_tick() as u64)
                        .unwrap_or(u64::MAX);
                    let ctrl_tick = track_controls
                        .get(ctrl_idx)
                        .map(|c| c.tick as u64)
                        .unwrap_or(u64::MAX);
                    ev_tick.min(ctrl_tick)
                };

                if next_tick == u64::MAX {
                    break;
                }

                // 渲染时间片
                let target_time = tempo_map.tick_to_seconds(next_tick);
                let current_time = tempo_map.tick_to_seconds(current_tick);
                if target_time > current_time {
                    let render_time = target_time - current_time;
                    exporter.render_batch(render_time);
                    let samples = exporter.take_samples();
                    if !samples.is_empty() {
                        writer.write_samples(&samples)?;
                    }
                    current_tick = next_tick;
                }

                // 处理本 tick 的所有 CompactEvent
                while ev_idx < track_events.len()
                    && track_events[ev_idx].delta_tick() as u64 == next_tick
                {
                    let ev = &track_events[ev_idx];
                    match ev.kind() {
                        EventKind::NoteOn => {
                            let key = ev.param1() as u8;
                            let vel = ev.param2() as u8;
                            exporter.send_event(SynthEvent::Channel(
                                ev.channel() as u32,
                                ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key, vel }),
                            ));
                        }
                        EventKind::NoteOff => {
                            exporter.send_event(SynthEvent::Channel(
                                ev.channel() as u32,
                                ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                                    key: ev.param1() as u8,
                                }),
                            ));
                        }
                        EventKind::Tempo => {
                            // Tempo 已由 TempoMap 预扫描，此处跳过
                        }
                        _ => {}
                    }
                    ev_idx += 1;
                }

                // 处理本 tick 的所有控制事件
                while ctrl_idx < track_controls.len()
                    && track_controls[ctrl_idx].tick as u64 == next_tick
                {
                    let ctrl = track_controls[ctrl_idx];
                    let ch = ctrl.channel as u32;
                    match ctrl.kind {
                        0 => {
                            // ControlChange
                            let controller = (ctrl.param >> 8) as u8;
                            let value = (ctrl.param & 0xFF) as u8;
                            exporter.send_event(SynthEvent::Channel(
                                ch,
                                ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                                    controller, value,
                                ))),
                            ));
                        }
                        1 => {
                            // ProgramChange
                            exporter.send_event(SynthEvent::Channel(
                                ch,
                                ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(
                                    ctrl.param as u8,
                                )),
                            ));
                        }
                        2 => {
                            // PitchBend
                            let bend_value = ctrl.param as f32 / 8192.0;
                            exporter.send_event(SynthEvent::Channel(
                                ch,
                                ChannelEvent::Audio(ChannelAudioEvent::Control(
                                    ControlEvent::PitchBendValue(bend_value),
                                )),
                            ));
                        }
                        _ => {}
                    }
                    ctrl_idx += 1;
                }

                // 更新进度（防除零）
                if let Some(callback) = progress_callback {
                    let progress = if total_ticks > 0 {
                        (current_tick as f64 / total_ticks as f64 * 100.0).min(99.0)
                    } else {
                        99.0
                    };
                    if (progress - last_progress).abs() >= 1.0 {
                        callback(progress as f32);
                        last_progress = progress;
                    }
                }
            }
        }

        Ok(())
    }
}

/// 从已解析的 ParsedMidi 导出音频（直接使用内存中 CompactEvent 数据，零解析）
///
/// 与 `export_audio_from_bytes` 不同，此函数**不会**再次 `midly::Smf::parse()`，
/// 而是直接读取 `MidiDocument.events`（CompactEvent）+ `control_events`
/// （PackedControlEvent）流式渲染，彻底消除 Smf 结构占用。
///
/// # 参数
/// - `parsed_midi`: 已解析的 MIDI 数据（必须包含 `document`）
/// - `soundfont_path`: SF2 音色库路径
/// - `output_path`: 输出音频文件路径
/// - `options`: 导出选项
/// - `progress_callback`: 进度回调 (0.0 - 100.0)
/// - `cancel_flag`: 取消标志
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

    // 从 MIDI 头部提取 PPQN（仅 14 字节，不解析完整文件）
    let ppqn = if let Some(ref midi_data) = parsed_midi.midi_data {
        extract_ppqn_from_bytes(midi_data)?
    } else {
        480
    };

    // 从预提取的 tempo 变化构建速度图（已由 midly loader 扫描完毕）
    let tempo_map = TempoMap::from_changes(&document.tempo_changes, ppqn);

    tracing::info!(
        "开始音频导出(CompactEvent 直连): 输出={:?}, 格式={}, \
         采样率={}Hz, 事件数={}, 音轨数={}, PPQN={}",
        output_path,
        options.format,
        options.sample_rate,
        document.events.len() + document.control_events.len(),
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

    // 核心渲染 — 直接从内存 CompactEvent 流式消费
    MidiEventParser::render_compact_events(
        document,
        &tempo_map,
        &mut exporter,
        &mut writer,
        progress_callback.as_ref(),
        cancel_flag.as_ref(),
    )?;

    // 完成渲染：衰减样本写入
    exporter.finalize(&mut writer)?;

    // 完成文件写入
    writer.finalize()?;

    // 进度 100%
    if let Some(callback) = progress_callback {
        callback(100.0);
    }

    let elapsed = start.elapsed();
    tracing::info!("音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}
