//! 音频渲染模块 — MIDI→音频渲染流水线
//!
//! 参考 OmniConverter 的 MIDIConverter + EventsProcesser 架构设计：
//!
//! ```text
//! Config + MIDI
//!    ↓
//! AudioEngine (初始化 xsynth + 加载 SoundFont)
//!    ↓
//! EventProcessor (逐事件渲染，时间驱动)
//!    ↓
//! SampleSink (写入 WAV / FFmpeg 编码)
//!    ↓
//! 输出文件
//! ```
//!
//! # 双模式
//!
//! | 模式 | 函数 | 场景 |
//! |------|------|------|
//! | **流式** | [`render_audio`] | 从磁盘 MIDI 文件流式渲染 |
//! | **内存** | [`render_audio_from_document`] | 复用已加载的 `MidiDocument` |

pub mod codec;
pub mod config;
pub mod engine;
pub mod event;
pub mod limiter;
pub mod renderer;
pub mod stream;
pub mod tick_conv;

use midly::{MidiMessage, PitchBend, TrackEventKind};
use tracing::info;

use lumino_midi_loader::{MidiDocument, streaming::StreamingMidiPlayer};

use crate::error::{ExportError, ExportResult};

pub use config::AudioRenderConfig;
pub use engine::AudioEngine;
pub use stream::SampleSink;

use self::{
    event::MidiEventProcessor,
    stream::{FfmpegSink, WavFileSink},
    tick_conv::TickToTime,
};

// ════════════════════════════════════════════════════════════
// 公共 API
// ════════════════════════════════════════════════════════════

/// 流式模式：直接从磁盘 MIDI 文件渲染为音频文件
pub fn render_audio(config: &AudioRenderConfig) -> ExportResult<()> {
    info!(
        "[流式] 音频渲染: MIDI={:?}, SF2={:?}, 输出={:?}",
        config.midi_path, config.soundfonts, config.output_path
    );

    // 使用 mmap 映射 MIDI 文件
    let file = std::fs::File::open(&config.midi_path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    let player = StreamingMidiPlayer::from_bytes(&mmap)
        .map_err(|e| ExportError::AudioWrite(format!("解析 MIDI 失败: {e}")))?;

    let total_ticks = player.total_ticks().max(1);
    let tempos = player.tempo_changes().to_vec();
    let ppqn = player.ppqn();

    info!(
        "MIDI: {} 音轨, {} ticks, PPQN={}, 速度变化={}",
        player.track_count(),
        total_ticks,
        ppqn,
        tempos.len()
    );

    // 创建输出接收器
    let mut sink = create_output_sink(config)?;

    // 初始化渲染引擎
    let mut engine = AudioEngine::new(config.clone())?;

    // Tick→时间转换
    let mut tick_conv = TickToTime::new(tempos, ppqn);

    // 事件处理流水线
    let mut processor =
        MidiEventProcessor::new(config, engine.channel_group(), &mut tick_conv, &mut sink);

    // 流式渲染主循环
    run_streaming_render(config, &mut processor, player)?;

    // 收尾
    processor.finalize()?;
    sink.finalize()?;

    info!("音频渲染完成: {:?}", config.output_path);
    Ok(())
}

/// 内存模式：复用内存中已加载的 MidiDocument 渲染为音频文件
pub fn render_audio_from_document(
    config: &AudioRenderConfig,
    doc: &MidiDocument,
) -> ExportResult<()> {
    info!(
        "[内存] 音频渲染: SF2={:?}, 输出={:?}",
        config.soundfonts, config.output_path
    );

    let total_events: usize =
        doc.notes.iter().map(|v| v.len()).sum::<usize>() * 2 + doc.control_events.len();
    if total_events == 0 {
        return Err(ExportError::AudioWrite(
            "MIDI 文档中没有可渲染的事件".into(),
        ));
    }

    // 创建输出接收器
    let mut sink = create_output_sink(config)?;

    // 初始化渲染引擎
    let mut engine = AudioEngine::new(config.clone())?;

    // Tick→时间转换
    let tempos = doc.tempo_changes.clone();
    let ppqn = 480;
    let mut tick_conv = TickToTime::new(tempos, ppqn);

    // 事件处理流水线
    let mut processor =
        MidiEventProcessor::new(config, engine.channel_group(), &mut tick_conv, &mut sink);

    // 总 tick 数（以最后音符的 end_tick 为准）
    let total_tick = doc
        .notes
        .iter()
        .flat_map(|t| t.iter())
        .map(|n| n.end_tick as u64)
        .max()
        .unwrap_or(0)
        .max(1);

    // 文档渲染主循环
    run_document_render(config, &mut processor, doc, total_tick)?;

    // 收尾
    processor.finalize()?;
    sink.finalize()?;

    info!("文档音频渲染完成: {:?}", config.output_path);
    Ok(())
}

// ════════════════════════════════════════════════════════════
// 内部函数
// ════════════════════════════════════════════════════════════

/// 根据配置创建输出接收器
fn create_output_sink(config: &AudioRenderConfig) -> ExportResult<Box<dyn SampleSink>> {
    let codec = config.audio_codec;

    if codec.needs_ffmpeg() {
        let ffmpeg_path = codec::find_ffmpeg().ok_or_else(|| {
            ExportError::AudioWrite(format!(
                "需要 ffmpeg 来编码 {} 格式，但未找到 ffmpeg",
                codec.extension()
            ))
        })?;

        let sink = FfmpegSink::new(
            &ffmpeg_path,
            &config.output_path,
            codec,
            config.sample_rate,
            config.channels.channel_count(),
            config.audio_bitrate,
        )?;
        Ok(Box::new(sink))
    } else {
        let channels = config.channels.channel_count();
        let sink = WavFileSink::new(&config.output_path, config.sample_rate, channels)?;
        Ok(Box::new(sink))
    }
}

/// 流式渲染主循环（从 StreamingMidiPlayer 读取事件）
fn run_streaming_render(
    config: &AudioRenderConfig,
    processor: &mut MidiEventProcessor,
    mut player: StreamingMidiPlayer,
) -> ExportResult<()> {
    let total_ticks = player.total_ticks().max(1);
    let mut event_count = 0_u64;
    let mut note_count = 0_u64;
    let mut last_progress_time = std::time::Instant::now();
    let start_time = std::time::Instant::now();

    while let Some((tick, _track_idx, kind)) = player.next_event() {
        // 进度报告
        let now = std::time::Instant::now();
        if now.duration_since(last_progress_time) >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_ticks as f64;
            report_progress(config, pct, event_count, note_count, start_time);
            last_progress_time = now;
        }

        // 处理事件（渲染到该 tick + 发送事件）
        processor.process_midi_event(tick, &kind)?;

        // 统计
        if let TrackEventKind::Midi {
            channel: _,
            message,
        } = &kind
        {
            match message {
                MidiMessage::NoteOn { .. } => {
                    event_count += 1;
                    note_count += 1;
                }
                MidiMessage::NoteOff { .. } => {
                    event_count += 1;
                }
                _ => {}
            }
        }
    }

    // 完成进度
    report_progress(config, 1.0, event_count, note_count, start_time);

    info!("流式渲染完成: 处理 {event_count} 个事件, {note_count} 个音符");
    Ok(())
}

/// 文档渲染主循环（从 MidiDocument 读取事件）
fn run_document_render(
    config: &AudioRenderConfig,
    processor: &mut MidiEventProcessor,
    doc: &MidiDocument,
    total_tick: u64,
) -> ExportResult<()> {
    let mut event_count = 0_u64;
    let mut last_progress_time = std::time::Instant::now();
    let start_time = std::time::Instant::now();

    // 使用 MidiDocEventStream 流式迭代事件
    let mut stream = MidiDocEventStream::new(doc);
    let total_events = stream.total_events();

    info!("文档流式渲染循环开始 ({} 事件)...", total_events);

    while let Some(event) = stream.next_event() {
        let tick = event.tick as u64;

        // 进度报告
        let now = std::time::Instant::now();
        if now.duration_since(last_progress_time) >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_tick as f64;
            report_progress(config, pct, event_count, 0, start_time);
            last_progress_time = now;
        }

        // 构建 TrackEventKind（未知 kind 跳过该事件）
        if let Some(kind) = build_track_event_kind(&event) {
            processor.process_midi_event(tick, &kind)?;
            event_count += 1;
        }
    }

    // 完成进度
    report_progress(config, 1.0, event_count, 0, start_time);

    info!("文档流式渲染完成: 处理 {event_count} 个事件");
    Ok(())
}

/// 报告进度
fn report_progress(
    config: &AudioRenderConfig,
    pct: f64,
    event_count: u64,
    note_count: u64,
    start_time: std::time::Instant,
) {
    let elapsed = start_time.elapsed();
    let msg = format!(
        "进度: {:.1}% | 事件: {} | 音符: {} | 耗时: {:.1}s",
        pct * 100.0,
        event_count,
        note_count,
        elapsed.as_secs_f64()
    );
    if let Some(ref callback) = config.progress_callback {
        callback(msg, pct);
    } else {
        eprint!("\r{}  ", msg);
    }
}

// ════════════════════════════════════════════════════════════
// MidiDocEventStream — 流式迭代 MidiDocument 中的事件
// ════════════════════════════════════════════════════════════

/// 合并事件（8 字节对齐）
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct MergedEvent {
    tick: u32,
    /// 0=NoteOn, 1=NoteOff, 2=CC, 3=PC, 4=PB
    kind: u8,
    channel: u8,
    param1: u8,
    param2: u16,
}

/// MidiDocEventStream — 流式迭代 MidiDocument 中的事件
struct MidiDocEventStream<'a> {
    doc: &'a MidiDocument,
    note_cursors: Vec<(usize, bool)>,
    ctrl_cursor: usize,
    total_events: usize,
    emitted: usize,
}

impl<'a> MidiDocEventStream<'a> {
    fn new(doc: &'a MidiDocument) -> Self {
        let track_count = doc.notes.len();
        let note_cursors = vec![(0_usize, false); track_count];
        let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
        let total_events = total_notes * 2 + doc.control_events.len();
        MidiDocEventStream {
            doc,
            note_cursors,
            ctrl_cursor: 0,
            total_events,
            emitted: 0,
        }
    }

    fn total_events(&self) -> usize {
        self.total_events
    }

    fn next_event(&mut self) -> Option<MergedEvent> {
        if self.emitted >= self.total_events {
            return None;
        }

        // 1. 找到最小 tick
        let mut min_tick = u32::MAX;

        for (track_idx, &(note_idx, note_on_emitted)) in self.note_cursors.iter().enumerate() {
            if note_idx < self.doc.notes[track_idx].len() {
                let note = &self.doc.notes[track_idx][note_idx];
                let tick = if note_on_emitted {
                    note.end_tick
                } else {
                    note.start_tick
                };
                if tick < min_tick {
                    min_tick = tick;
                }
            }
        }
        if self.ctrl_cursor < self.doc.control_events.len() {
            let tick = self.doc.control_events[self.ctrl_cursor].tick;
            if tick < min_tick {
                min_tick = tick;
            }
        }

        if min_tick == u32::MAX {
            return None;
        }

        // 2. 在最小 tick 处找到优先级最高的事件（priority 数值越小优先级越高）
        let mut best: Option<(u8, MergedEvent)> = None;

        for (track_idx, &(note_idx, note_on_emitted)) in self.note_cursors.iter().enumerate() {
            if note_idx >= self.doc.notes[track_idx].len() {
                continue;
            }
            let note = &self.doc.notes[track_idx][note_idx];
            let tick = if note_on_emitted {
                note.end_tick
            } else {
                note.start_tick
            };
            if tick != min_tick {
                continue;
            }
            let priority = if note_on_emitted { 1 } else { 5 };
            let event = if note_on_emitted {
                MergedEvent {
                    tick: note.end_tick,
                    kind: 1,
                    channel: note.channel,
                    param1: note.key,
                    param2: 0,
                }
            } else {
                MergedEvent {
                    tick: note.start_tick,
                    kind: 0,
                    channel: note.channel,
                    param1: note.key,
                    param2: note.velocity as u16,
                }
            };
            if best.as_ref().map_or(true, |(p, _)| priority < *p) {
                best = Some((priority, event));
            }
        }

        if self.ctrl_cursor < self.doc.control_events.len() {
            let ctrl = &self.doc.control_events[self.ctrl_cursor];
            if ctrl.tick == min_tick {
                // 未知类型的控制事件直接跳过，避免 panic（合法 kind 仅为 0/1/2）
                if let Some((priority, event)) = match ctrl.kind {
                    0 => {
                        let (c, v) = ctrl.as_control_change();
                        Some((
                            2,
                            MergedEvent {
                                tick: ctrl.tick,
                                kind: 2,
                                channel: ctrl.channel,
                                param1: c,
                                param2: v as u16,
                            },
                        ))
                    }
                    1 => Some((
                        3,
                        MergedEvent {
                            tick: ctrl.tick,
                            kind: 3,
                            channel: ctrl.channel,
                            param1: ctrl.as_program_change(),
                            param2: 0,
                        },
                    )),
                    2 => Some((
                        4,
                        MergedEvent {
                            tick: ctrl.tick,
                            kind: 4,
                            channel: ctrl.channel,
                            param1: 0,
                            param2: ctrl.param,
                        },
                    )),
                    _ => None,
                } {
                    if best.as_ref().map_or(true, |(p, _)| priority < *p) {
                        best = Some((priority, event));
                    }
                }
            }
        }

        // 3. 推进游标
        if let Some((_, event)) = &best {
            match event.kind {
                0 | 1 => {
                    for (track_idx, cursor) in self.note_cursors.iter_mut().enumerate() {
                        let (note_idx, note_on_emitted) = cursor;
                        if *note_idx < self.doc.notes[track_idx].len() {
                            let note = &self.doc.notes[track_idx][*note_idx];
                            let note_tick = if *note_on_emitted {
                                note.end_tick
                            } else {
                                note.start_tick
                            };
                            if note_tick == event.tick {
                                if *note_on_emitted {
                                    *note_idx += 1;
                                    *note_on_emitted = false;
                                } else {
                                    *note_on_emitted = true;
                                }
                                break;
                            }
                        }
                    }
                }
                2..=4 => {
                    self.ctrl_cursor += 1;
                }
                // 未知 kind 不推进任何游标（正常情况下 MergedEvent::kind 恒为 0..=4）
                _ => {}
            }
        }

        self.emitted += 1;
        best.map(|(_, e)| e)
    }
}

/// 将 MergedEvent 转换为 TrackEventKind；遇到未知 kind 返回 None（跳过该事件）
fn build_track_event_kind(event: &MergedEvent) -> Option<TrackEventKind<'static>> {
    use midly::num::{u4, u7, u14};
    let channel = u4::new(event.channel & 0x0f);
    match event.kind {
        0 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::NoteOn {
                key: event.param1,
                vel: u7::new(event.param2 as u8),
            },
        }),
        1 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::NoteOff {
                key: event.param1,
                vel: u7::new(0),
            },
        }),
        2 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::Controller {
                controller: u7::new(event.param1),
                value: u7::new(event.param2 as u8),
            },
        }),
        3 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::ProgramChange {
                program: u7::new(event.param1),
            },
        }),
        4 => Some(TrackEventKind::Midi {
            channel,
            message: MidiMessage::PitchBend {
                bend: PitchBend(u14::new(event.param2)),
            },
        }),
        // 正常情况下 MergedEvent::kind 恒为 0..=4
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::{NoteEvent, track::TrackManager};

    fn make_doc(notes: Vec<Vec<NoteEvent>>, total_ticks: u32) -> MidiDocument {
        let track_count = notes.len() as u16;
        MidiDocument {
            notes,
            tempo_changes: vec![(0, 120.0)],
            control_events: vec![],
            track_names: (0..track_count).map(|_| None).collect(),
            total_ticks,
            track_count,
            tracks: TrackManager::new(track_count),
        }
    }

    /// 验证单个音符的 next_event 顺序：NoteOn（tick=0）→ NoteOff（tick=end）→ None
    #[test]
    fn test_next_event_single_note_order() {
        let doc = make_doc(vec![vec![NoteEvent::new(0, 10, 60, 100, 0)]], 10);
        let mut stream = MidiDocEventStream::new(&doc);
        assert_eq!(stream.total_events(), 2, "1 note = 2 events (on+off)");

        // First: note-on at tick 0（kind: 0 = NoteOn, param2=0 表示无 velocity 编码）
        let e1 = stream.next_event().expect("first event");
        assert_eq!(e1.kind, 0, "first should be NoteOn (kind=0)");
        assert_eq!(e1.tick, 0, "note-on at start tick");
        assert_eq!(e1.channel, 0);
        assert_eq!(e1.param1, 60, "param1 = note key");

        // Second: note-off at tick 10（kind: 1 = NoteOff）
        let e2 = stream.next_event().expect("second event");
        assert_eq!(e2.kind, 1, "second should be NoteOff (kind=1)");
        assert_eq!(e2.tick, 10, "note-off at end tick");
        assert_eq!(e2.channel, 0);
        assert_eq!(e2.param1, 60, "param1 = note key");

        // Exhausted
        assert!(stream.next_event().is_none(), "stream should be exhausted");
    }

    /// 验证跨轨最小 tick 优先：track 1 有更早的音符 → 先发出 track 1 的 NoteOn
    #[test]
    fn test_next_event_cross_track_min_tick() {
        let doc = make_doc(
            vec![
                vec![NoteEvent::new(10, 20, 60, 100, 0)],
                vec![NoteEvent::new(0, 5, 64, 100, 1)],
            ],
            20,
        );
        let mut stream = MidiDocEventStream::new(&doc);
        let e = stream.next_event().expect("first event");
        assert_eq!(e.tick, 0, "first event should be at earliest tick");
        assert_eq!(e.kind, 0, "first event should be NoteOn (kind=0)");
        assert_eq!(e.channel, 1, "track 1 has the earlier note");
        assert_eq!(e.param1, 64, "key from track 1");
    }

    /// 验证相同最小 tick 时按轨道迭代顺序取第一个（先到先得）
    #[test]
    fn test_next_event_tie_same_tick_first_track() {
        let doc = make_doc(
            vec![
                vec![NoteEvent::new(0, 10, 60, 100, 0)],
                vec![NoteEvent::new(0, 10, 64, 100, 1)],
            ],
            10,
        );
        let mut stream = MidiDocEventStream::new(&doc);
        let e = stream.next_event().expect("first event");
        assert_eq!(e.channel, 0, "track 0 wins tie (iteration order)");
        let e = stream.next_event().expect("second event");
        assert_eq!(e.kind, 0, "second event should also be NoteOn (kind=0)");
        assert_eq!(e.channel, 1, "track 1 second");
    }
}
