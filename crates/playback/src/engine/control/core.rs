//! PlaybackEngine 结构体定义与基础访问器

use parking_lot::Mutex;
use std::collections::BinaryHeap;
use std::sync::Arc;

use crate::{
    EventType, MidiMessage, MidiTrackEvent, NoteEvent, Playback, PlaybackAccessor, ScheduledEvent,
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
    /// 当前音轨音符（仅编辑过的音轨，小数据量）
    pub(crate) notes: Vec<NoteEvent>,
    /// 非音符MIDI事件（CC/PC/PB等）
    pub(crate) midi_events: Vec<MidiTrackEvent>,
    /// MIDI 文档（其他音轨从此流式读取）
    pub(crate) document: Option<Arc<MidiDocument>>,
    /// 每个非当前音轨的事件读取状态
    pub(crate) track_states: Vec<TrackEventState>,
    /// 当前音轨索引（此轨从 self.notes 读，不从 document 读）
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
    /// 复用的消息缓冲区，避免 update() 每帧分配新 Vec
    pub(crate) reused_messages: Vec<MidiMessage>,
}

impl PlaybackEngine {
    /// 创建新的播放引擎
    pub fn new(playback: Arc<Mutex<Playback>>) -> Self {
        Self {
            playback,
            event_queue: BinaryHeap::new(),
            notes: Vec::new(),
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
    }

    /// 设置当前音轨音符列表（仅编辑过的音轨，小数据量）
    /// 当前轨不从 document 读取，而从此队列播放
    pub fn set_current_track_notes(&mut self, notes: Vec<NoteEvent>) {
        self.notes = notes;
        // 重排当前轨的 event_queue
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

    /// 重建当前音轨的事件队列
    pub(crate) fn rebuild_queue_from_current_track(&mut self, seek_tick: Option<f32>) {
        self.event_queue.clear();
        // 每颗音符最多产生 NoteOn + NoteOff 两个事件，预分配避免反复扩容。
        self.event_queue.reserve(self.notes.len() * 2);
        let mut seq: u64 = 0;

        for note in &self.notes {
            if let Some(st) = seek_tick
                && note.tick + note.length <= st
            {
                continue;
            }
            // 力度过滤：低于等于阈值的音符不加入播放队列。
            // 这是用户配置的语义过滤，不是性能节流；默认阈值 1 只过滤 velocity=0。
            if note.velocity <= self.velocity_filter_threshold {
                continue;
            }
            self.event_queue.push(ScheduledEvent {
                tick: note.tick,
                event_type: EventType::NoteOn {
                    channel: note.channel,
                    key: note.key,
                    velocity: note.velocity,
                },
                seq,
            });
            seq += 1;
            self.event_queue.push(ScheduledEvent {
                tick: note.tick + note.length,
                event_type: EventType::NoteOff {
                    channel: note.channel,
                    key: note.key,
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
