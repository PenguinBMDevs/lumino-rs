//! 事件浏览器 CRUD 操作
//!
//! 提供拍号、调号、标记、歌词、和弦、音色变换以及自动化事件的
//! 设置、删除、插入方法。所有修改均先 push 快照到历史记录。

use std::collections::HashSet;
use std::sync::Arc;

use lumino_note_core::automation as auto;
use lumino_note_core::event::{
    AutomationEvent, AutomationTarget, ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent,
    ProgramChangeEvent, ScaleType, SegmentShape, TimeSignatureEvent,
};
use lumino_note_core::midi_types::TempoPoint;
use lumino_note_core::note::Note;

use super::EditorData;

/// 通用事件 trait：按 tick 排序
trait EventByTick {
    /// 返回事件 tick
    fn tick(&self) -> u32;
}

/// 带音轨归属的事件 trait：用于 per-track 事件（Lyrics/Chord/PC）
trait EventByTrack: EventByTick {
    /// 返回事件所属音轨
    fn track(&self) -> u16;
}

macro_rules! impl_event_by_tick {
    ($t:ty) => {
        impl EventByTick for $t {
            fn tick(&self) -> u32 {
                self.tick
            }
        }
    };
}

impl_event_by_tick!(TimeSignatureEvent);
impl_event_by_tick!(KeySignatureEvent);
impl_event_by_tick!(MarkerEvent);
impl_event_by_tick!(LyricsEvent);
impl_event_by_tick!(ChordEvent);
impl_event_by_tick!(ProgramChangeEvent);
impl_event_by_tick!(AutomationEvent);

impl EventByTrack for LyricsEvent {
    fn track(&self) -> u16 {
        self.track
    }
}
impl EventByTrack for ChordEvent {
    fn track(&self) -> u16 {
        self.track
    }
}
impl EventByTrack for ProgramChangeEvent {
    fn track(&self) -> u16 {
        self.track
    }
}

impl EventByTick for (u32, u8, u8) {
    fn tick(&self) -> u32 {
        self.0
    }
}

/// 替换或插入事件，保持按 tick 升序
fn replace_or_insert_event<T: EventByTick>(events: &mut Vec<T>, event: T) {
    if let Some(pos) = events.iter().position(|e| e.tick() == event.tick()) {
        events[pos] = event;
    } else {
        let idx = events.partition_point(|e| e.tick() < event.tick());
        events.insert(idx, event);
    }
}

/// 替换或插入带音轨归属的事件：按 (track, tick) 定位，保持按 tick 升序
fn replace_or_insert_tracked_event<T: EventByTrack>(events: &mut Vec<T>, event: T) {
    if let Some(pos) = events
        .iter()
        .position(|e| e.track() == event.track() && e.tick() == event.tick())
    {
        events[pos] = event;
    } else {
        let idx = events.partition_point(|e| e.tick() < event.tick());
        events.insert(idx, event);
    }
}

/// 删除指定 tick 的事件
fn delete_events_by_ticks<T: EventByTick>(events: &mut Vec<T>, ticks: &HashSet<u32>) {
    events.retain(|e| !ticks.contains(&e.tick()));
}

/// 删除指定音轨中指定 tick 的事件
fn delete_tracked_events_by_ticks<T: EventByTrack>(
    events: &mut Vec<T>,
    track: u16,
    ticks: &HashSet<u32>,
) {
    events.retain(|e| e.track() != track || !ticks.contains(&e.tick()));
}

impl EditorData {
    // ── TimeSig ───────────────────────────────────────────────

    /// 设置或替换拍号事件
    pub fn set_time_sig_event(&mut self, tick: u32, numerator: u8, denominator: u8) {
        self.push_history();
        replace_or_insert_event(&mut self.time_signatures, (tick, numerator, denominator));
    }

    /// 删除指定 tick 的拍号事件
    pub fn delete_time_sig_events(&mut self, ticks: &HashSet<u32>) {
        self.push_history();
        delete_events_by_ticks(&mut self.time_signatures, ticks);
    }

    /// 在指定 tick 插入默认 4/4 拍号事件
    pub fn insert_time_sig_event(&mut self, tick: u32) {
        self.set_time_sig_event(tick, 4, 4);
    }

    // ── KeySig ───────────────────────────────────────────────

    /// 设置或替换调号事件
    pub fn set_key_sig_event(&mut self, tick: u32, root: u8, scale: ScaleType) {
        self.push_history();
        replace_or_insert_event(
            &mut self.key_signatures,
            KeySignatureEvent { tick, root, scale },
        );
    }

    /// 删除指定 tick 的调号事件
    pub fn delete_key_sig_events(&mut self, ticks: &HashSet<u32>) {
        self.push_history();
        delete_events_by_ticks(&mut self.key_signatures, ticks);
    }

    /// 在指定 tick 插入默认 C 大调调号事件
    pub fn insert_key_sig_event(&mut self, tick: u32) {
        self.set_key_sig_event(tick, 0, ScaleType::Major);
    }

    // ── Markers ──────────────────────────────────────────────

    /// 设置或替换标记事件
    pub fn set_marker_event(&mut self, tick: u32, text: String) {
        self.push_history();
        replace_or_insert_event(&mut self.markers, MarkerEvent { tick, text });
    }

    /// 删除指定 tick 的标记事件
    pub fn delete_marker_events(&mut self, ticks: &HashSet<u32>) {
        self.push_history();
        delete_events_by_ticks(&mut self.markers, ticks);
    }

    /// 在指定 tick 插入默认标记事件
    pub fn insert_marker_event(&mut self, tick: u32) {
        self.set_marker_event(tick, "New".into());
    }

    // ── Lyrics ───────────────────────────────────────────────

    /// 设置或替换歌词事件
    pub fn set_lyrics_event(&mut self, track: u16, tick: u32, text: String) {
        self.push_history();
        replace_or_insert_tracked_event(&mut self.lyrics, LyricsEvent { track, tick, text });
    }

    /// 删除指定音轨指定 tick 的歌词事件
    pub fn delete_lyrics_events(&mut self, track: u16, ticks: &HashSet<u32>) {
        self.push_history();
        delete_tracked_events_by_ticks(&mut self.lyrics, track, ticks);
    }

    /// 在指定音轨指定 tick 插入空歌词事件
    pub fn insert_lyrics_event(&mut self, track: u16, tick: u32) {
        self.set_lyrics_event(track, tick, String::new());
    }

    // ── Chord ────────────────────────────────────────────────

    /// 设置或替换和弦事件
    pub fn set_chord_event(&mut self, track: u16, tick: u32, text: String) {
        self.push_history();
        replace_or_insert_tracked_event(&mut self.chords, ChordEvent { track, tick, text });
    }

    /// 删除指定音轨指定 tick 的和弦事件
    pub fn delete_chord_events(&mut self, track: u16, ticks: &HashSet<u32>) {
        self.push_history();
        delete_tracked_events_by_ticks(&mut self.chords, track, ticks);
    }

    /// 在指定音轨指定 tick 插入空和弦事件
    pub fn insert_chord_event(&mut self, track: u16, tick: u32) {
        self.set_chord_event(track, tick, String::new());
    }

    // ── Program Change ───────────────────────────────────────

    /// 设置或替换音色变换事件
    pub fn set_program_change_event(&mut self, track: u16, tick: u32, program: u8) {
        self.push_history();
        replace_or_insert_tracked_event(
            &mut self.program_changes,
            ProgramChangeEvent {
                track,
                tick,
                program,
            },
        );
    }

    /// 删除指定音轨指定 tick 的音色变换事件
    pub fn delete_program_change_events(&mut self, track: u16, ticks: &HashSet<u32>) {
        self.push_history();
        delete_tracked_events_by_ticks(&mut self.program_changes, track, ticks);
    }

    /// 在指定音轨指定 tick 插入默认音色 0 事件
    pub fn insert_program_change_event(&mut self, track: u16, tick: u32) {
        self.set_program_change_event(track, tick, 0);
    }

    // ── Automation ───────────────────────────────────────────

    /// 设置或替换自动化事件
    ///
    /// 非 Tempo 目标会写入对应自动化 lane；Tempo 目标写入 `tempo_points`。
    /// 曲线控制点目前映射为直线 tension=0，后续可扩展贝塞尔到 tension 的转换。
    pub fn set_automation_event(
        &mut self,
        track: u16,
        target: AutomationTarget,
        tick: u32,
        value: f32,
        shape: SegmentShape,
    ) {
        if target == AutomationTarget::Tempo {
            self.push_history();
            let point = TempoPoint {
                tick: tick as f32,
                bpm: value as f64,
            };
            if let Some(pos) = self.tempo_points.iter().position(|p| p.tick as u32 == tick) {
                self.tempo_points[pos] = point;
            } else {
                let idx = self
                    .tempo_points
                    .partition_point(|p| (p.tick as u32) < tick);
                self.tempo_points.insert(idx, point);
            }
            return;
        }

        let auto_target = super::shape_convert::event_target_to_auto_target(&target);
        let max = auto_target.max_value() as f32;
        let auto_value = value.max(0.0).min(max).round() as u16;
        let auto_shape = super::shape_convert::event_shape_to_auto_shape(shape);

        self.push_history();
        let idx = self.find_or_create_automation_lane(track, auto_target);
        let lane = Arc::make_mut(&mut self.automation_lanes[idx]);
        lane.events.retain(|e| e.tick != tick);
        lane.events.push(auto::AutomationEvent {
            tick,
            value: auto_value,
            shape: auto_shape,
        });
        lane.events.sort_by_key(|e| e.tick);
    }

    /// 删除指定自动化事件
    ///
    /// Tempo 目标会删除 `tempo_points` 中对应 tick 的点。
    pub fn delete_automation_events(
        &mut self,
        track: u16,
        target: &AutomationTarget,
        ticks: &HashSet<u32>,
    ) {
        if *target == AutomationTarget::Tempo {
            self.push_history();
            self.tempo_points
                .retain(|p| !ticks.contains(&(p.tick as u32)));
            return;
        }

        let auto_target = super::shape_convert::event_target_to_auto_target(target);
        if let Some(idx) = self.find_automation_lane(track, &auto_target) {
            self.push_history();
            let lane = Arc::make_mut(&mut self.automation_lanes[idx]);
            lane.events.retain(|e| !ticks.contains(&e.tick));
        }
    }

    /// 在指定 tick 插入默认自动化事件
    ///
    /// Tempo 默认值为 120 BPM，其余目标默认值为 0。
    pub fn insert_automation_event(&mut self, track: u16, target: &AutomationTarget, tick: u32) {
        let default_value = if *target == AutomationTarget::Tempo {
            120.0
        } else {
            0.0
        };
        self.set_automation_event(track, *target, tick, default_value, SegmentShape::Step);
    }

    // ── Notes 便捷操作 ───────────────────────────────────────

    /// 删除当前音轨中 tick 位于集合中的音符
    pub fn delete_notes_at_ticks(&mut self, ticks: &HashSet<u32>) {
        if self.current_track == 0 || ticks.is_empty() || self.notes.is_empty() {
            return;
        }

        let indices: HashSet<usize> = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, note)| note.tick >= 0.0 && ticks.contains(&(note.tick as u32)))
            .map(|(idx, _)| idx)
            .collect();

        if indices.is_empty() {
            return;
        }

        self.push_history();
        self.batch_delete_notes_from_set(&indices);
    }

    /// 在当前音轨指定 tick 插入默认 C4 音符
    ///
    /// 返回创建的音符；若当前音轨为 0（Conductor）则返回 None。
    pub fn insert_note_at_tick(&mut self, tick: f32) -> Option<Note> {
        if self.current_track == 0 {
            return None;
        }

        self.push_history();
        let note = Note::new(tick, 60, 480.0);
        self.push_note(note.clone());
        Some(note)
    }
}
