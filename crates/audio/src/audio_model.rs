//! 音频引擎数据模型 — 适配 lumino 的 MidiDocument 结构。
//!
//! 将 lumino 的 `Vec<Vec<NoteEvent>>`（按轨分桶）转换为按 key 分桶的
//! `Box<[Vec<NoteBucketEntry>; 128]>`，以便 sample-accurate 渲染时
//! 只需遍历 128 个 key 而非全部音轨。

use std::sync::Arc;

use lumino_midi_loader::MidiDocument;
use midly::loader::PackedControlEvent;
use xsynth_core::channel::ChannelAudioEvent;

/// 按 sample 排序的控制事件（CC / PC / PB）。
pub(crate) struct SortedCC {
    pub(crate) sample: u64,
    pub(crate) channel: u32,
    pub(crate) event: ChannelAudioEvent,
}

/// 正在发声的音符（用于跟踪 NoteOff 时机）。
#[derive(Clone, Copy)]
pub(crate) struct ActiveNote {
    pub(crate) key: u8,
    pub(crate) channel: u8,
    pub(crate) end_sample: u64,
}

/// 按 key 分桶的音符条目。
#[derive(Clone, Copy)]
pub(crate) struct NoteBucketEntry {
    pub(crate) start_tick: u32,
    pub(crate) end_tick: u32,
    pub(crate) velocity: u8,
    pub(crate) channel: u8,
    pub(crate) track: u16,
}

/// 速度段（用于 tick→sample 转换）。
#[derive(Clone, Copy)]
pub(crate) struct TempoSegment {
    pub(crate) start_tick: u32,
    pub(crate) start_time: f64,
    pub(crate) micros_per_quarter: f64,
}

/// 预计算模型数据，在 worker 线程构建后原子应用到音频引擎。
///
/// `notes_by_key` 在实时播放路径中为 `None`（零拷贝，不复制音符数据），
/// 仅在离线导出路径中为 `Some(...)`（构建按 key 分桶的索引）。
pub(crate) struct PreparedModel {
    /// 按 key 分桶的音符索引。`None` 表示实时播放模式，跳过事件派发。
    pub notes_by_key: Option<Box<[Vec<NoteBucketEntry>; 128]>>,
    pub cc_events: Vec<SortedCC>,
    pub tempo_segments: Vec<TempoSegment>,
    pub duration_samples: u64,
    pub division: u16,
}

/// 为**实时播放**构建轻量模型 — 只提取 tempo + CC 数据，**不拷贝音符**。
///
/// 实时播放的事件通过 MIDI-stream（PlaybackManager → AudioCommandAdapter）直接注入
/// ChannelGroup，不需要 PreparedModel 的按 key 分桶索引。因此 notes_by_key = None，
/// 避免 160M 音符的全量拷贝和排序。
pub(crate) fn prepare_playback_model(doc: &MidiDocument, sample_rate: u32) -> PreparedModel {
    let sr = sample_rate as f64;
    let tempo_segments = build_tempo_segments(&doc.tempo_changes, doc.total_ticks);
    let duration_samples = tick_to_sample(doc.total_ticks as u64, &tempo_segments, sr);
    let cc_events = build_cc_events(&doc.control_events, &tempo_segments, sr);
    PreparedModel {
        notes_by_key: None,
        cc_events,
        tempo_segments,
        duration_samples,
        division: 480,
    }
}

/// 为**离线导出**构建完整预计算模型 — 包含按 key 分桶的音符索引。
///
/// 与 `prepare_playback_model` 的区别：额外构建 `notes_by_key`（160M 音符的全量拷贝），
/// 用于 `render_block` 的 sample-accurate 事件派发。仅在导出 WAV 时调用。
pub(crate) fn prepare_export_model(doc: &Arc<MidiDocument>, sample_rate: u32) -> PreparedModel {
    let sr = sample_rate as f64;
    let tempo_segments = build_tempo_segments(&doc.tempo_changes, doc.total_ticks);
    let duration_samples = tick_to_sample(doc.total_ticks as u64, &tempo_segments, sr);

    // ── 按 key 分桶（仅导出路径需要） ──
    let mut notes_by_key: Box<[Vec<NoteBucketEntry>; 128]> =
        Box::new(std::array::from_fn(|_| Vec::new()));
    for (track_idx, track_notes) in doc.notes.iter().enumerate() {
        for note in track_notes {
            if note.velocity <= 1 {
                continue;
            }
            let key = note.key as usize;
            if key < 128 {
                notes_by_key[key].push(NoteBucketEntry {
                    start_tick: note.start_tick,
                    end_tick: note.end_tick,
                    velocity: note.velocity,
                    channel: note.channel,
                    track: track_idx as u16,
                });
            }
        }
    }
    for bucket in notes_by_key.iter_mut() {
        bucket.sort_unstable_by_key(|n| n.start_tick);
    }

    let cc_events = build_cc_events(&doc.control_events, &tempo_segments, sr);

    PreparedModel {
        notes_by_key: Some(notes_by_key),
        cc_events,
        tempo_segments,
        duration_samples,
        division: 480,
    }
}

/// 从 MIDI 控制事件构建排序去重的 SortedCC 列表。
///
/// 被 `prepare_playback_model` 和 `prepare_export_model` 共享。
fn build_cc_events(
    control_events: &[PackedControlEvent],
    tempo_segments: &[TempoSegment],
    sr: f64,
) -> Vec<SortedCC> {
    let mut cc_events = Vec::with_capacity(control_events.len());
    for ev in control_events {
        let sample = tick_to_sample(ev.tick as u64, tempo_segments, sr);
        let channel = ev.channel as u32;
        match ev.kind {
            0 => {
                let (controller, value) = ev.as_control_change();
                cc_events.push(SortedCC {
                    sample,
                    channel,
                    event: ChannelAudioEvent::Control(xsynth_core::channel::ControlEvent::Raw(
                        controller, value,
                    )),
                });
            }
            1 => {
                let program = ev.as_program_change();
                cc_events.push(SortedCC {
                    sample,
                    channel,
                    event: ChannelAudioEvent::ProgramChange(program),
                });
            }
            2 => {
                let value = ev.as_pitch_bend();
                cc_events.push(SortedCC {
                    sample,
                    channel,
                    event: ChannelAudioEvent::Control(
                        xsynth_core::channel::ControlEvent::PitchBendValue(value),
                    ),
                });
            }
            _ => {}
        }
    }
    cc_events.sort_unstable_by_key(|e| e.sample);
    cc_events.dedup_by(|a, b| a.channel == b.channel && a.event == b.event);
    cc_events
}

/// 从 lumino 的 tempo_changes (tick, bpm) 构建速度段。
fn build_tempo_segments(tempo_changes: &[(u32, f32)], total_ticks: u32) -> Vec<TempoSegment> {
    if tempo_changes.is_empty() {
        return vec![TempoSegment {
            start_tick: 0,
            start_time: 0.0,
            micros_per_quarter: 500_000.0, // 120 BPM
        }];
    }

    let mut sorted: Vec<(u32, f32)> = tempo_changes.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let mut segments = Vec::with_capacity(sorted.len());
    let mut current_time = 0.0f64;
    let division = 480.0f64; // lumino 默认 PPQ

    for (i, &(tick, bpm)) in sorted.iter().enumerate() {
        let micros_per_quarter = 60_000_000.0 / bpm as f64;
        if i > 0 {
            let prev = &sorted[i - 1];
            let delta_ticks = tick - prev.0;
            let _delta_secs = delta_ticks as f64 / division * micros_per_quarter / 1_000_000.0;
            // 不对，应该用前一个段的 micros_per_quarter
            let prev_micros = 60_000_000.0 / prev.1 as f64;
            let delta_secs = delta_ticks as f64 / division * prev_micros / 1_000_000.0;
            current_time += delta_secs;
        }
        segments.push(TempoSegment {
            start_tick: tick,
            start_time: current_time,
            micros_per_quarter,
        });
    }

    let _ = total_ticks;
    segments
}

/// tick → sample 转换（基于 tempo segments）。
pub(crate) fn tick_to_sample(tick: u64, segments: &[TempoSegment], sr: f64) -> u64 {
    if segments.is_empty() {
        return 0;
    }
    let tick_u32 = tick as u32;
    let idx = match segments.binary_search_by_key(&tick_u32, |s| s.start_tick) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let seg = &segments[idx];
    let delta_ticks = tick - seg.start_tick as u64;
    let division = 480.0f64;
    let secs =
        seg.start_time + delta_ticks as f64 / division * seg.micros_per_quarter / 1_000_000.0;
    (secs * sr) as u64
}

/// sample → tick 转换（用于从播放位置反查 tick）。
pub(crate) fn sample_to_tick(sample: u64, segments: &[TempoSegment], sr: f64) -> f64 {
    if segments.is_empty() {
        return 0.0;
    }
    let target_secs = sample as f64 / sr;
    let idx = match segments.binary_search_by(|s| {
        s.start_time
            .partial_cmp(&target_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let seg = &segments[idx];
    let delta_secs = target_secs - seg.start_time;
    let division = 480.0f64;
    let delta_ticks = delta_secs * 1_000_000.0 / seg.micros_per_quarter * division;
    seg.start_tick as f64 + delta_ticks
}
