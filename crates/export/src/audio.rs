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
mod tick_conv;
mod writer;

use std::sync::Arc;

use midly::{MidiMessage, TrackEventKind};
use tracing::info;
use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::SynthEvent,
    soundfont::{SampleSoundfont, SoundfontBase},
};

use lumino_midi_loader::{
    document::MidiDocument,
    streaming::StreamingMidiPlayer,
};

use crate::error::{ExportError, ExportResult};

pub use config::AudioRenderConfig;
pub use renderer::AudioRenderer;
use self::tick_conv::TickToTime;

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

    // 读取 MIDI 字节（必须持有以维持 StreamingMidiPlayer 的借用）
    let midi_bytes = std::fs::read(&config.midi_path)
        .map_err(ExportError::Io)?;

    let player = StreamingMidiPlayer::from_bytes(&midi_bytes)
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
    run_streaming_render_loop(&mut renderer, &mut tick_conv, player)?;

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
        "[内存] 音频渲染: SF2={:?}, 输出={:?}",
        config.soundfonts, config.output_path
    );

    // 初始化渲染引擎 + SF2 加载
    let (mut renderer, _) = init_renderer(config)?;

    // 从 MidiDocument 构建合并事件列表
    let merged = build_document_events(doc);
    if merged.is_empty() {
        return Err(ExportError::AudioWrite("MIDI 文档中没有可渲染的事件".into()));
    }

    // 构造 Tick→Time 转换器
    let tempos = doc.tempo_changes.clone();
    let ppqn = 480; // MidiDocument 不保存 PPQN，480 是标准默认值
    let mut tick_conv = TickToTime::new(tempos, ppqn);

    // 文档渲染主循环
    run_document_render_loop(&mut renderer, &mut tick_conv, &merged)?;

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
    param1: u8,   // key / controller / program
    param2: u16,  // velocity / value / raw bend
}

// ════════════════════════════════════════════════════════════
// 初始化
// ════════════════════════════════════════════════════════════

/// 初始化 AudioRenderer + 加载 SF2 音色库
fn init_renderer(config: &AudioRenderConfig) -> ExportResult<(AudioRenderer, xsynth_core::AudioStreamParams)> {
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
) -> ExportResult<()> {
    info!("流式渲染循环开始...");
    let mut event_count = 0_u64;
    let mut note_count = 0_u64;
    let mut current_tick: u64 = 0;
    let mut last_progress_tick: u64 = 0;

    // 预计算总 tick 用于进度
    let total_ticks = player.total_ticks().max(1);
    let start_time = std::time::Instant::now();

    while let Some((tick, _track_idx, kind)) = player.next_event() {
        // 进度报告（每约 1% 的 tick 进度）
        if tick - last_progress_tick > total_ticks / 100 {
            let pct = tick as f64 / total_ticks as f64 * 100.0;
            let elapsed = start_time.elapsed();
            eprint!(
                "\r  进度: {:5.1}% | 事件: {:>8} | 音符: {:>8} | 耗时: {:>6.1}s",
                pct, event_count, note_count, elapsed.as_secs_f64()
            );
            last_progress_tick = tick;
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
        match kind {
            TrackEventKind::Midi { channel, message } => {
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
                            ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(
                                program.as_int(),
                            )),
                        ));
                    }
                    MidiMessage::PitchBend { bend } => {
                        renderer.send_event(SynthEvent::Channel(
                            ch,
                            ChannelEvent::Audio(ChannelAudioEvent::Control(
                                ControlEvent::PitchBendValue(
                                    bend.as_int() as f32 / 8192.0 - 1.0,
                                ),
                            )),
                        ));
                    }
                    _ => {}
                }
            }
            _ => {} // 跳过 Meta / SysEx
        }
    }

    // 完成进度条
    let elapsed = start_time.elapsed();
    eprintln!(
        "\r  进度: 100.0% | 事件: {:>8} | 音符: {:>8} | 耗时: {:>6.1}s",
        event_count, note_count, elapsed.as_secs_f64()
    );

    info!("流式渲染完成: 处理 {event_count} 个事件, {note_count} 个音符");
    Ok(())
}

// ════════════════════════════════════════════════════════════
// 文档渲染主循环（MidiDocument）
// ════════════════════════════════════════════════════════════

fn run_document_render_loop(
    renderer: &mut AudioRenderer,
    tick_conv: &mut TickToTime,
    events: &[MergedEvent],
) -> ExportResult<()> {
    info!("文档渲染循环开始 ({})...", events.len());
    let mut event_count = 0_u64;
    let mut current_tick: u64 = 0;

    // 按 tick 逐批处理：同一 tick 的事件先累积再发送
    let mut batch_start = 0_usize;
    while batch_start < events.len() {
        let tick = events[batch_start].tick as u64;

        // 找到同一 tick 的事件范围
        let mut batch_end = batch_start + 1;
        while batch_end < events.len() && events[batch_end].tick == events[batch_start].tick {
            batch_end += 1;
        }

        // 前进时间
        if tick > current_tick {
            let delta = tick_conv.advance_to(tick);
            if delta > 0.0 {
                renderer.render_batch(delta);
            }
            current_tick = tick;
        }

        // 处理本 tick 的所有事件
        for i in batch_start..batch_end {
            let e = &events[i];
            let ch = e.channel as u32;
            match e.kind {
                0 => {
                    // NoteOn
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                            key: e.param1,
                            vel: e.param2 as u8,
                        }),
                    ));
                    event_count += 1;
                }
                1 => {
                    // NoteOff
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                            key: e.param1,
                        }),
                    ));
                    event_count += 1;
                }
                2 => {
                    // CC
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                            e.param1,
                            e.param2 as u8,
                        ))),
                    ));
                }
                3 => {
                    // PC
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(e.param1)),
                    ));
                }
                4 => {
                    // PB
                    let normalized = (e.param2 as i16 - 8192) as f32 / 8192.0;
                    renderer.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(
                            ControlEvent::PitchBendValue(normalized),
                        )),
                    ));
                }
                _ => {}
            }
        }

        batch_start = batch_end;
    }

    info!("文档渲染完成: 处理 {event_count} 个事件");
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

// ════════════════════════════════════════════════════════════
// MidiDocument → 合并事件列表
// ════════════════════════════════════════════════════════════

/// 从 MidiDocument 构建按 tick 排序的合并事件列表。
///
/// - 每个 `NoteEvent` 拆为 NoteOn(start_tick) + NoteOff(end_tick)
/// - `PackedControlEvent` 按 kind 映射为 CC/PC/PB
/// - 结果按 tick 升序，同 tick 内 NoteOff → CC → PC → PB → NoteOn
fn build_document_events(doc: &MidiDocument) -> Vec<MergedEvent> {
    let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
    let mut events = Vec::with_capacity(total_notes * 2 + doc.control_events.len());

    // 1. 音符 → NoteOn + NoteOff
    for track_notes in &doc.notes {
        for note in track_notes {
            events.push(MergedEvent {
                tick: note.start_tick,
                kind: 0, // NoteOn
                channel: note.channel,
                param1: note.key,
                param2: note.velocity as u16,
            });
            events.push(MergedEvent {
                tick: note.end_tick,
                kind: 1, // NoteOff
                channel: note.channel,
                param1: note.key,
                param2: 0,
            });
        }
    }

    // 2. 控制事件
    for ctrl in &doc.control_events {
        let (kind, chan, p1, p2): (u8, u8, u8, u16) = match ctrl.kind {
            0 => {
                let (c, v) = ctrl.as_control_change();
                (2, ctrl.channel, c, v as u16)
            }
            1 => {
                let p = ctrl.as_program_change();
                (3, ctrl.channel, p, 0)
            }
            2 => {
                (4, ctrl.channel, 0, ctrl.param)
            }
            _ => continue,
        };
        events.push(MergedEvent { tick: ctrl.tick, kind, channel: chan, param1: p1, param2: p2 });
    }

    // 3. 排序：tick → 事件类型优先级
    // NoteOff(1) < CC(2) < PC(3) < PB(4) < NoteOn(0→映射为5)
    events.sort_unstable_by_key(|e| (e.tick, if e.kind == 0 { 5 } else { e.kind as u32 }));

    events
}
