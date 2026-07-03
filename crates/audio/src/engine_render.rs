//! 音频渲染逻辑 — 分两个模式：
//!
//! - **实时播放**（`PreparedModel.notes_by_key == None`）：直接从 ChannelGroup 读音频，
//!   不涉及 PreparedModel 事件派发。所有事件通过 MIDI-stream（PlaybackManager →
//!   AudioCommandAdapter）直接注入 ChannelGroup。
//! - **离线导出**（`notes_by_key == Some(...)`）：sample-accurate 事件派发，由
//!   `render_block` 在精确样本位置派发 NoteOn/NoteOff/CC，确保输出和 MIDI 定时一致。

use xsynth_core::AudioPipe;
use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent};
use xsynth_core::channel_group::SynthEvent;

use crate::audio_model::{ActiveNote, tick_to_sample};

/// 渲染游标 — 跟踪当前播放位置和已渲染位置。
pub(crate) struct RenderCursor {
    pub(crate) position: u64,
}

impl RenderCursor {
    pub(crate) fn new() -> Self {
        Self { position: 0 }
    }

    pub(crate) fn set_position(&mut self, sample: u64) {
        self.position = sample;
    }

    pub(crate) fn advance(&mut self, samples: u64) {
        self.position += samples;
    }
}

/// 渲染一块音频到 output buffer（interleaved stereo: L, R, L, R, ...）。
///
/// 渲染模式由 `model.notes_by_key` 决定：
/// - `None` → 实时播放模式：纯 `read_samples`，无事件派发
/// - `Some(...)` → 离线导出模式：sample-accurate 事件派发 + `read_samples`
///
/// 返回实际渲染的帧数（如果到达文件末尾则可能比 output.len()/2 少）。
pub(crate) fn render_block(engine: &mut crate::engine::AudioEngine, output: &mut [f32]) -> usize {
    let block_size = output.len() / 2;
    if block_size == 0 {
        return 0;
    }

    let block_start = engine.cursor.position;
    let block_end = block_start + block_size as u64;

    // 检查是否到达文件末尾，截断最后一帧
    let duration = engine.duration_samples();
    let effective_end = if duration > 0 {
        block_end.min(duration)
    } else {
        block_end
    };
    let effective_block_size = (effective_end - block_start) as usize;
    if effective_block_size == 0 {
        return 0;
    }

    let model_has_notes = engine
        .state
        .model()
        .and_then(|m| m.notes_by_key.as_ref())
        .is_some();

    if model_has_notes {
        // ── 离线导出模式：sample-accurate 事件派发 ──
        render_block_with_dispatch(
            engine,
            output,
            block_start,
            effective_block_size,
            block_size,
        )
    } else {
        // ── 实时播放模式：纯 read_samples，事件通过 MIDI-stream 注入 ──
        render_block_realtime(engine, output, effective_block_size, block_size)
    }
}

/// 检查 output buffer 中是否有非零样本（用于调试是否真的产生了声音）。
fn has_audio_content(buf: &[f32]) -> bool {
    buf.iter().any(|&s| s.abs() > 0.001)
}

/// 实时播放模式：直接从 ChannelGroup 读取音频，不派发任何事件。
///
/// 所有事件（NoteOn/NoteOff/CC/PC/PB）通过 MIDI-stream 路径
/// （PlaybackManager → AudioCommandAdapter → process_commands → preview_*）
/// 在 `render_block` 之外注入 ChannelGroup。
fn render_block_realtime(
    engine: &mut crate::engine::AudioEngine,
    output: &mut [f32],
    effective_block_size: usize,
    block_size: usize,
) -> usize {
    let frames = effective_block_size.min(block_size);
    if frames == 0 {
        return 0;
    }

    let end = frames * 2;
    engine.channel_group.read_samples(&mut output[..end]);

    // 检查 read_samples 是否产生了非零音频
    let has_audio = has_audio_content(&output[..end]);
    if has_audio {
        tracing::debug!(
            "[AUDIO] render_block_realtime: {} frames, 有音频输出, cursor_before={}",
            frames,
            engine.cursor.position,
        );
    } else {
        tracing::debug!(
            "[AUDIO] render_block_realtime: {} frames, 静音输出, cursor_before={}",
            frames,
            engine.cursor.position,
        );
    }

    // 剩余部分填充静音
    if frames < block_size {
        for sample in &mut output[frames * 2..block_size * 2] {
            *sample = 0.0;
        }
    }

    // 应用音量限制器防止削波
    engine.limiter.limit(&mut output[..end]);

    engine.cursor.advance(frames as u64);
    frames
}

/// 离线导出模式：sample-accurate 事件派发 + 渲染。
///
/// 从 PreparedModel 中按 sample 位置精确派发 NoteOn/NoteOff/CC 事件，
/// 适用于 WAV 导出等需要精确时间控制的场景。
fn render_block_with_dispatch(
    engine: &mut crate::engine::AudioEngine,
    output: &mut [f32],
    block_start: u64,
    effective_block_size: usize,
    block_size: usize,
) -> usize {
    let sr = engine.config.sample_rate as f64;
    let effective_end = block_start + effective_block_size as u64;
    let mut written_frames = 0usize;

    while written_frames < effective_block_size {
        let current_sample = block_start + written_frames as u64;
        let remaining_frames = effective_block_size - written_frames;

        // 找到下一个事件的 sample 位置
        let next_ev = next_event_sample(engine, current_sample, effective_end);

        // 渲染到下一个事件位置（或 block 结束）
        let frames_until_event = if let Some(ev_sample) = next_ev {
            ((ev_sample - current_sample) as usize).min(remaining_frames)
        } else {
            remaining_frames
        };

        if frames_until_event > 0 {
            let start = written_frames * 2;
            let end = start + frames_until_event * 2;
            engine.channel_group.read_samples(&mut output[start..end]);
            written_frames += frames_until_event;
        }

        // 派发当前位置的事件
        let dispatch_sample = block_start + written_frames as u64;
        if dispatch_sample < effective_end {
            dispatch_events_at(engine, dispatch_sample, sr);
        }
    }

    // 剩余部分填充静音（如果 duration 截断了 block）
    if written_frames < block_size {
        for sample in &mut output[written_frames * 2..block_size * 2] {
            *sample = 0.0;
        }
    }

    // 应用音量限制器防止削波
    let rendered_samples = written_frames * 2;
    engine.limiter.limit(&mut output[..rendered_samples]);

    engine.cursor.advance(effective_block_size as u64);
    written_frames
}

/// 找到当前播放位置之后、block_end 之前的下一个事件 sample 位置。
///
/// 仅在 `notes_by_key` 为 `Some(...)` 时有效（离线导出模式）。
/// 实时播放模式下 `next_event_sample` 不会被调用。
fn next_event_sample(
    engine: &crate::engine::AudioEngine,
    current: u64,
    block_end: u64,
) -> Option<u64> {
    let model = engine.state.model()?;
    let notes_by_key = model.notes_by_key.as_ref()?;
    let sr = engine.config.sample_rate as f64;

    let mut next: Option<u64> = None;

    // CC 事件
    if engine.cc_cursor < model.cc_events.len() {
        let cc_sample = model.cc_events[engine.cc_cursor].sample;
        if cc_sample >= current && cc_sample < block_end {
            next = Some(next.map_or(cc_sample, |s| s.min(cc_sample)));
        }
    }

    // NoteOn 事件
    for key in 0..128 {
        let cursor = engine.note_cursors[key];
        let bucket = &notes_by_key[key];
        if cursor < bucket.len() {
            let note = &bucket[cursor];
            let note_sample = tick_to_sample(note.start_tick as u64, &model.tempo_segments, sr);
            if note_sample >= current && note_sample < block_end {
                next = Some(next.map_or(note_sample, |s| s.min(note_sample)));
            }
        }
    }

    // 活跃音符的 NoteOff
    for note in &engine.active_notes {
        if note.end_sample >= current && note.end_sample < block_end {
            next = Some(next.map_or(note.end_sample, |s| s.min(note.end_sample)));
        }
    }

    next
}

/// 在指定 sample 位置派发所有到期的事件。
///
/// 仅在 `notes_by_key` 为 `Some(...)` 时有效（离线导出模式）。
fn dispatch_events_at(engine: &mut crate::engine::AudioEngine, sample: u64, sr: f64) {
    let model = match engine.state.model() {
        Some(m) => m,
        None => return,
    };
    let notes_by_key = match model.notes_by_key.as_ref() {
        Some(n) => n,
        None => return,
    };

    // CC 事件
    while engine.cc_cursor < model.cc_events.len()
        && model.cc_events[engine.cc_cursor].sample <= sample
    {
        let cc = &model.cc_events[engine.cc_cursor];
        let channel = cc.channel;
        engine
            .channel_group
            .send_event(SynthEvent::Channel(channel, ChannelEvent::Audio(cc.event)));
        engine.channel_states[channel as usize].apply(&cc.event);
        engine.cc_cursor += 1;
    }

    // NoteOn 事件
    for key in 0..128u8 {
        let bucket = &notes_by_key[key as usize];
        while engine.note_cursors[key as usize] < bucket.len() {
            let cursor = engine.note_cursors[key as usize];
            let note = &bucket[cursor];
            let note_sample = tick_to_sample(note.start_tick as u64, &model.tempo_segments, sr);
            if note_sample > sample {
                break;
            }

            let end_sample = tick_to_sample(note.end_tick as u64, &model.tempo_segments, sr);
            engine.channel_group.send_event(SynthEvent::Channel(
                note.channel as u32,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                    key,
                    vel: note.velocity,
                }),
            ));
            engine.active_notes.push(ActiveNote {
                key,
                channel: note.channel,
                end_sample,
            });
            engine.note_cursors[key as usize] += 1;
        }
    }

    // NoteOff 事件（活跃音符到期）
    let mut i = 0;
    while i < engine.active_notes.len() {
        if engine.active_notes[i].end_sample <= sample {
            let note = engine.active_notes[i];
            engine.channel_group.send_event(SynthEvent::Channel(
                note.channel as u32,
                ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: note.key }),
            ));
            engine.active_notes.swap_remove(i);
        } else {
            i += 1;
        }
    }
}
