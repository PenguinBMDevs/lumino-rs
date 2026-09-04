//! 渲染循环 — MIDI 渲染的主循环逻辑
//!
//! 包含公共 API（render_audio / render_audio_from_document）和内部渲染循环。
//! 流式模式从磁盘 MIDI 流式读取，内存模式复用已加载的 MidiDocument。

use tracing::info;

use midly::{MidiMessage, TrackEventKind};

use lumino_midi_loader::{MidiDocument, streaming::StreamingMidiPlayer};

use crate::error::{ExportError, ExportResult};

use super::config::AudioRenderConfig;
use super::engine::AudioEngine;
use super::event::MidiEventProcessor;
use super::event_kind::{build_track_event_kind, compute_total_tick};
use super::event_stream::MidiDocEventStream;
use super::sink_factory::create_output_sink;
use super::tick_conv::TickToTime;

// ════════════════════════════════════════════════════════════
// 公共 API
// ════════════════════════════════════════════════════════════

/// 流式模式：直接从磁盘 MIDI 文件渲染为音频文件
pub fn render_audio(config: &AudioRenderConfig) -> ExportResult<()> {
    if let Some(ctrl) = &config.control {
        ctrl.check_abort()?;
    }
    // GPU 后端优先尝试
    if config.backend == super::config::AudioBackendKind::Gpu {
        match super::gpu_backend::render_audio_gpu_streaming(config) {
            Ok(()) => return Ok(()),
            Err(e) => {
                if matches!(e, ExportError::Aborted) {
                    return Err(e);
                }
                tracing::warn!("GPU 渲染失败，回退到 CPU: {e}");
                if matches!(config.backend, super::config::AudioBackendKind::Gpu) {
                    // 如果 GPU 明确请求但失败，返回错误让调用方感知（而非静默回退）
                    // 此处保留回退逻辑以保证导出可用性
                }
            }
        }
    }

    info!(
        "[流式] 音频渲染: MIDI={:?}, SF2={:?}, 输出={:?} [backend={}, sr={}, ch={:?}]",
        config.midi_path,
        config.soundfonts,
        config.output_path,
        config.backend,
        config.sample_rate,
        config.channels
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

    let mut sink = create_output_sink(config)?;
    let mut engine = AudioEngine::new(config.clone())?;
    let mut tick_conv = TickToTime::new(tempos, ppqn);
    let mut processor =
        MidiEventProcessor::new(config, engine.channel_group(), &mut tick_conv, &mut sink);

    run_streaming_render(config, &mut processor, player)?;

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
    if let Some(ctrl) = &config.control {
        ctrl.check_abort()?;
    }
    // GPU 后端（SFZ 会自动回退到 CPU，保证导出可用）
    if config.backend == super::config::AudioBackendKind::Gpu {
        match super::gpu_backend::render_audio_gpu_from_document(config, doc) {
            Ok(()) => return Ok(()),
            Err(e) => {
                if matches!(e, ExportError::Aborted) {
                    return Err(e);
                }
                // SFZ 等 GPU 不支持的格式，warn 后回退到 CPU
                let msg = e.to_string();
                if msg.contains("SFZ") || msg.contains("sfz") {
                    tracing::warn!("GPU 不支持 SFZ，已自动回退到 CPU 渲染: {e}");
                } else {
                    tracing::warn!("GPU 渲染失败，回退到 CPU: {e}");
                }
            }
        }
    }

    let total_events: usize =
        doc.notes.iter().map(|v| v.len()).sum::<usize>() * 2 + doc.control_events.len();
    if total_events == 0 {
        return Err(ExportError::AudioWrite(
            "MIDI 文档中没有可渲染的事件".into(),
        ));
    }

    let ppqn = u32::from(doc.division.max(1));
    info!(
        "[内存] 音频渲染: SF2={:?}, 输出={:?} [backend={}, sr={}, ch={:?}, ppqn={}, division={}]",
        config.soundfonts,
        config.output_path,
        config.backend,
        config.sample_rate,
        config.channels,
        ppqn,
        doc.division
    );

    let mut sink = create_output_sink(config)?;
    let mut engine = AudioEngine::new(config.clone())?;
    let tempos = doc.tempo_changes.clone();
    // PPQ 分辨率必须与文档一致：音符/事件 tick 基于 `doc.division`
    // （MIDI 文件头 PPQ，或编辑器当前 PPQ）。此前硬编码 480 导致
    // 非 480 PPQ 文档（如 192/960/1920）的 tick→秒换算被放大
    // （division/480 倍），导出音频时长错误、速度减慢但音调正常。
    let mut tick_conv = TickToTime::new(tempos, ppqn);
    let mut processor =
        MidiEventProcessor::new(config, engine.channel_group(), &mut tick_conv, &mut sink);
    let total_tick = compute_total_tick(doc);

    run_document_render(config, &mut processor, doc, total_tick)?;

    processor.finalize()?;
    sink.finalize()?;

    info!("文档音频渲染完成: {:?}", config.output_path);
    Ok(())
}

// ════════════════════════════════════════════════════════════
// 内部渲染循环
// ════════════════════════════════════════════════════════════

/// 流式渲染主循环（从 StreamingMidiPlayer 读取事件）
pub(super) fn run_streaming_render(
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
        if let Some(ctrl) = &config.control {
            ctrl.wait_if_paused();
            ctrl.check_abort()?;
        }
        let now = std::time::Instant::now();
        if now.duration_since(last_progress_time) >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_ticks as f64;
            report_progress(config, pct, event_count, note_count, start_time);
            last_progress_time = now;
        }

        processor.process_midi_event(tick, &kind)?;

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

    report_progress(config, 1.0, event_count, note_count, start_time);
    info!("流式渲染完成: 处理 {event_count} 个事件, {note_count} 个音符");
    Ok(())
}

/// 文档渲染主循环（从 MidiDocument 读取事件）
pub(super) fn run_document_render(
    config: &AudioRenderConfig,
    processor: &mut MidiEventProcessor,
    doc: &MidiDocument,
    total_tick: u64,
) -> ExportResult<()> {
    let mut event_count = 0_u64;
    let mut last_progress_time = std::time::Instant::now();
    let start_time = std::time::Instant::now();

    let mut stream = MidiDocEventStream::new(doc);
    let total_events = stream.total_events();

    info!("文档流式渲染循环开始 ({} 事件)...", total_events);

    while let Some(event) = stream.next_event() {
        if let Some(ctrl) = &config.control {
            ctrl.wait_if_paused();
            ctrl.check_abort()?;
        }
        let tick = event.tick as u64;

        let now = std::time::Instant::now();
        if now.duration_since(last_progress_time) >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_tick as f64;
            report_progress(config, pct, event_count, 0, start_time);
            last_progress_time = now;
        }

        if let Some(kind) = build_track_event_kind(&event) {
            processor.process_midi_event(tick, &kind)?;
            event_count += 1;
        }
    }

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
