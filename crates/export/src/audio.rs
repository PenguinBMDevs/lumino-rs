//! 音频渲染模块 — 基于 xsynth 的 MIDI→WAV 离线渲染引擎
//!
//! # 双模式设计
//!
//! | 模式 | 函数 | 场景 |
//! |------|------|------|
//! | **流式** | [`render_audio`] | 从磁盘 MIDI 文件流式渲染（零事件常驻）|
//! | **内存** | [`render_audio_from_document`] | 复用已加载的 `MidiDocument`（零拷贝）|

pub mod config;
mod renderer;
pub mod tick_conv;
mod writer;

use std::sync::Arc;

use midly::{MidiMessage, TrackEventKind};
use tracing::info;
use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::SynthEvent,
    soundfont::{SampleSoundfont, SoundfontBase},
};

use lumino_midi_loader::{document::MidiDocument, streaming::StreamingMidiPlayer};

use crate::error::{ExportError, ExportResult};

use self::tick_conv::TickToTime;
pub use config::AudioRenderConfig;
pub use renderer::AudioRenderer;

// ════════════════════════════════════════════════════════════
// 公共 API
// ════════════════════════════════════════════════════════════

/// **流式模式**：直接从磁盘 MIDI 文件渲染为 WAV，零事件常驻内存。
///
/// 内部使用 `StreamingMidiPlayer`（基于 midly::mmap 零拷贝），
/// 逐事件从磁盘读取、逐 tick 渲染、用完即弃。
///
/// 适用于用户选择了一个磁盘上的 MIDI 文件进行导出的场景。
pub fn render_audio(config: &AudioRenderConfig) -> ExportResult<()> {
    info!(
        "[流式] 音频渲染: MIDI={:?}, SF2={:?}, 输出={:?}",
        config.midi_path, config.soundfonts, config.output_path
    );

    // 使用 mmap 映射 MIDI 文件，操作系统按需加载页面，不预读整个文件
    let file = std::fs::File::open(&config.midi_path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    let player = StreamingMidiPlayer::from_bytes(&mmap)
        .map_err(|e| ExportError::AudioWrite(format!("解析 MIDI 失败: {e}")))?;

    info!(
        "MIDI: {} 音轨, {} ticks, PPQN={}, 速度变化={}",
        player.track_count(),
        player.total_ticks(),
        player.ppqn(),
        player.tempo_changes().len()
    );

    // 初始化渲染引擎 + SF2 加载
    let (mut renderer, _) = init_renderer(config)?;

    // 构造 Tick→Time 转换器
    let tempos: Vec<(u32, f32)> = player.tempo_changes().to_vec();
    let ppqn = player.ppqn();
    let mut tick_conv = TickToTime::new(tempos, ppqn);

    // 流式渲染主循环
    run_streaming_render_loop(
        &mut renderer,
        &mut tick_conv,
        player,
        config.progress_callback.clone(),
    )?;

    // 收尾
    finalize_renderer(renderer, config)
}

/// **内存模式**：复用内存中已加载的 `MidiDocument` 渲染为 WAV，不拷贝音符数据。
///
/// 直接从 `doc.notes`（`Vec<Vec<NoteEvent>>`）和 `doc.control_events` 中
/// 引用数据，拆解为 NoteOn/NoteOff/CC/PC/PB 事件序列。
///
/// 适用于编辑器已加载 MIDI 时直接导出音频的场景。
pub fn render_audio_from_document(
    config: &AudioRenderConfig,
    doc: &MidiDocument,
) -> ExportResult<()> {
    info!(
        "[内存-流式] 音频渲染: SF2={:?}, 输出={:?}",
        config.soundfonts, config.output_path
    );

    // 初始化渲染引擎 + SF2 加载
    let (mut renderer, _) = init_renderer(config)?;

    // 流式迭代 MidiDocument 中的事件，无需构建 Vec<MergedEvent>
    let total_events: usize =
        doc.notes.iter().map(|v| v.len()).sum::<usize>() * 2 + doc.control_events.len();
    if total_events == 0 {
        return Err(ExportError::AudioWrite(
            "MIDI 文档中没有可渲染的事件".into(),
        ));
    }

    // 构造 Tick→Time 转换器
    let tempos = doc.tempo_changes.clone();
    let ppqn = 480; // MidiDocument 不保存 PPQN，480 是标准默认值
    let mut tick_conv = TickToTime::new(tempos, ppqn);

    // 文档渲染主循环（流式）
    run_document_render_loop(
        &mut renderer,
        &mut tick_conv,
        doc,
        config.progress_callback.clone(),
    )?;

    // 收尾
    finalize_renderer(renderer, config)
}

// ════════════════════════════════════════════════════════════
// 内部类型
// ════════════════════════════════════════════════════════════

/// 源自 MidiDocument 的合并事件（8 字节对齐）
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct MergedEvent {
    tick: u32,
    /// 0=NoteOn, 1=NoteOff, 2=CC, 3=PC, 4=PB
    kind: u8,
    channel: u8,
    param1: u8,  // key / controller / program
    param2: u16, // velocity / value / raw bend
}

/// MidiDocEventStream — 流式迭代 MidiDocument 中的事件，
/// 避免创建 Vec<MergedEvent> 冗余分配。
///
/// 内部维护每轨的 note 游标，每次迭代扫描所有轨道找到最小 tick，
/// 按优先级发射事件（NoteOff < CC < PC < PB < NoteOn）。
struct MidiDocEventStream<'a> {
    doc: &'a MidiDocument,
    /// 每轨的 Note 游标：(note_idx, note_on_emitted)
    note_cursors: Vec<(usize, bool)>,
    /// 控制事件游标
    ctrl_cursor: usize,
    total_events: usize,
    emitted: usize,
}

enum SourceId {
    Note(usize),
    Control,
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

    /// 获取下一个事件（按 tick 升序，同 tick 内 NoteOff < CC < PC < PB < NoteOn）
    #[allow(unused_assignments)]
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

        // 2. 在最小 tick 处找到优先级最高的事件
        // 优先级: NoteOff(1) < CC(2) < PC(3) < PB(4) < NoteOn(5)
        let mut best_source: Option<SourceId> = None;
        let mut best_priority = 6u8;
        let mut best_event: Option<MergedEvent> = None;

        // 检查 Note 游标
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
            if priority < best_priority {
                best_priority = priority;
                best_source = Some(SourceId::Note(track_idx));
                best_event = if note_on_emitted {
                    Some(MergedEvent {
                        tick: note.end_tick,
                        kind: 1, // NoteOff
                        channel: note.channel,
                        param1: note.key,
                        param2: 0,
                    })
                } else {
                    Some(MergedEvent {
                        tick: note.start_tick,
                        kind: 0, // NoteOn
                        channel: note.channel,
                        param1: note.key,
                        param2: note.velocity as u16,
                    })
                };
            }
        }

        // 检查控制事件
        if self.ctrl_cursor < self.doc.control_events.len() {
            let ctrl = &self.doc.control_events[self.ctrl_cursor];
            if ctrl.tick == min_tick {
                let (priority, event) = match ctrl.kind {
                    0 => {
                        let (c, v) = ctrl.as_control_change();
                        (
                            2,
                            MergedEvent {
                                tick: ctrl.tick,
                                kind: 2,
                                channel: ctrl.channel,
                                param1: c,
                                param2: v as u16,
                            },
                        )
                    }
                    1 => (
                        3,
                        MergedEvent {
                            tick: ctrl.tick,
                            kind: 3,
                            channel: ctrl.channel,
                            param1: ctrl.as_program_change(),
                            param2: 0,
                        },
                    ),
                    2 => (
                        4,
                        MergedEvent {
                            tick: ctrl.tick,
                            kind: 4,
                            channel: ctrl.channel,
                            param1: 0,
                            param2: ctrl.param,
                        },
                    ),
                    _ => unreachable!("PackedControlEvent.kind 只能是 0/1/2，实际为 {}", ctrl.kind),
                };
                if priority < best_priority {
                    best_priority = priority;
                    best_source = Some(SourceId::Control);
                    best_event = Some(event);
                }
            }
        }

        // 3. 推进游标
        if let Some(source) = best_source {
            match source {
                SourceId::Note(track_idx) => {
                    let (ref mut note_idx, ref mut note_on_emitted) = self.note_cursors[track_idx];
                    if *note_on_emitted {
                        // NoteOff 已发射 → 前进到下一音符
                        *note_idx += 1;
                        *note_on_emitted = false;
                    } else {
                        // NoteOn 已发射 → 下一事件是 NoteOff
                        *note_on_emitted = true;
                    }
                }
                SourceId::Control => {
                    self.ctrl_cursor += 1;
                }
            }
        }

        self.emitted += 1;
        best_event
    }
}

// ════════════════════════════════════════════════════════════
// 初始化
// ════════════════════════════════════════════════════════════

/// 初始化 AudioRenderer + 加载 SF2 音色库
fn init_renderer(
    config: &AudioRenderConfig,
) -> ExportResult<(AudioRenderer, xsynth_core::AudioStreamParams)> {
    let mut renderer = AudioRenderer::new(config, &config.output_path)?;
    let stream_params = renderer.stream_params();

    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite("未指定音色库文件".into()));
    }

    let sf_options = config.build_sf_options();
    let soundfonts: Vec<Arc<dyn SoundfontBase>> = config
        .soundfonts
        .iter()
        .map(|sf_path| {
            let sf: Arc<dyn SoundfontBase> = Arc::new(
                SampleSoundfont::new(sf_path, stream_params, sf_options)
                    .map_err(|e| ExportError::AudioWrite(format!("音色库 {sf_path:?}: {e}")))?,
            );
            Ok(sf)
        })
        .collect::<ExportResult<Vec<_>>>()?;

    renderer.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(soundfonts),
    )));
    renderer.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetLayerCount(config.layer_limit),
    )));

    Ok((renderer, stream_params))
}

// ════════════════════════════════════════════════════════════
// 流式渲染主循环（StreamingMidiPlayer）
// ════════════════════════════════════════════════════════════

fn run_streaming_render_loop(
    renderer: &mut AudioRenderer,
    tick_conv: &mut TickToTime,
    mut player: StreamingMidiPlayer,
    progress_callback: Option<crate::audio::config::ProgressCallback>,
) -> ExportResult<()> {
    info!("流式渲染循环开始...");
    let mut event_count = 0_u64;
    let mut note_count = 0_u64;
    let mut current_tick: u64 = 0;
    let mut last_progress_time = std::time::Instant::now();

    // 预计算总 tick 用于进度
    let total_ticks = player.total_ticks().max(1);
    let start_time = std::time::Instant::now();

    while let Some((tick, _track_idx, kind)) = player.next_event() {
        // 进度报告：每 100ms 一次
        if last_progress_time.elapsed() >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_ticks as f64;
            let elapsed = start_time.elapsed();
            let msg = format!(
                "进度: {:.1}% | 事件: {} | 音符: {} | 耗时: {:.1}s",
                pct * 100.0,
                event_count,
                note_count,
                elapsed.as_secs_f64()
            );
            if let Some(ref callback) = progress_callback {
                callback(msg, pct);
            } else {
                eprint!("\r{}", msg);
            }
            last_progress_time = std::time::Instant::now();
        }

        // 前进到事件所在 tick（delta 秒数驱动 xsynth 渲染）
        if tick > current_tick {
            let delta = tick_conv.advance_to(tick);
            if delta > 0.0 {
                renderer.render_batch(delta);
            }
            current_tick = tick;
        }

        // 转换并发送事件
        if let TrackEventKind::Midi { channel, message } = kind {
            let ch = channel.as_int() as u32;
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                            key,
                            vel: vel.as_int(),
                        }),
                    ));
                    event_count += 1;
                    note_count += 1;
                }
                MidiMessage::NoteOff { key, .. } => {
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key }),
                    ));
                    event_count += 1;
                }
                MidiMessage::Controller { controller, value } => {
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                            controller.as_int(),
                            value.as_int(),
                        ))),
                    ));
                }
                MidiMessage::ProgramChange { program } => {
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(program.as_int())),
                    ));
                }
                MidiMessage::PitchBend { bend } => {
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(
                            ControlEvent::PitchBendValue(bend.as_int() as f32 / 8192.0 - 1.0),
                        )),
                    ));
                }
                _ => {}
            }
        } // Meta / SysEx 被跳过
    }

    // 完成进度条
    let elapsed = start_time.elapsed();
    let msg = format!(
        "进度: 100.0% | 事件: {} | 音符: {} | 耗时: {:.1}s",
        event_count,
        note_count,
        elapsed.as_secs_f64()
    );
    if let Some(ref callback) = progress_callback {
        callback(msg, 1.0);
    } else {
        eprintln!("\r{}", msg);
    }

    info!("流式渲染完成: 处理 {event_count} 个事件, {note_count} 个音符");
    Ok(())
}

// ════════════════════════════════════════════════════════════
// 文档渲染主循环（MidiDocEventStream — 零 Vec 分配）
// ════════════════════════════════════════════════════════════

fn run_document_render_loop(
    renderer: &mut AudioRenderer,
    tick_conv: &mut TickToTime,
    doc: &MidiDocument,
    progress_callback: Option<crate::audio::config::ProgressCallback>,
) -> ExportResult<()> {
    let mut stream = MidiDocEventStream::new(doc);
    let total_tick = doc
        .notes
        .iter()
        .flat_map(|t| t.iter())
        .map(|n| n.end_tick)
        .max()
        .unwrap_or(0)
        .max(1) as u64;

    info!("文档流式渲染循环开始 ({} 事件)...", stream.total_events);
    let mut event_count = 0_u64;
    let mut current_tick: u64 = 0;
    let mut last_progress_time = std::time::Instant::now();
    let start_time = std::time::Instant::now();

    while let Some(event) = stream.next_event() {
        let tick = event.tick as u64;

        // 进度报告：每 100ms 一次
        if last_progress_time.elapsed() >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_tick as f64;
            let elapsed = start_time.elapsed();
            let msg = format!(
                "进度: {:.1}% | 事件: {} | 耗时: {:.1}s",
                pct * 100.0,
                event_count,
                elapsed.as_secs_f64()
            );
            if let Some(ref callback) = progress_callback {
                callback(msg, pct);
            } else {
                eprint!("\r{}", msg);
            }
            last_progress_time = std::time::Instant::now();
        }

        // 前进时间
        if tick > current_tick {
            let delta = tick_conv.advance_to(tick);
            if delta > 0.0 {
                renderer.render_batch(delta);
            }
            current_tick = tick;
        }

        // 发送事件
        let ch = event.channel as u32;
        match event.kind {
            0 => {
                renderer.send_event(SynthEvent::Channel(
                    ch,
                    ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                        key: event.param1,
                        vel: event.param2 as u8,
                    }),
                ));
                event_count += 1;
            }
            1 => {
                renderer.send_event(SynthEvent::Channel(
                    ch,
                    ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: event.param1 }),
                ));
                event_count += 1;
            }
            2 => {
                renderer.send_event(SynthEvent::Channel(
                    ch,
                    ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                        event.param1,
                        event.param2 as u8,
                    ))),
                ));
            }
            3 => {
                renderer.send_event(SynthEvent::Channel(
                    ch,
                    ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(event.param1)),
                ));
            }
            4 => {
                let normalized = (event.param2 as i16 - 8192) as f32 / 8192.0;
                renderer.send_event(SynthEvent::Channel(
                    ch,
                    ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                        normalized,
                    ))),
                ));
            }
            _ => {}
        }
    }

    // 完成进度
    let elapsed = start_time.elapsed();
    let msg = format!(
        "进度: 100.0% | 事件: {} | 耗时: {:.1}s",
        event_count,
        elapsed.as_secs_f64()
    );
    if let Some(ref callback) = progress_callback {
        callback(msg, 1.0);
    } else {
        eprintln!("\r{}", msg);
    }

    info!("文档流式渲染完成: 处理 {event_count} 个事件");
    Ok(())
}

// ════════════════════════════════════════════════════════════
// 收尾
// ════════════════════════════════════════════════════════════

fn finalize_renderer(renderer: AudioRenderer, config: &AudioRenderConfig) -> ExportResult<()> {
    let mut r = renderer;
    r.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
        ChannelAudioEvent::AllNotesOff,
    )));
    r.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
        ChannelAudioEvent::ResetControl,
    )));
    r.finalize()?;
    info!("音频渲染完成: {:?}", config.output_path);
    Ok(())
}

// build_document_events 已废弃 — 由 MidiDocEventStream 流式替代
