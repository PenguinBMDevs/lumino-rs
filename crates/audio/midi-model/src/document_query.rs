//! MidiDocument 只读查询（从 `document.rs` 拆分）
//!
//! 事件/音符/音轨信息的访问器，以及 max_end_tick 缓存的惰性重算。

use crate::note_event::NoteEvent;
use crate::track::TrackManager;

use super::{MidiDocument, TICK_SEARCH_BUFFER, TrackNoteView};

impl MidiDocument {
    /// 获取总 tick 数
    #[inline]
    pub fn total_ticks(&self) -> u32 {
        self.total_ticks
    }

    /// 获取音轨可见性管理（只读视图）。
    #[inline]
    pub fn tracks(&self) -> &TrackManager {
        &self.tracks
    }

    /// 获取 MIDI 文件头 division（PPQ）。
    #[inline]
    pub fn division(&self) -> u16 {
        self.division
    }

    /// 获取所有 CompactEvent（按需从 NoteEvent 实时构造）。
    pub fn all_events(&self) -> Vec<crate::compact::CompactEvent> {
        let mut events = Vec::with_capacity(self.total_note_count() * 2);
        for (track_id, track_notes) in self.notes.iter().enumerate() {
            let track_id_u16 = track_id as u16;
            for note in track_notes {
                let [on, off] = note.to_compact_events(track_id_u16);
                events.push(on);
                events.push(off);
            }
        }
        events
    }

    /// 获取指定音轨的所有 CompactEvent（按需从 NoteEvent 构造）。
    pub fn get_track_events(&self, track_id: u16) -> Vec<crate::compact::CompactEvent> {
        let tid = track_id as usize;
        match self.notes.get(tid) {
            Some(track_notes) => {
                let mut events = Vec::with_capacity(track_notes.len() * 2);
                for note in track_notes {
                    let [on, off] = note.to_compact_events(track_id);
                    events.push(on);
                    events.push(off);
                }
                events
            }
            None => Vec::new(),
        }
    }

    /// 获取指定 tick 范围内的所有 CompactEvent（按需从 NoteEvent 构造）。
    pub fn get_events_in_range(
        &self,
        from_tick: u32,
        to_tick: u32,
        max_events: usize,
    ) -> Vec<crate::compact::CompactEvent> {
        let limit = if max_events == 0 {
            usize::MAX
        } else {
            max_events
        };
        let mut events = Vec::new();
        for (track_id, track_notes) in self.notes.iter().enumerate() {
            let track_id_u16 = track_id as u16;
            for note in track_notes {
                let [on, off] = note.to_compact_events(track_id_u16);
                let on_tick = on.delta_tick();
                let off_tick = off.delta_tick();
                if on_tick >= from_tick && on_tick < to_tick {
                    events.push(on);
                }
                if off_tick >= from_tick && off_tick < to_tick {
                    events.push(off);
                }
                if events.len() >= limit {
                    return events;
                }
            }
        }
        events
    }

    /// 检查指定音轨在指定范围内是否有事件。
    pub fn has_track_events_in_range(&self, track_id: u16, from_tick: u32, to_tick: u32) -> bool {
        let tid = track_id as usize;
        let Some(track_notes) = self.notes.get(tid) else {
            return false;
        };
        track_notes.iter().any(|note| {
            (note.start_tick >= from_tick && note.start_tick < to_tick)
                || (note.end_tick > from_tick && note.end_tick < to_tick)
        })
    }

    /// 轻量获取指定音轨的音符数。
    pub fn track_note_count(&self, track_id: u16) -> u64 {
        let tid = track_id as usize;
        self.notes
            .get(tid)
            .map(|notes| notes.len() as u64)
            .unwrap_or(0)
    }

    /// 获取总音符数。
    fn total_note_count(&self) -> usize {
        self.notes.iter().map(|v| v.len()).sum()
    }

    /// 获取指定音轨在指定 tick 范围内的音符。
    pub fn get_track_notes_in_range(
        &self,
        track_id: u16,
        tick_start: f32,
        tick_end: f32,
    ) -> Vec<TrackNoteView> {
        let tid = track_id as usize;
        let notes = match self.notes.get(tid) {
            Some(n) if !n.is_empty() => n,
            _ => return Vec::new(),
        };

        let tick_start_u = tick_start as u32;
        let tick_end_u = tick_end as u32;

        // 分块二分：从 tick_start - TICK_SEARCH_BUFFER 开始扫描（跨视口长音符）
        let search_start_tick = tick_start_u.saturating_sub(TICK_SEARCH_BUFFER);
        let mut result = Vec::with_capacity(256);
        for n in notes.range(search_start_tick, tick_end_u + 1) {
            if n.end_tick() >= tick_start_u && n.start_tick <= tick_end_u {
                result.push(TrackNoteView::from_event(n));
            }
        }

        result
    }

    /// 获取指定音轨的所有音符。
    pub fn get_track_notes(&self, track_id: u16) -> Vec<TrackNoteView> {
        let tid = track_id as usize;
        match self.notes.get(tid) {
            Some(notes) if !notes.is_empty() => {
                let mut result = Vec::with_capacity(notes.len());
                for n in notes {
                    result.push(TrackNoteView::from_event(n));
                }
                result
            }
            _ => Vec::new(),
        }
    }

    /// 获取指定音轨的代表性 MIDI 通道。
    ///
    /// 通道确定策略（参考 yinhe MIDI 导入逻辑）：
    /// 1. 如果有音符，取**第一个音符**的通道；
    /// 2. 如果没有音符但有控制事件（CC/PC/PB），取第一个控制事件的通道；
    /// 3. 如果既无音符也无控制事件，返回 0（默认）。
    ///
    /// 取首事件通道而非统计最频通道，原因：
    /// - 一个音轨中绝大多数音符在单一通道，但可能混入少量其他通道的事件
    ///   （如控制器事件），统计最频会导致偶然偏差；
    /// - 首个事件的通道代表 DAW/MIDI 编排时为该轨分配的"意图通道"。
    pub fn track_channel(&self, track_id: u16) -> u8 {
        let tid = track_id as usize;
        // 策略 1：取第一个音符的通道
        if let Some(first) = self.notes.get(tid).and_then(|n| n.first()) {
            return first.channel & 0x0F;
        }
        // 策略 2：没有音符时，取第一个控制事件的通道
        for ev in &self.control_events {
            if ev.track == track_id {
                return ev.channel & 0x0F;
            }
        }
        // 策略 3：都没有，返回 0
        0
    }

    /// 获取指定音轨的 MIDI 端口（从 MidiPort meta FF 21 提取）。
    /// 若音轨无 MidiPort 事件，返回 0（默认端口）。
    #[inline]
    pub fn track_port(&self, track_id: u16) -> u8 {
        self.track_ports
            .get(track_id as usize)
            .copied()
            .unwrap_or(0)
    }

    /// 获取所有音轨（排除指定音轨）在指定 tick 范围内的音符。
    pub fn get_all_notes_in_range_except(
        &self,
        exclude_track: usize,
        tick_start: f32,
        tick_end: f32,
    ) -> Vec<TrackNoteView> {
        let tick_start_u = tick_start as u32;
        let tick_end_u = tick_end as u32;

        let mut all_notes = Vec::with_capacity(1024);

        for track_idx in 0..self.track_count() {
            if track_idx == exclude_track {
                continue;
            }

            let notes = match self.notes.get(track_idx) {
                Some(n) => n,
                None => continue,
            };

            if notes.is_empty() {
                continue;
            }

            let search_start_tick = tick_start_u.saturating_sub(TICK_SEARCH_BUFFER);
            for n in notes.range(search_start_tick, tick_end_u + 1) {
                if n.end_tick() >= tick_start_u && n.start_tick <= tick_end_u {
                    all_notes.push(TrackNoteView::from_event(n));
                }
            }
        }

        all_notes.sort_by(|a, b| a.start_tick.total_cmp(&b.start_tick));
        all_notes
    }

    /// 获取音轨数量
    #[inline]
    pub fn track_count(&self) -> usize {
        self.track_count as usize
    }

    /// 获取指定音轨的名称
    #[inline]
    pub fn track_name(&self, track_id: usize) -> Option<&str> {
        self.track_names.get(track_id).and_then(|n| n.as_deref())
    }

    /// 获取指定音轨的预解析音符引用（分块容器）。
    #[inline]
    pub fn track_notes(&self, track_id: usize) -> &crate::chunked_list::ChunkedList<NoteEvent> {
        static EMPTY: crate::chunked_list::ChunkedList<NoteEvent> =
            crate::chunked_list::ChunkedList::EMPTY;
        self.notes.get(track_id).unwrap_or(&EMPTY)
    }

    /// 指定音轨的最大音符结束 tick（O(1) 缓存命中；缓存脏时惰性重算一次 O(N)）。
    #[inline]
    pub fn track_max_end_tick(&self, track_id: usize) -> u32 {
        let Some(cell) = self.track_max_end_ticks.get(track_id) else {
            return 0;
        };
        if let Some(v) = cell.lock().ok().and_then(|g| *g) {
            return v;
        }
        // 惰性重算：end_tick 与 start_tick 排序无关，需全轨扫描取最大
        let max = self
            .notes
            .get(track_id)
            .map(|n| n.iter().map(|note| note.end_tick).max().unwrap_or(0))
            .unwrap_or(0);
        // 空轨 max=0 不缓存为 Some(0)（避免与"脏"语义混淆），保持脏（None），
        // 下次查询继续惰性重算（空轨重算成本为 0）。
        *cell.lock().unwrap_or_else(|e| e.into_inner()) = if max == 0 { None } else { Some(max) };
        max
    }

    /// 所有音轨的最大音符结束 tick（走带视图滚动范围用，O(音轨数)）。
    #[inline]
    pub fn tracks_max_end_tick(&self) -> u32 {
        (0..self.track_count())
            .map(|t| self.track_max_end_tick(t))
            .max()
            .unwrap_or(0)
    }
}
