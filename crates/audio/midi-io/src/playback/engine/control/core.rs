//! PlaybackEngine 结构体定义与基础访问器

use parking_lot::Mutex;
use std::collections::BinaryHeap;
use std::sync::Arc;

use crate::playback::{
    EventType, MidiMessage, MidiTrackEvent, Playback, PlaybackAccessor, ScheduledEvent,
};
use lumino_midi_loader::MidiDocument;

/// 其他音轨的事件读取状态。
///
/// 不再预先构建 CompactEvent 缓冲区和排序，而是直接引用 `MidiDocument` 中
/// 已按 `start_tick` 排好序的音符，通过游标 + 最小堆在播放时按需生成 NoteOn/NoteOff。
#[derive(Debug, Default)]
pub(crate) struct TrackEventState {
    /// 下一个尚未处理的 NoteOn 在音轨音符切片中的索引
    pub(crate) note_cursor: usize,
    /// 已经触发 NoteOn、等待 NoteOff 的音符，按 `end_tick` 组织为最小堆
    pub(crate) pending_offs: BinaryHeap<PendingNoteOff>,
}

/// 等待释放的音符，按 `end_tick` 升序排在 `BinaryHeap` 中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingNoteOff {
    pub(crate) end_tick: u32,
    pub(crate) note_index: usize,
}

impl PartialOrd for PendingNoteOff {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PendingNoteOff {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // `BinaryHeap` 是最大堆，反转比较以得到最小堆
        other
            .end_tick
            .cmp(&self.end_tick)
            .then_with(|| other.note_index.cmp(&self.note_index))
    }
}

/// 播放引擎
///
/// 当前音轨从内存队列读取（支持实时编辑），其他音轨从 document 流式读取（零拷贝）。
pub struct PlaybackEngine {
    /// 播放器
    pub(crate) playback: Arc<Mutex<Playback>>,
    /// 当前音轨的待播放事件队列（优先队列，按tick排序）
    /// 仅有当前轨（可能有编辑），其他轨直接从 document 流式读取
    pub(crate) event_queue: BinaryHeap<ScheduledEvent>,
    /// 非音符MIDI事件（CC/PC/PB等）
    pub(crate) midi_events: Vec<MidiTrackEvent>,
    /// MIDI 文档（当前轨与其他轨统一从此流式读取）
    pub(crate) document: Option<Arc<MidiDocument>>,
    /// 每个非当前音轨的事件读取状态
    pub(crate) track_states: Vec<TrackEventState>,
    /// 当前音轨索引（当前轨从 document 流式读取，与 self.notes 解耦）
    pub(crate) current_track: u16,
    /// 单位：tick
    pub(crate) last_processed_tick: f32,
    /// 循环播放
    pub(crate) looping: bool,
    /// 循环范围（开始tick，结束tick）
    pub(crate) loop_range: Option<(f32, f32)>,
    /// 控制事件（CC/PC/PB）游标
    pub(crate) control_event_cursor: usize,
    /// 额外 MIDI 事件游标（避免每次 update 线性扫描全部事件）
    pub(crate) midi_event_cursor: usize,
    /// 力度过滤阈值：velocity 小于等于此值的音符不发声（0 表示不过滤）。
    pub(crate) velocity_filter_threshold: u8,
    /// 音轨静音状态（按 document 音轨索引，true = 静音）
    pub(crate) track_muted: Vec<bool>,
    /// 音轨独奏状态（按 document 音轨索引，true = 独奏）
    pub(crate) track_soloed: Vec<bool>,
    /// 复用的消息缓冲区，避免 update() 每帧分配新 Vec
    pub(crate) reused_messages: Vec<MidiMessage>,
}

impl PlaybackEngine {
    /// 创建新的播放引擎
    pub fn new(playback: Arc<Mutex<Playback>>) -> Self {
        Self {
            playback,
            event_queue: BinaryHeap::new(),
            midi_events: Vec::new(),
            document: None,
            track_states: Vec::new(),
            current_track: 0,
            last_processed_tick: 0.0,
            looping: false,
            loop_range: None,
            control_event_cursor: 0,
            midi_event_cursor: 0,
            velocity_filter_threshold: 1,
            track_muted: Vec::new(),
            track_soloed: Vec::new(),
            reused_messages: Vec::with_capacity(64),
        }
    }

    /// 是否正在播放（用于播放线程决定高精度循环还是空闲阻塞）
    pub fn is_playing(&self) -> bool {
        self.playback.lock().is_playing()
    }

    /// 设置 MIDI 文档引用（其他音轨从此流式读取）
    ///
    /// 不再预先把所有音符转换成 CompactEvent 并排序，而是直接保存 `MidiDocument`
    /// 的 `Arc` 引用，并为每个音轨初始化一个读取状态。播放时按需从 document
    /// 的音符切片中读取，消除播放前的大块拷贝与排序开销。
    ///
    /// 2026-08-06 音频修复：编辑后更新文档快照时保留播放游标，避免播放中
    /// 其他音轨跳回开头。仅首次设置或音轨数变化时才全量初始化 track_states。
    /// ChunkedList 内部 Arc 块级共享使 clone 退化为 O(块数) 指针拷贝，
    /// 每次编辑后发送快照的开销可忽略。
    pub fn set_document(&mut self, doc: Arc<MidiDocument>, current_track: u16) {
        let track_count = doc.track_count();
        let needs_full_reset = self.document.is_none() || self.track_states.len() != track_count;

        if needs_full_reset {
            // 首次设置或音轨数变化：复用/扩展 Vec，避免每次重新分配。
            if self.track_states.len() < track_count {
                self.track_states
                    .resize_with(track_count, TrackEventState::default);
            } else {
                self.track_states.truncate(track_count);
            }
            self.control_event_cursor = 0;
            self.midi_event_cursor = 0;
        } else {
            // 编辑后更新快照：保留播放游标，仅钳制越界 cursor
            // process_other_tracks 使用 get() 安全访问，不会 panic；
            // 下次 seek/play 会通过 reset_cursors_to 精确重定位。
            for track_idx in 0..track_count {
                let notes_len = doc.track_notes(track_idx).len();
                let state = &mut self.track_states[track_idx];
                if state.note_cursor > notes_len {
                    state.note_cursor = notes_len;
                }
            }
        }

        self.current_track = current_track;
        self.document = Some(doc);
        // 当前轨队列统一从 document 重建（UI 侧不再传 Vec<NoteEvent> 中转，
        // 消除编辑后全量克隆当前轨音符的 CPU 内存阻塞）
        self.rebuild_queue_from_current_track(None);
    }

    /// 从当前 MIDI 文档重建当前音轨播放队列（与其他轨一致从 document 流式读取）
    pub fn rebuild_current_track_queue(&mut self) {
        self.rebuild_queue_from_current_track(None);
    }

    /// 设置非音符MIDI事件列表
    pub fn set_midi_events(&mut self, events: Vec<MidiTrackEvent>) {
        self.midi_event_cursor = 0;
        self.midi_events = events;
    }

    /// 设置力度过滤阈值。仅当阈值变化时才重建当前轨队列，避免不必要的重排。
    pub fn set_velocity_filter_threshold(&mut self, threshold: u8) {
        if self.velocity_filter_threshold != threshold {
            self.velocity_filter_threshold = threshold;
            self.rebuild_queue_from_current_track(None);
        }
    }

    /// 设置音轨静音/独奏状态（按 document 音轨索引）
    ///
    /// 在播放线程中由 `Command::SetTrackPlayStates` 调用。状态随后被
    /// [`Self::track_should_play`] 用于过滤当前轨与其他轨的事件。
    pub(crate) fn set_track_play_states(&mut self, muted: Vec<bool>, soloed: Vec<bool>) {
        self.track_muted = muted;
        self.track_soloed = soloed;
    }

    /// 音轨是否应当发声（综合静音/独奏规则）
    ///
    /// 标准 DAW 语义：
    /// - 存在任一独奏音轨（`has_solo`）时，仅独奏音轨发声；
    /// - 否则所有未静音（`!muted`）的音轨发声。
    ///
    /// 索引越界（如音轨数变化后状态未同步）按"未静音、未独奏"处理，
    /// 即默认发声，避免误杀声音。
    pub(crate) fn track_should_play(&self, track_idx: usize) -> bool {
        let has_solo = self.track_soloed.iter().any(|&s| s);
        if has_solo {
            self.track_soloed.get(track_idx).copied().unwrap_or(false)
        } else {
            !self.track_muted.get(track_idx).copied().unwrap_or(false)
        }
    }

    /// 设置循环
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// 设置循环范围
    pub fn set_loop_range(&mut self, start: f32, end: f32) {
        self.loop_range = Some((start, end));
    }

    /// 清除循环范围
    pub fn clear_loop_range(&mut self) {
        self.loop_range = None;
    }

    /// 重建当前音轨的事件队列（从 document 流式读取，无 Vec<NoteEvent> 中转）
    pub(crate) fn rebuild_queue_from_current_track(&mut self, seek_tick: Option<f32>) {
        self.event_queue.clear();
        let Some(doc) = self.document.as_ref() else {
            return;
        };
        // 当前轨被静音或未被独奏 → 整轨不出声，直接清空队列。
        if !self.track_should_play(self.current_track as usize) {
            return;
        }
        let notes = doc.track_notes(self.current_track as usize);
        // 每颗音符最多产生 NoteOn + NoteOff 两个事件，预分配避免反复扩容。
        self.event_queue.reserve(notes.len() * 2);
        let mut seq: u64 = 0;

        for ne in notes.iter() {
            let tick = ne.start_tick as f32;
            let length = (ne.end_tick - ne.start_tick) as f32;
            if let Some(st) = seek_tick
                && tick + length <= st
            {
                continue;
            }
            // 力度过滤：低于等于阈值的音符不加入播放队列。
            // 这是用户配置的语义过滤，不是性能节流；默认阈值 1 只过滤 velocity=0。
            if ne.velocity <= self.velocity_filter_threshold {
                continue;
            }
            self.event_queue.push(ScheduledEvent {
                tick,
                event_type: EventType::NoteOn {
                    channel: ne.channel,
                    key: ne.key,
                    velocity: ne.velocity,
                },
                seq,
            });
            seq += 1;
            self.event_queue.push(ScheduledEvent {
                tick: tick + length,
                event_type: EventType::NoteOff {
                    channel: ne.channel,
                    key: ne.key,
                },
                seq,
            });
            seq += 1;
        }
    }

    /// 安全地跳转播放位置（内部辅助方法）
    pub(crate) fn seek_playback(&self, tick: f32) {
        if let Some(mut playback) = self.lock_playback() {
            playback.seek(tick);
        }
    }

    pub(crate) fn lock_playback(&self) -> Option<parking_lot::MutexGuard<'_, Playback>> {
        Some(self.playback.lock())
    }
}

impl PlaybackAccessor for PlaybackEngine {
    fn playback(&self) -> &Arc<Mutex<Playback>> {
        &self.playback
    }
}
