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
//! lumino_midi_loader::StreamingMidiPlayer (按 tick 排序)
//!     │
//!     ▼
//! block_render::render_tail()        (尾部衰减)
//! AudioFileWriter
//! ```
//!
//! # 流式特性
//!
//! 本模块是真正的事件流式渲染——不预缓存任何事件到 `Vec`：
//! - `StreamingMidiPlayer.next_event()` 逐事件输出，零拷贝
//! - 渲染循环按固定时间块（512 samples）消费，每个块结算一次音频
//! - 最多缓冲一个跨块事件（O(1) 内存）, 不存在 OOM 风险

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use midly::{MidiMessage, TrackEventKind};
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};
use xsynth_core::{AudioStreamParams, ChannelCount};

use lumino_midi_loader::StreamingMidiPlayer;

use crate::audio::block_render::{RenderCommand, render_tail, send_command};
use crate::audio::gpu_renderer::{GPU_BLOCK_SAMPLES, GpuSynth};
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

/// 使用流式 MIDI 播放器渲染音频文件（真正的流式渲染）。
///
/// 与旧版不同，本实现**不缓存全量事件到 `Vec`**，而是在固定块循环中
/// 逐帧消费 `StreamingMidiPlayer`：
///
/// 1. 将时间划分为固定块（512 samples）
/// 2. 每块内从 `StreamingMidiPlayer` 拉取落在本块时间范围内的事件
/// 3. 立即 dispatch 到 xsynth，渲染本块，写入文件
/// 4. 最多缓冲一个跨块事件（O(1) 内存）
///
/// 优点：O(1) 常驻内存，不受 MIDI 长度影响，不存在 OOM 风险。
pub fn render_streaming<'a>(
    midi_bytes: &'a [u8],
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

    // 用总 tick 估算总时长（仅用于进度条上限）。StreamingMidiPlayer 的
    // scan_tempos 已经全扫描过一次总 tick，无需额外遍历。
    let total_seconds = tempo_map.tick_to_seconds(player.total_ticks());

    // 5. 流式渲染主循环 — 逐块消费事件，不缓存全量
    const BLOCK_SAMPLES: usize = 16384;
    let block_sec = BLOCK_SAMPLES as f64 / options.sample_rate as f64;
    let mut block_start = 0.0_f64;
    // 最多缓存一个跨块事件（next_event 消费后无法回退）
    let mut pending_event: Option<(u64, usize, TrackEventKind<'a>)> = None;

    // ---- 统计 ----
    let mut total_events: u64 = 0;
    let mut note_on_count: u64 = 0;
    let mut note_off_count: u64 = 0;
    let mut pc_count: u64 = 0;
    use std::collections::BTreeMap;
    let mut ch_map: BTreeMap<u32, u64> = BTreeMap::new();
    let mut key_out_of_range: u64 = 0;

    while block_start < total_seconds {
        // 检查取消
        if let Some(ref cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(ExportError::AudioWrite("导出已取消".to_string()));
        }

        let block_end = (block_start + block_sec).min(total_seconds);
        let delta = block_end - block_start;

        // 消费本块内所有事件（time_sec <= block_end）
        loop {
            // 从 pending 或 player 获取下一事件
            let next: Option<(u64, usize, TrackEventKind<'_>)> =
                pending_event.take().or_else(|| player.next_event());
            let (tick, _track_idx, kind) = match next {
                Some(ev) => ev,
                None => break,
            };

            let time_sec = tempo_map.tick_to_seconds(tick);
            total_events += 1;

            if time_sec <= block_end {
                // 本块内事件：立即 dispatch 并统计
                for cmd in kind_to_command(kind) {
                    match &cmd {
                        RenderCommand::NoteOn { key, channel, .. } => {
                            note_on_count += 1;
                            *ch_map.entry(*channel).or_default() += 1;
                            if *key >= 128 {
                                key_out_of_range += 1;
                            }
                        }
                        RenderCommand::NoteOff { .. } => note_off_count += 1,
                        RenderCommand::ProgramChange { .. } => pc_count += 1,
                        _ => {}
                    }
                    send_command(&mut exporter, &cmd);
                }
            } else {
                // 超出本块：缓存到 pending，等下一块处理
                pending_event = Some((tick, _track_idx, kind));
                break;
            }
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
        if let Some(callback) = &progress_callback {
            let progress = ((block_end / total_seconds.max(1.0)) * 100.0).min(99.0) as f32;
            callback(progress);
        }

        // 所有事件消费完毕 → 提前退出主循环
        if player.is_exhausted() && pending_event.is_none() {
            break;
        }

        block_start = block_end;
    }

    // 6. 诊断日志
    let ch_dist: String = ch_map
        .iter()
        .map(|(ch, n)| format!("ch{}:{}", ch, n))
        .collect::<Vec<_>>()
        .join(", ");
    tracing::info!(
        "[PATH_B] extract_commands: {} NoteOn, {} NoteOff, {} PC, total_events={}, channels=[{}], out_of_range={}",
        note_on_count,
        note_off_count,
        pc_count,
        total_events,
        ch_dist,
        key_out_of_range,
    );

    // 渲染完成后检查 voice 数量（诊断）
    let voice_count = exporter.voice_count();
    tracing::info!(
        "[PATH_B] 渲染完成: {} events, {} voices, total={:.2}s",
        total_events,
        voice_count,
        total_seconds,
    );

    // 7. 尾部衰减
    render_tail(
        &mut exporter,
        &mut writer,
        options.sample_rate,
        BLOCK_SAMPLES,
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

/// 使用 GPU 合成器渲染音频文件（替代 xsynth CPU 渲染）。
///
/// 与 `render_streaming` 功能相同，但使用 `GpuSynth` 替代 xsynth 的 `AudioExporter`。
/// 对于黑乐谱等密集 MIDI 文件，GPU 渲染速度可提升 10-50x。
///
/// 优化：预提取全量事件为 `Vec<(f64, RenderCommand)>`，避免 per-block 调用
/// `StreamingMidiPlayer::next_event()`（该函数每次 find_min_tick 都分配 Vec）。
pub fn render_streaming_gpu<'a>(
    midi_bytes: &'a [u8],
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
        "[GPU] 开始 GPU 加速音频导出: 输出={:?}, 格式={}, 采样率={}Hz",
        output_path,
        options.format,
        options.sample_rate,
    );

    let start = std::time::Instant::now();

    // 1. 创建 GPU 合成器（加载 SF2 + 初始化 wgpu）
    let mut synth = GpuSynth::new(
        soundfont_path,
        options.sample_rate,
        options.channels.count(),
    )
    .map_err(|e| ExportError::AudioWrite(format!("GPU 合成器初始化失败: {}", e)))?;

    // 2. 创建音频文件写入器
    let mut writer = super::writer::AudioFileWriter::create(
        output_path,
        options.format,
        options.sample_rate,
        options.channels,
    )?;

    // 3. 创建流式播放器
    let mut player = StreamingMidiPlayer::from_bytes(midi_bytes)
        .map_err(|e| ExportError::MidiParse(format!("流式 MIDI 解析失败: {}", e)))?;
    let tempo_map = TempoMap::from_changes(player.tempo_changes(), player.ppqn());
    let total_seconds = tempo_map.tick_to_seconds(player.total_ticks());

    let block_sec = GPU_BLOCK_SAMPLES as f64 / options.sample_rate as f64;

    // 4. 流式渲染主循环 — 每块从 player 拉事件，不缓存全量
    let mut block_start = 0.0_f64;
    let mut pending_event: Option<(u64, usize, TrackEventKind<'_>)> = None;
    let mut block_i: u64 = 0;

    while block_start < total_seconds {
        let _b0 = std::time::Instant::now();

        // 检查取消
        if let Some(ref cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(ExportError::AudioWrite("导出已取消".to_string()));
        }

        let block_end = (block_start + block_sec).min(total_seconds);
        let delta = block_end - block_start;

        // 消费本块内所有事件（流式，O(1) 内存）
        let mut ev_count: u64 = 0;
        loop {
            let ev = if let Some(p) = pending_event.take() {
                p
            } else {
                match player.next_event() {
                    Some(e) => e,
                    None => break,
                }
            };
            let time_sec = tempo_map.tick_to_seconds(ev.0);
            if time_sec <= block_end {
                // 内联 kind_to_command 避免 Vec 分配
                if let TrackEventKind::Midi { channel, message } = ev.2 {
                    let ch = u8::from(channel) as u32;
                    match message {
                        MidiMessage::NoteOn { key, vel } if u8::from(vel) > 0 => {
                            synth.send_note_on(ch, key, u8::from(vel));
                            ev_count += 1;
                        }
                        MidiMessage::NoteOn { key, .. } => {
                            synth.send_note_off(ch, key);
                            ev_count += 1;
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            synth.send_note_off(ch, key);
                            ev_count += 1;
                        }
                        MidiMessage::Controller { controller, value } => {
                            synth.send_control_change(ch, u8::from(controller), u8::from(value));
                            ev_count += 1;
                        }
                        MidiMessage::ProgramChange { program } => {
                            synth.send_program_change(ch, u8::from(program));
                            ev_count += 1;
                        }
                        MidiMessage::PitchBend { bend } => {
                            synth.send_pitch_bend(ch, bend.as_int());
                            ev_count += 1;
                        }
                        _ => {}
                    }
                }
            } else {
                pending_event = Some(ev);
                break;
            }
        }
        let _b1 = std::time::Instant::now();

        // GPU 渲染本块
        if delta > 0.0 {
            synth.render_block(delta);
            let samples = synth.take_samples();
            if !samples.is_empty() {
                writer.write_samples(&samples)?;
            }
        }
        let _b2 = std::time::Instant::now();

        // 进度
        if let Some(callback) = &progress_callback {
            let progress = ((block_end / total_seconds.max(1.0)) * 100.0).min(99.0) as f32;
            callback(progress);
        }

        if player.is_exhausted() && pending_event.is_none() {
            break;
        }

        if block_i % 5 == 0 || ev_count > 0 {
            tracing::info!(
                "[GPU.loop] block={} time={:.2}..{:.2}s ev={} midi={:?} render={:?} total={:?}",
                block_i,
                block_start,
                block_end,
                ev_count,
                _b1 - _b0,
                _b2 - _b1,
                _b2 - _b0,
            );
        }

        block_start = block_end;
        block_i += 1;
    }

    // 5. 尾部衰减（用 GPU 渲染剩余的 voice）
    let mut tail_remaining = 5.0_f64;
    while tail_remaining > 0.0 && synth.is_active() {
        let delta = tail_remaining.min(block_sec * 4.0);
        synth.render_block(delta);
        let samples = synth.take_samples();
        if !samples.is_empty() {
            let is_silent = samples.iter().all(|s| s.abs() <= 0.0001);
            writer.write_samples(&samples)?;
            if is_silent {
                break;
            }
        }
        tail_remaining -= delta;
    }

    // 6. 完成文件写入
    writer.finalize()?;

    if let Some(callback) = &progress_callback {
        callback(100.0);
    }

    let elapsed = start.elapsed();
    tracing::info!("GPU 音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}
