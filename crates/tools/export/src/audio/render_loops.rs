//! 渲染循环 — MIDI 渲染的主循环逻辑
//!
//! 包含公共 API（render_audio / render_audio_from_document）和内部渲染循环。
//! 流式模式从磁盘 MIDI 流式读取，内存模式复用已加载的 MidiDocument。

use tracing::info;

use midly::{MidiMessage, TrackEventKind};

use lumino_midi_loader::{MidiDocument, streaming::StreamingMidiPlayer};
use lumino_midi_model::multi_port::MAX_PORTS;

use crate::error::{ExportError, ExportResult};

use super::block_scheduler::RenderCursor;
use super::config::AudioRenderConfig;
use super::engine::AudioEngine;
use super::event::MidiEventProcessor;
use super::event_kind::{build_track_event_kind, compute_total_tick};
use super::event_stream::MidiDocEventStream;
use super::sink_factory::create_output_sink;
use super::speed::{DEFAULT_SPEED_WINDOW_SECS, ExportSpeedMeter};
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

    // REND-002：多端口由文件预扫描推导（覆盖 UI 传入的 0），并做 B1 超限告警。
    let mut config = config.clone();
    config.midi_max_port = player.max_port();
    let config = &config;
    warn_port_overflow("流式导出", player.track_ports());

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
    // 音频总时长：供进度回调估算"导出倍速"（音频秒/墙钟秒）。
    let total_seconds = tick_conv.tick_to_seconds(total_ticks);
    let mut processor =
        MidiEventProcessor::new(config, engine.channel_group(), &mut tick_conv, &mut sink);

    run_streaming_render(config, &mut processor, player, total_seconds)?;

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
    // REND-002：多端口由文档端口信息推导（覆盖 UI 传入的 0），并做 B1 超限告警。
    let mut config = config.clone();
    config.midi_max_port = doc.max_port();
    let config = &config;
    warn_port_overflow("内存导出", &doc.track_ports);
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
    let total_tick = compute_total_tick(doc);
    // 音频总时长：供进度回调估算"导出倍速"（音频秒/墙钟秒）。
    let total_seconds = tick_conv.tick_to_seconds(total_tick);
    let mut processor =
        MidiEventProcessor::new(config, engine.channel_group(), &mut tick_conv, &mut sink);

    run_document_render(config, &mut processor, doc, total_tick, total_seconds)?;

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
    total_seconds: f64,
) -> ExportResult<()> {
    let total_ticks = player.total_ticks().max(1);
    let mut event_count = 0_u64;
    let mut note_count = 0_u64;
    let mut last_progress_time = std::time::Instant::now();
    let start_time = std::time::Instant::now();
    // 倍速用 3s 窗口平均；tick 占比 × 总时长是音频时长的近似（tempo 变化时略有偏差，
    // 对"倍速"展示足够）。
    let mut speed_meter = ExportSpeedMeter::new(DEFAULT_SPEED_WINDOW_SECS);
    // 块式渲染游标（PREF-002）：事件按 B 帧块对齐，块首一次性投递后整块渲染
    let mut cursor = RenderCursor::new(u64::from(config.effective_block_frames()));
    // REND-002：先取出每轨端口快照，事件循环里按 track_idx 查（零拷贝借用冲突）。
    let track_ports = player.track_ports().to_vec();
    // B1：超上限端口的折叠事件计数（渲染结束后汇总告警）。
    let mut clamped_events = 0_u64;

    while let Some((tick, track_idx, kind)) = player.next_event() {
        if let Some(ctrl) = &config.control {
            ctrl.wait_if_paused();
            ctrl.check_abort()?;
        }
        let now = std::time::Instant::now();
        if now.duration_since(last_progress_time) >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_ticks as f64;
            speed_meter.record(
                now.duration_since(start_time).as_secs_f64(),
                pct * total_seconds,
            );
            report_progress(
                config,
                pct,
                event_count,
                note_count,
                start_time,
                speed_meter.speed(),
            );
            last_progress_time = now;
        }

        // 时间推进由调度器负责（块模式量化到块首；精确模式=逐事件）
        let frame = processor.frame_at_tick(tick);
        let advance = cursor.frames_before(frame);
        if advance > 0 {
            processor.render_frames(advance)?;
        }
        let port = track_ports.get(track_idx).copied().unwrap_or(0);
        if port >= MAX_PORTS {
            clamped_events += 1;
        }
        cursor.add_rendered(processor.dispatch_event(&kind, port)?);

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

    // 收尾：补齐到最后一个事件的精确帧（输出长度与逐事件旧实现一致）
    let remainder = cursor.finish_remainder();
    if remainder > 0 {
        processor.render_frames(remainder)?;
    }

    if clamped_events > 0 {
        tracing::warn!(
            "[REND-002] 流式导出：{clamped_events} 个事件来自超上限端口，已折叠到端口 {} 块发送（B1）",
            MAX_PORTS - 1
        );
    }

    report_progress(
        config,
        1.0,
        event_count,
        note_count,
        start_time,
        speed_meter.speed(),
    );
    info!("流式渲染完成: 处理 {event_count} 个事件, {note_count} 个音符");
    Ok(())
}

/// 文档渲染主循环（从 MidiDocument 读取事件）
pub(super) fn run_document_render(
    config: &AudioRenderConfig,
    processor: &mut MidiEventProcessor,
    doc: &MidiDocument,
    total_tick: u64,
    total_seconds: f64,
) -> ExportResult<()> {
    let mut event_count = 0_u64;
    let mut last_progress_time = std::time::Instant::now();
    let start_time = std::time::Instant::now();
    let mut speed_meter = ExportSpeedMeter::new(DEFAULT_SPEED_WINDOW_SECS);
    let mut cursor = RenderCursor::new(u64::from(config.effective_block_frames()));

    let mut stream = MidiDocEventStream::new(doc);
    let total_events = stream.total_events();
    // B1：超上限端口的折叠事件计数（渲染结束后汇总告警）。
    let mut clamped_events = 0_u64;

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
            speed_meter.record(
                now.duration_since(start_time).as_secs_f64(),
                pct * total_seconds,
            );
            report_progress(config, pct, event_count, 0, start_time, speed_meter.speed());
            last_progress_time = now;
        }

        if let Some(kind) = build_track_event_kind(&event) {
            let frame = processor.frame_at_tick(tick);
            let advance = cursor.frames_before(frame);
            if advance > 0 {
                processor.render_frames(advance)?;
            }
            if event.port >= MAX_PORTS {
                clamped_events += 1;
            }
            cursor.add_rendered(processor.dispatch_event(&kind, event.port)?);
            event_count += 1;
        }
    }

    // 收尾：补齐到最后一个事件的精确帧（输出长度与逐事件旧实现一致）
    let remainder = cursor.finish_remainder();
    if remainder > 0 {
        processor.render_frames(remainder)?;
    }

    if clamped_events > 0 {
        tracing::warn!(
            "[REND-002] 内存导出：{clamped_events} 个事件来自超上限端口，已折叠到端口 {} 块发送（B1）",
            MAX_PORTS - 1
        );
    }

    report_progress(config, 1.0, event_count, 0, start_time, speed_meter.speed());
    info!("文档流式渲染完成: 处理 {event_count} 个事件");
    Ok(())
}

/// B1 口径：端口超出产品上限（[`MAX_PORTS`]）时显式告警，列出超限轨道（截断 20 条）。
///
/// 不做整体拒绝、不静默丢音：事件会按 `effective_port` 折叠到端口 15 块发送；
/// 运行期实际折叠事件数由各渲染循环统计后在结束时汇总告警。
fn warn_port_overflow(source: &str, track_ports: &[u8]) {
    let over: Vec<(usize, u8)> = track_ports
        .iter()
        .enumerate()
        .filter(|entry| *entry.1 >= MAX_PORTS)
        .map(|(idx, &port)| (idx, port))
        .collect();
    if over.is_empty() {
        return;
    }

    let mut detail: Vec<String> = over
        .iter()
        .take(20)
        .map(|(idx, port)| format!("轨{idx}:port{port}"))
        .collect();
    if over.len() > detail.len() {
        detail.push(format!("等 +{}", over.len() - detail.len()));
    }
    tracing::warn!(
        "[REND-002] {source}：{} 条轨道端口超出上限 {}（{}），已折叠到端口 {} 块（B1）；超出部分将共享该块通道状态",
        over.len(),
        MAX_PORTS,
        detail.join(", "),
        MAX_PORTS - 1
    );
}

/// 报告进度
fn report_progress(
    config: &AudioRenderConfig,
    pct: f64,
    event_count: u64,
    note_count: u64,
    start_time: std::time::Instant,
    speed: Option<f64>,
) {
    let elapsed = start_time.elapsed();
    let speed_text = speed.map_or(String::new(), |s| format!(" | {s:.2}× 实时"));
    let msg = format!(
        "进度: {:.1}% | 事件: {} | 音符: {}{speed_text} | 耗时: {:.1}s",
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
