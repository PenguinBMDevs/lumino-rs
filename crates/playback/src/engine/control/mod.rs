//! 播放引擎控制

use parking_lot::Mutex;
use std::collections::BinaryHeap;
use std::sync::Arc;

use crate::{Playback, PlaybackAccessor, PlaybackState};

use super::{EventType, MidiMessage, MidiTrackEvent, NoteEvent, ScheduledEvent};
use lumino_midi_io::compact::{CompactEvent, EventKind};
use lumino_midi_loader::MidiDocument;

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
    /// 每个音轨的事件游标（当前处理到第几个事件）
    track_cursors: Vec<usize>,
    /// 其他音轨的 CompactEvent 缓冲区（从 `NoteEvent` 按需构造，按 tick 排序）
    track_event_buffers: Vec<Vec<CompactEvent>>,
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
            track_cursors: Vec::new(),
            track_event_buffers: Vec::new(),
            current_track: 0,
            last_processed_tick: 0.0,
            looping: false,
            loop_range: None,
            control_event_cursor: 0,
        }
    }

    /// 设置 MIDI 文档引用（其他音轨从此流式读取）
    ///
    /// 幂等：如果文档 Arc 指针相同且 current_track 未变，跳过重建。
    /// 避免 `update_playback_notes` 多次调用时重复构建 track_event_buffers。
    pub fn set_document(&mut self, doc: Arc<MidiDocument>, current_track: u16) {
        // 幂等检查：同一文档 + 同一 current_track → 跳过重建
        if let Some(ref existing) = self.document
            && Arc::ptr_eq(existing, &doc)
            && self.current_track == current_track
        {
            return;
        }

        let track_count = doc.track_count();
        self.track_cursors = vec![0usize; track_count];
        self.track_event_buffers = (0..track_count)
            .map(|t| {
                let notes = doc.track_notes(t);
                let track_id = t as u16;
                let mut events: Vec<CompactEvent> = Vec::with_capacity(notes.len() * 2);
                for note in notes {
                    let [on, off] = note.to_compact_events(track_id);
                    events.push(on);
                    events.push(off);
                }
                events.sort_unstable_by_key(|e| e.delta_tick());
                events
            })
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

    /// 处理其他音轨的事件（从预生成的 CompactEvent 缓冲区流式读取）
    fn process_other_tracks(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        let Some(doc) = &self.document else { return };
        let tick_start_u = self.last_processed_tick as u32;
        let tick_end_u = current_tick as u32;

        for t in 0..self.track_cursors.len() {
            if t == self.current_track as usize {
                continue;
            }
            let events = match self.track_event_buffers.get(t) {
                Some(e) if !e.is_empty() => e,
                _ => continue,
            };
            let cursor = &mut self.track_cursors[t];

            while *cursor < events.len() {
                let ev = &events[*cursor];
                let ev_tick = ev.delta_tick();
                if ev_tick > tick_end_u {
                    break;
                }
                if ev_tick >= tick_start_u {
                    Self::push_event(ev, messages);
                }
                *cursor += 1;
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
            if let Some(doc) = &self.document {
                for t in 0..self.track_cursors.len() {
                    if t == self.current_track as usize {
                        continue;
                    }
                    let events = match self.track_event_buffers.get(t) {
                        Some(e) if !e.is_empty() => e,
                        _ => continue,
                    };
                    let cursor = &mut self.track_cursors[t];
                    *cursor = events.partition_point(|ev| ev.delta_tick() < seek_tick_u);
                }
                let ctrl_events = &doc.control_events;
                self.control_event_cursor = ctrl_events.partition_point(|ev| ev.tick < seek_tick_u);
            }
            self.rebuild_queue_from_current_track(Some(loop_start));
            self.last_processed_tick = loop_start;
        }
    }

    #[inline]
    fn push_event(ev: &CompactEvent, messages: &mut Vec<MidiMessage>) {
        match ev.kind() {
            EventKind::NoteOn => {
                messages.push(MidiMessage::NoteOn {
                    channel: ev.channel(),
                    key: ev.param1() as u8,
                    velocity: ev.param2() as u8,
                });
            }
            EventKind::NoteOff => {
                messages.push(MidiMessage::NoteOff {
                    channel: ev.channel(),
                    key: ev.param1() as u8,
                });
            }
            _ => {}
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
        // 重置游标
        self.track_cursors.fill(0);
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

    /// 将 event buffer 游标定位到指定 tick 之后第一个事件
    fn reset_cursors_to(&mut self, tick: f32) {
        let Some(doc) = &self.document else { return };
        for t in 0..self.track_cursors.len() {
            if t == self.current_track as usize {
                continue;
            }
            let events = match self.track_event_buffers.get(t) {
                Some(e) if !e.is_empty() => e,
                _ => continue,
            };
            // 二分查找第一个 >= tick 的事件
            let pos = events.partition_point(|e| e.delta_tick() < tick as u32);
            self.track_cursors[t] = pos;
        }
        // 重置控制事件游标
        self.control_event_cursor = doc.control_events.partition_point(|e| e.tick < tick as u32);
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
