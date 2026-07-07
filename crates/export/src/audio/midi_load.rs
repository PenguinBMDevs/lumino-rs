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

use crate::audio::block_render::{RenderCommand, render_tail, send_command};
use crate::audio::gpu_renderer::{GPU_BLOCK_SAMPLES, GpuSynth, PendingRender, RawEvent};
use crate::audio::tempo::TempoMap;
use crate::audio::types::{AudioExportOptions, ExportProgress};
use crate::error::{ExportError, ExportResult};
use lumino_midi_loader::StreamingMidiPlayer;

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
    progress_callback: Option<Arc<dyn Fn(ExportProgress) + Send + Sync>>,
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

    // 4. 创建流式播放器 + TempoCursor（O(1) amortized tick→秒）
    let mut player = StreamingMidiPlayer::from_bytes(midi_bytes)
        .map_err(|e| ExportError::MidiParse(format!("流式 MIDI 解析失败: {}", e)))?;
    let tempo_map = TempoMap::from_changes(player.tempo_changes(), player.ppqn());
    let total_seconds = tempo_map.tick_to_seconds(player.total_ticks());
    let mut tempo_cur = tempo_map.cursor();

    // 5. 流式渲染主循环 — PA+PB 一起跑，per-block 逐块消费
    const BLOCK_SAMPLES: usize = 16384;
    let block_sec = BLOCK_SAMPLES as f64 / options.sample_rate as f64;
    let mut block_start = 0.0_f64;
    let mut pending_event: Option<(u64, usize, TrackEventKind<'a>)> = None;

    // ---- 统计 ----
    let mut total_events: u64 = 0;
    let mut note_on_count: u64 = 0;
    let mut note_off_count: u64 = 0;
    let mut pc_count: u64 = 0;
    use std::collections::BTreeMap;
    let mut ch_map: BTreeMap<u32, u64> = BTreeMap::new();
    let mut key_out_of_range: u64 = 0;

    // ---- 实时进度 ----
    let mut last_progress = std::time::Instant::now();

    while block_start < total_seconds {
        if let Some(ref cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(ExportError::AudioWrite("导出已取消".to_string()));
        }

        let block_end = (block_start + block_sec).min(total_seconds);
        let delta = block_end - block_start;

        // 消费本块内所有事件 — 用 TempoCursor 而非 TempoMap::tick_to_seconds，
        // 前者 O(1) amortized，后者 O(num_tempo_changes) 每事件从头扫描
        loop {
            let next = pending_event.take().or_else(|| player.next_event());
            let (tick, _track_idx, kind) = match next {
                Some(ev) => ev,
                None => break,
            };

            let time_sec = tempo_cur.advance_to(tick);
            total_events += 1;

            if time_sec <= block_end {
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
                pending_event = Some((tick, _track_idx, kind));
                break;
            }

            // 实时进度：每 20ms 一次，基于事件时间位置
            let now = std::time::Instant::now();
            if now.duration_since(last_progress).as_millis() >= 20 {
                last_progress = now;
                if let Some(ref cb) = progress_callback {
                    cb(ExportProgress {
                        progress: ((time_sec / total_seconds.max(1.0)) * 100.0).min(99.0) as f32,
                        note_on: note_on_count,
                        note_off: note_off_count,
                    });
                }
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

        // 渲染后进度 — 每 50ms 或 block 结束
        let now = std::time::Instant::now();
        if now.duration_since(last_progress).as_millis() >= 50 || block_end >= total_seconds {
            last_progress = now;
            if let Some(ref cb) = progress_callback {
                cb(ExportProgress {
                    progress: ((block_end / total_seconds.max(1.0)) * 100.0).min(99.0) as f32,
                    note_on: note_on_count,
                    note_off: note_off_count,
                });
            }
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
        callback(ExportProgress {
            progress: 100.0,
            note_on: 0,
            note_off: 0,
        });
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
pub fn render_streaming_gpu(
    midi_bytes: &[u8],
    soundfont_path: &Path,
    output_path: &Path,
    options: &AudioExportOptions,
    progress_callback: Option<Arc<dyn Fn(ExportProgress) + Send + Sync>>,
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

    // 3. 使用 StreamingMidiPlayer + TempoCursor，事件流式直通 GPU
    //    双缓冲流水线：submit() 非阻塞，GPU 渲染 block N 的同时 CPU 抽 block N+1 的事件
    let mut player = StreamingMidiPlayer::from_bytes(midi_bytes)
        .map_err(|e| ExportError::MidiParse(format!("流式 MIDI 解析失败: {}", e)))?;
    let tempo_map = TempoMap::from_changes(player.tempo_changes(), player.ppqn());
    let total_seconds = tempo_map.tick_to_seconds(player.total_ticks());
    let mut tempo_cur = tempo_map.cursor();

    let block_sec = GPU_BLOCK_SAMPLES as f64 / options.sample_rate as f64;
    let sample_rate_f = options.sample_rate as f64;
    let mut block_start = 0.0_f64;
    let mut pending: Option<(u64, usize, TrackEventKind<'_>)> = None;
    let mut total_note_on: u64 = 0;
    let mut total_note_off: u64 = 0;
    let mut last_progress = std::time::Instant::now();
    // 预分配事件 buffer + GPU 流水线句柄
    let mut raw_events: Vec<RawEvent> = Vec::with_capacity(2048);
    let mut gpu_pending: Option<PendingRender> = None;
    let mut block_idx: u64 = 0;

    while block_start < total_seconds {
        if let Some(ref cancel) = cancel_flag
            && cancel.load(Ordering::Relaxed)
        {
            return Err(ExportError::AudioWrite("导出已取消".to_string()));
        }

        let block_end = (block_start + block_sec).min(total_seconds);
        let block_start_smp = (block_start * sample_rate_f) as u32;

        // ── Step 1: 读回上一 block 的音频（GPU 已完成，不阻塞 CPU） ──
        if let Some(p) = gpu_pending.take() {
            let audio = synth.readback_audio(&p);
            if !audio.is_empty() {
                // 诊断：样本统计（每 10 block 或第一个 block 输出）
                if block_idx % 10 == 0 || block_idx == 0 {
                    let min_s = audio.iter().copied().fold(f32::MAX, f32::min);
                    let max_s = audio.iter().copied().fold(f32::MIN, f32::max);
                    let mean_s = audio.iter().sum::<f32>() / audio.len() as f32;
                    let clipped = audio.iter().filter(|&&s| s.abs() >= 1.0).count();
                    let first_32: Vec<f32> = audio.iter().take(32).copied().collect();
                    // 计算前 64 个 sample 中非零的数量
                    let non_zero = audio.iter().take(64).filter(|&&s| s != 0.0).count();
                    tracing::info!(
                        "[GPU_DIAG] block={}: events={} samples={} min={:.4} max={:.4} mean={:.4} clipped={} nz64={} first32={:.4?}",
                        block_idx,
                        raw_events.len(),
                        audio.len(),
                        min_s,
                        max_s,
                        mean_s,
                        clipped,
                        non_zero,
                        first_32,
                    );
                }
                writer.write_samples(&audio)?;
            }
        }

        // ── Step 2: 抽本 block 的事件（CPU 工作，GPU 同时在渲染上一 block） ──
        raw_events.clear();
        loop {
            let ev = if let Some(p) = pending.take() {
                p
            } else {
                match player.next_event() {
                    Some(e) => e,
                    None => break,
                }
            };
            let time_sec = tempo_cur.advance_to(ev.0);
            if time_sec <= block_end {
                // to = 块内 sample offset。当 time_sec == block_end 时，
                // to 可能等于 GPU_BLOCK_SAMPLES，超出 sidx 范围 0..ns-1。
                // clamp 到 ns-1，使音符在块末尾触发、下一块继续。
                let to = ((time_sec * sample_rate_f) as u32 - block_start_smp)
                    .min(GPU_BLOCK_SAMPLES - 1);
                if let TrackEventKind::Midi { channel, message } = ev.2 {
                    let ch = u8::from(channel) as u32;
                    match message {
                        MidiMessage::NoteOn { key, vel } => {
                            let v = u8::from(vel);
                            total_note_on += 1;
                            raw_events.push(RawEvent::new(to, 0, ch, key as u32, v as u32));
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            total_note_off += 1;
                            raw_events.push(RawEvent::new(to, 1, ch, key as u32, 0));
                        }
                        _ => {}
                    }
                }
            } else {
                pending = Some(ev);
                break;
            }

            // 实时进度：每 20ms 一次
            let now = std::time::Instant::now();
            if now.duration_since(last_progress).as_millis() >= 20 {
                last_progress = now;
                if let Some(ref cb) = progress_callback {
                    cb(ExportProgress {
                        progress: ((time_sec / total_seconds.max(1.0)) * 100.0).min(99.0) as f32,
                        note_on: total_note_on,
                        note_off: total_note_off,
                    });
                }
            }
        }

        // ── Step 3: 非阻塞提交给 GPU（不等待，GPU 立即开始渲染） ──
        // 诊断：记录事件的 to 值分布
        if block_idx < 10 || block_idx % 100 == 0 {
            let mut to_min = u32::MAX;
            let mut to_max = u32::MIN;
            for ev in &raw_events {
                to_min = to_min.min(ev.tick_offset);
                to_max = to_max.max(ev.tick_offset);
            }
            tracing::info!(
                "[GPU_EV] block={}: n_events={} to_range=[{},{}]",
                block_idx,
                raw_events.len(),
                to_min,
                to_max,
            );
        }
        gpu_pending = Some(synth.submit(&raw_events));
        block_idx += 1;

        // 渲染后进度
        let now = std::time::Instant::now();
        if now.duration_since(last_progress).as_millis() >= 50 || block_end >= total_seconds {
            last_progress = now;
            if let Some(ref cb) = progress_callback {
                cb(ExportProgress {
                    progress: ((block_end / total_seconds.max(1.0)) * 100.0).min(99.0) as f32,
                    note_on: total_note_on,
                    note_off: total_note_off,
                });
            }
        }

        // 所有事件消费完毕 → 提前退出（最后一块由循环后 readback）
        if player.is_exhausted() && pending.is_none() {
            break;
        }

        block_start = block_end;
    }

    // 读回最后一块的音频
    if let Some(p) = gpu_pending.take() {
        let audio = synth.readback_audio(&p);
        if !audio.is_empty() {
            let min_s = audio.iter().copied().fold(f32::MAX, f32::min);
            let max_s = audio.iter().copied().fold(f32::MIN, f32::max);
            let clipped = audio.iter().filter(|&&s| s.abs() >= 1.0).count();
            tracing::info!(
                "[GPU_DIAG] final_block: min={:.4} max={:.4} clipped={}",
                min_s,
                max_s,
                clipped,
            );
            writer.write_samples(&audio)?;
        }
    }

    // 5. 尾部衰减（用 GPU 渲染剩余的 voice，无新事件）
    let mut tail_remaining = 5.0_f64;
    while tail_remaining > 0.0 && synth.is_active() {
        let p = synth.submit(&[]);
        let samples = synth.readback_audio(&p);
        if !samples.is_empty() {
            let is_silent = samples.iter().all(|s| s.abs() <= 0.0001);
            writer.write_samples(&samples)?;
            if is_silent {
                break;
            }
        }
        tail_remaining -= block_sec.max(0.1);
    }

    // 6. 完成文件写入
    writer.finalize()?;

    if let Some(callback) = &progress_callback {
        callback(ExportProgress {
            progress: 100.0,
            note_on: total_note_on,
            note_off: total_note_off,
        });
    }

    let elapsed = start.elapsed();
    tracing::info!("GPU 音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}
