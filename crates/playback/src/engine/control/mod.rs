//! 播放引擎控制

use parking_lot::Mutex;
use std::collections::BinaryHeap;
use std::sync::Arc;

use crate::{Playback, PlaybackAccessor, PlaybackState};

use super::{EventType, MidiMessage, MidiTrackEvent, NoteEvent, ScheduledEvent};
use lumino_midi_loader::MidiDocument;

/// 其他音轨的事件读取状态。
///
/// 不再预先构建 CompactEvent 缓冲区和排序，而是直接引用 `MidiDocument` 中
/// 已按 `start_tick` 排好序的音符，通过游标 + 最小堆在播放时按需生成 NoteOn/NoteOff。
#[derive(Debug, Default)]
struct TrackEventState {
    /// 下一个尚未处理的 NoteOn 在音轨音符切片中的索引
    note_cursor: usize,
    /// 已经触发 NoteOn、等待 NoteOff 的音符，按 `end_tick` 组织为最小堆
    pending_offs: BinaryHeap<PendingNoteOff>,
}

/// 等待释放的音符，按 `end_tick` 升序排在 `BinaryHeap` 中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingNoteOff {
    end_tick: u32,
    note_index: usize,
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
    playback: Arc<Mutex<Playback>>,
    /// 当前音轨的待播放事件队列（优先队列，按tick排序）
    /// 仅有当前轨（可能有编辑），其他轨直接从 document 流式读取
    event_queue: BinaryHeap<ScheduledEvent>,
    /// 当前音轨音符（仅编辑过的音轨，小数据量）
    notes: Vec<NoteEvent>,
    /// 非音符MIDI事件（CC/PC/PB等）
    midi_events: Vec<MidiTrackEvent>,
    /// MIDI 文档（其他音轨从此流式读取）
    document: Option<Arc<MidiDocument>>,
    /// 每个非当前音轨的事件读取状态
    track_states: Vec<TrackEventState>,
    /// 当前音轨索引（此轨从 self.notes 读，不从 document 读）
    current_track: u16,
    /// 单位：tick
    last_processed_tick: f32,
    /// 循环播放
    looping: bool,
    /// 循环范围（开始tick，结束tick）
    loop_range: Option<(f32, f32)>,
    /// 控制事件（CC/PC/PB）游标
    control_event_cursor: usize,
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
        }
    }

    /// 设置 MIDI 文档引用（其他音轨从此流式读取）
    ///
    /// 不再预先把所有音符转换成 CompactEvent 并排序，而是直接保存 `MidiDocument`
    /// 的 `Arc` 引用，并为每个音轨初始化一个读取状态。播放时按需从 document
    /// 的音符切片中读取，消除播放前的大块拷贝与排序开销。
    pub fn set_document(&mut self, doc: Arc<MidiDocument>, current_track: u16) {
        let track_count = doc.track_count();
        self.track_states = (0..track_count)
            .map(|_| TrackEventState::default())
            .collect();
        self.control_event_cursor = 0;
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
        self.midi_events = events;
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
    fn rebuild_queue_from_current_track(&mut self, seek_tick: Option<f32>) {
        self.event_queue.clear();
        let mut seq: u64 = 0;

        for note in &self.notes {
            if let Some(st) = seek_tick
                && note.tick + note.length <= st
            {
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

    /// 处理播放更新
    ///
    /// 返回：需要发送的MIDI消息列表
    /// 当前音轨从 event_queue 读取，其他音轨从 document 流式读取
    pub fn update(&mut self) -> Vec<MidiMessage> {
        let mut messages = Vec::new();

        let (current_tick, is_playing) = {
            let Some(playback) = self.lock_playback() else {
                return messages;
            };
            (
                playback.current_tick(),
                playback.state() == PlaybackState::Playing,
            )
        };

        if !is_playing {
            self.last_processed_tick = current_tick;
            return messages;
        }

        self.process_current_track(current_tick, &mut messages);
        self.process_other_tracks(current_tick, &mut messages);
        self.process_midi_events(current_tick, &mut messages);
        self.last_processed_tick = current_tick;
        self.handle_loop_wrap(current_tick, &mut messages);

        messages
    }

    /// 处理当前音轨的事件队列
    fn process_current_track(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        while let Some(event) = self.event_queue.peek() {
            if event.tick > current_tick {
                break;
            }
            let event = if let Some(e) = self.event_queue.pop() {
                e
            } else {
                break;
            };
            Self::push_midi_message(event.event_type, messages);
        }
    }

    /// 处理其他音轨的事件（直接从 `MidiDocument` 音符切片流式读取）
    ///
    /// 每个非当前音轨维护一个 `note_cursor` 指向下一颗待触发 NoteOn 的音符，
    /// 并用最小堆保存已触发 NoteOn、等待 NoteOff 的音符。播放时按时间顺序
    /// 合并 NoteOn/NoteOff，避免预先把整轨事件拷贝排序。
    fn process_other_tracks(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        let Some(doc) = &self.document else { return };
        let tick_start_u = self.last_processed_tick as u32;
        let tick_end_u = current_tick as u32;

        for t in 0..self.track_states.len() {
            if t == self.current_track as usize {
                continue;
            }
            let notes = doc.track_notes(t);
            if notes.is_empty() {
                continue;
            }
            let state = &mut self.track_states[t];

            loop {
                let next_on_tick = notes
                    .get(state.note_cursor)
                    .map(|n| n.start_tick)
                    .unwrap_or(u32::MAX);
                let next_off_tick = state
                    .pending_offs
                    .peek()
                    .map(|off| off.end_tick)
                    .unwrap_or(u32::MAX);

                let next_tick = next_on_tick.min(next_off_tick);
                if next_tick > tick_end_u {
                    break;
                }

                if next_tick == next_on_tick {
                    let note = &notes[state.note_cursor];
                    if note.start_tick >= tick_start_u {
                        messages.push(MidiMessage::NoteOn {
                            channel: note.channel,
                            key: note.key,
                            velocity: note.velocity,
                        });
                    }
                    state.pending_offs.push(PendingNoteOff {
                        end_tick: note.end_tick,
                        note_index: state.note_cursor,
                    });
                    state.note_cursor += 1;
                } else {
                    let Some(off) = state.pending_offs.pop() else {
                        break;
                    };
                    if off.end_tick >= tick_start_u {
                        let note = &notes[off.note_index];
                        messages.push(MidiMessage::NoteOff {
                            channel: note.channel,
                            key: note.key,
                        });
                    }
                }
            }
        }

        // ── 控制事件（CC/PC/PB）从 document 流式读取 ──
        let ctrl_events = &doc.control_events;
        let ctrl_cursor = &mut self.control_event_cursor;
        while *ctrl_cursor < ctrl_events.len() {
            let ev = &ctrl_events[*ctrl_cursor];
            let ev_tick = ev.tick as f32;
            if ev_tick > current_tick {
                break;
            }
            if ev_tick >= self.last_processed_tick {
                Self::push_control_event(ev, messages);
            }
            *ctrl_cursor += 1;
        }
    }

    /// 处理额外 MIDI 控制事件
    fn process_midi_events(&self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        for ev in &self.midi_events {
            if ev.tick >= self.last_processed_tick && ev.tick <= current_tick {
                Self::push_midi_message_from_event(&ev.message, messages);
            }
        }
    }

    /// 处理循环回绕
    fn handle_loop_wrap(&mut self, current_tick: f32, _messages: &mut Vec<MidiMessage>) {
        if self.looping
            && let Some((_loop_start, loop_end)) = self.loop_range
            && current_tick >= loop_end
        {
            let loop_start = self.loop_range.map_or(0.0, |(s, _)| s);
            if let Some(mut playback) = self.lock_playback() {
                playback.seek(loop_start);
            }
            let seek_tick_u = loop_start as u32;
            if let Some(doc) = self.document.clone() {
                for t in 0..self.track_states.len() {
                    if t == self.current_track as usize {
                        continue;
                    }
                    self.reset_track_state_to(t, seek_tick_u, &doc);
                }
                let ctrl_events = &doc.control_events;
                self.control_event_cursor = ctrl_events.partition_point(|ev| ev.tick < seek_tick_u);
            }
            self.rebuild_queue_from_current_track(Some(loop_start));
            self.last_processed_tick = loop_start;
        }
    }

    /// 将指定音轨的读取状态重置到 `seek_tick` 位置。
    ///
    /// `note_cursor` 指向第一颗 `start_tick >= seek_tick` 的音符；`pending_offs`
    /// 收集所有在 `seek_tick` 之前已经开始、但在 `seek_tick` 仍未结束的音符，
    /// 保证循环回绕或 seek 后这些音符的 NoteOff 能被正确发出。
    fn reset_track_state_to(&mut self, track: usize, seek_tick: u32, doc: &MidiDocument) {
        let notes = doc.track_notes(track);
        let state = &mut self.track_states[track];
        state.pending_offs.clear();
        state.note_cursor = notes.partition_point(|n| n.start_tick < seek_tick);
        for (i, note) in notes.iter().enumerate().take(state.note_cursor) {
            if note.end_tick >= seek_tick {
                state.pending_offs.push(PendingNoteOff {
                    end_tick: note.end_tick,
                    note_index: i,
                });
            }
        }
    }

    #[inline]
    fn push_control_event(ev: &midly::loader::PackedControlEvent, messages: &mut Vec<MidiMessage>) {
        match ev.kind {
            0 => {
                let (controller, value) = ev.as_control_change();
                messages.push(MidiMessage::ControlChange {
                    channel: ev.channel,
                    controller,
                    value,
                });
            }
            1 => {
                let program = ev.as_program_change();
                messages.push(MidiMessage::ProgramChange {
                    channel: ev.channel,
                    program,
                });
            }
            2 => {
                let value = ev.as_pitch_bend();
                messages.push(MidiMessage::PitchBend {
                    channel: ev.channel,
                    value,
                });
            }
            _ => {}
        }
    }

    #[inline]
    fn push_midi_message(event_type: EventType, messages: &mut Vec<MidiMessage>) {
        match event_type {
            EventType::NoteOn {
                channel,
                key,
                velocity,
            } => {
                messages.push(MidiMessage::NoteOn {
                    channel,
                    key,
                    velocity,
                });
            }
            EventType::NoteOff { channel, key } => {
                messages.push(MidiMessage::NoteOff { channel, key });
            }
        }
    }

    #[inline]
    fn push_midi_message_from_event(msg: &MidiMessage, messages: &mut Vec<MidiMessage>) {
        messages.push(msg.clone());
    }

    /// 安全地跳转播放位置（内部辅助方法）
    fn seek_playback(&self, tick: f32) {
        if let Some(mut playback) = self.lock_playback() {
            playback.seek(tick);
        }
    }

    /// 播放
    ///
    /// 事件由 set_current_track_notes/set_midi_events/seek 等操作触发重建，
    /// play() 自身不再重复重建，消除冗余操作。
    pub fn play(&mut self) {
        if let Some(mut playback) = self.lock_playback() {
            playback.play();
        }
    }

    /// 暂停
    pub fn pause(&mut self) {
        if let Some(mut playback) = self.lock_playback() {
            playback.pause();
        }
    }

    /// 停止
    pub fn stop(&mut self) {
        if let Some(mut playback) = self.lock_playback() {
            playback.stop();
        }
        // 重置所有音轨读取状态
        for state in &mut self.track_states {
            state.note_cursor = 0;
            state.pending_offs.clear();
        }
        self.control_event_cursor = 0;
        self.event_queue.clear();
        self.last_processed_tick = 0.0;
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        self.seek_playback(tick);
        // 重设游标到 seek_tick 位置
        self.reset_cursors_to(tick);
        // 重建当前轨事件队列
        self.rebuild_queue_from_current_track(Some(tick));
    }

    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.lock_playback()
            .map_or(PlaybackState::Stopped, |p| p.state())
    }

    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback().map_or(0.0, |p| p.current_tick())
    }

    fn lock_playback(&self) -> Option<parking_lot::MutexGuard<'_, Playback>> {
        Some(self.playback.lock())
    }

    /// 将各音轨读取状态定位到指定 tick 位置
    fn reset_cursors_to(&mut self, tick: f32) {
        let Some(doc) = self.document.clone() else {
            return;
        };
        let seek_tick = tick as u32;
        for t in 0..self.track_states.len() {
            if t == self.current_track as usize {
                continue;
            }
            self.reset_track_state_to(t, seek_tick, &doc);
        }
        // 重置控制事件游标
        self.control_event_cursor = doc.control_events.partition_point(|e| e.tick < seek_tick);
        self.last_processed_tick = tick;
    }
}

impl PlaybackAccessor for PlaybackEngine {
    fn playback(&self) -> &Arc<Mutex<Playback>> {
        &self.playback
    }
}

#[cfg(test)]
mod control_tests;
