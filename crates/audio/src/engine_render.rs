//! Sample-accurate 渲染逻辑 — 在渲染每块音频时精确派发事件。
//!
//! 核心思路（借鉴 yinhe）：
//! 1. 找到当前 block 内下一个事件的 sample 位置
//! 2. 先渲染到该位置
//! 3. 在精确位置派发事件（CC/NoteOn/NoteOff）
//! 4. 重复直到 block 结束

use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent};
use xsynth_core::channel_group::SynthEvent;
use xsynth_core::AudioPipe;

use crate::audio_model::{tick_to_sample, ActiveNote};

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
/// `output` 长度必须是偶数（每帧 2 个 sample）。
pub(crate) fn render_block(
    engine: &mut crate::engine::AudioEngine,
    output: &mut [f32],
) {
    let block_size = output.len() / 2;
    if block_size == 0 {
        return;
    }

    let block_start = engine.cursor.position;
    let block_end = block_start + block_size as u64;

    let sr = engine.config.sample_rate as f64;

    let mut written_frames = 0usize;

    while written_frames < block_size {
        let current_sample = block_start + written_frames as u64;
        let remaining_frames = block_size - written_frames;

        // 找到下一个事件的 sample 位置
        let next_event_sample = next_event_sample(engine, current_sample, block_end);

        // 渲染到下一个事件位置（或 block 结束）
        let frames_until_event = if let Some(ev_sample) = next_event_sample {
            (ev_sample - current_sample) as usize
        } else {
            remaining_frames
        };

        let render_frames = frames_until_event.min(remaining_frames);
        if render_frames > 0 {
            let start = written_frames * 2;
            let end = start + render_frames * 2;
            engine.channel_group.read_samples(&mut output[start..end]);
            written_frames += render_frames;
        }

        // 派发当前位置的事件
        let dispatch_sample = block_start + written_frames as u64;
        if dispatch_sample < block_end {
            dispatch_events_at(engine, dispatch_sample, sr);
        }
    }

    engine.cursor.advance(block_size as u64);
}

/// 找到当前播放位置之后、block_end 之前的下一个事件 sample 位置。
fn next_event_sample(
    engine: &crate::engine::AudioEngine,
    current: u64,
    block_end: u64,
) -> Option<u64> {
    let model = engine.state.model()?;
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
        let bucket = &model.notes_by_key[key];
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
fn dispatch_events_at(
    engine: &mut crate::engine::AudioEngine,
    sample: u64,
    sr: f64,
) {
    let model = match engine.state.model() {
        Some(m) => m,
        None => return,
    };

    // CC 事件
    while engine.cc_cursor < model.cc_events.len()
        && model.cc_events[engine.cc_cursor].sample <= sample
    {
        let cc = &model.cc_events[engine.cc_cursor];
        let channel = cc.channel;
        engine.channel_group.send_event(SynthEvent::Channel(
            channel,
            ChannelEvent::Audio(cc.event),
        ));
        engine.channel_states[channel as usize].apply(&cc.event);
        engine.cc_cursor += 1;
    }

    // NoteOn 事件
    for key in 0..128u8 {
        let cursor = engine.note_cursors[key as usize];
        let bucket = &model.notes_by_key[key as usize];
        while cursor < bucket.len() {
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
