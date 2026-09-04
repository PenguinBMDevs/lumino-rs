//! 编辑器音符变换操作
//!
//! 从 EditorData 中提取的变换方法：翻转、移调、变速。
//! 这些操作不直接属于 EditorData 的核心职责，因此提取为 trait 扩展。
//!
//! 2026-08 单一权威源改造：直接操作 document 当前轨（NoteEvent）。

use std::collections::HashSet;

use crate::EditorData;
use crate::editor_state::editor_data::CollabTransformSyncEntry;
use lumino_note_core::midi_types::VelocityPoint;

/// 音符变换操作 trait
pub trait EditorTransform {
    /// 垂直翻转选中音符
    fn flip_vertical(&mut self, selected: &HashSet<usize>, max_key_index: f32) -> usize;

    /// 水平翻转选中音符
    fn flip_horizontal(&mut self, selected: &HashSet<usize>, axis_tick: f32) -> usize;

    /// 移调选中音符（或全部音符）
    fn transpose(&mut self, selected: &HashSet<usize>, semitones: i16) -> usize;

    /// 变速选中音符（或全部音符）
    fn apply_speed_change(&mut self, selected: &HashSet<usize>, speed_factor: f32) -> usize;

    /// 构建力度点
    fn build_velocity_points(&self) -> Vec<VelocityPoint>;
}

impl EditorTransform for EditorData {
    fn flip_vertical(&mut self, selected: &HashSet<usize>, max_key_index: f32) -> usize {
        let selected_indices: Vec<usize> = selected.iter().copied().collect();
        if selected_indices.is_empty() {
            return 0;
        }
        let track = self.current_track_notes();
        let mut min_key = u8::MAX;
        let mut max_key = u8::MIN;
        for &note_idx in &selected_indices {
            if let Some(note) = track.get(note_idx) {
                min_key = min_key.min(note.key);
                max_key = max_key.max(note.key);
            }
        }
        if min_key > max_key {
            return 0;
        }
        let center = (min_key as f32 + max_key as f32) / 2.0;
        self.push_history();
        let mut modified = 0;
        let mut transitions: Vec<(lumino_midi_model::NoteEvent, lumino_midi_model::NoteEvent)> =
            Vec::new();
        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for &note_idx in &selected_indices {
                if let Some(note) = track.get_mut(note_idx) {
                    let old = *note;
                    let new_key = (2.0 * center - note.key as f32)
                        .round()
                        .clamp(0.0, max_key_index) as u8;
                    if new_key != note.key {
                        note.key = new_key;
                        transitions.push((old, *note));
                        modified += 1;
                    }
                }
            }
        }
        self.push_collab_transform_transitions(transitions);
        if modified > 0 {
            // 增量对账：等长修改记录事件（内部 mark 置 dirty 后清除）
            self.record_update_ranges(&selected_indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn flip_horizontal(&mut self, selected: &HashSet<usize>, axis_tick: f32) -> usize {
        let selected_indices: Vec<usize> = selected.iter().copied().collect();
        if selected_indices.is_empty() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        let mut transitions: Vec<(lumino_midi_model::NoteEvent, lumino_midi_model::NoteEvent)> =
            Vec::new();
        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for &note_idx in &selected_indices {
                if let Some(note) = track.get_mut(note_idx) {
                    let old = *note;
                    let tick = note.start_tick as f32;
                    let length = (note.end_tick - note.start_tick) as f32;
                    let new_tick = (2.0 * axis_tick - (tick + length)).max(0.0);
                    let new_tick_u =
                        crate::editor_state::editor_data::accessors::f32_to_tick(new_tick);
                    if new_tick_u != note.start_tick {
                        note.end_tick = note.end_tick.max(new_tick_u.saturating_add(1));
                        note.start_tick = new_tick_u;
                        transitions.push((old, *note));
                        modified += 1;
                    }
                }
            }
        }
        self.push_collab_transform_transitions(transitions);
        if modified > 0 {
            self.record_update_ranges(&selected_indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn transpose(&mut self, selected: &HashSet<usize>, semitones: i16) -> usize {
        let notes_len = self.current_track_note_count();
        let indices: Vec<usize> = if selected.is_empty() {
            (0..notes_len).collect()
        } else {
            selected.iter().copied().collect()
        };
        if indices.is_empty() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        let mut transitions: Vec<(lumino_midi_model::NoteEvent, lumino_midi_model::NoteEvent)> =
            Vec::new();
        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for &note_idx in &indices {
                if let Some(note) = track.get_mut(note_idx) {
                    let old = *note;
                    let new_key = (note.key as i16 + semitones).clamp(0, 255) as u8;
                    if new_key != note.key {
                        note.key = new_key;
                        transitions.push((old, *note));
                        modified += 1;
                    }
                }
            }
        }
        self.push_collab_transform_transitions(transitions);
        if modified > 0 {
            self.record_update_ranges(&indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn apply_speed_change(&mut self, selected: &HashSet<usize>, speed_factor: f32) -> usize {
        let notes_len = self.current_track_note_count();
        if notes_len == 0 {
            return 0;
        }
        let indices: Vec<usize> = if selected.is_empty() {
            (0..notes_len).collect()
        } else {
            let mut v: Vec<usize> = selected.iter().copied().collect();
            v.sort();
            v
        };
        if indices.is_empty() {
            return 0;
        }
        let track = self.current_track_notes();
        let min_tick = indices
            .iter()
            .filter_map(|idx| track.get(*idx).map(|note| note.start_tick as f32))
            .fold(f32::INFINITY, f32::min);
        if min_tick.is_infinite() {
            return 0;
        }
        self.push_history();
        let mut modified = 0;
        let mut transitions: Vec<(lumino_midi_model::NoteEvent, lumino_midi_model::NoteEvent)> =
            Vec::new();
        const MIN_LEN: f32 = 1.0;
        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for &note_idx in &indices {
                if let Some(note) = track.get_mut(note_idx) {
                    let old = *note;
                    let tick = note.start_tick as f32;
                    let length = (note.end_tick - note.start_tick) as f32;
                    let new_tick = min_tick + (tick - min_tick) * speed_factor;
                    let new_length = (length * speed_factor).max(MIN_LEN);
                    let new_tick_u =
                        crate::editor_state::editor_data::accessors::f32_to_tick(new_tick);
                    let new_end_u = crate::editor_state::editor_data::accessors::f32_to_tick(
                        new_tick + new_length,
                    );
                    if new_tick_u != note.start_tick || new_end_u != note.end_tick {
                        note.start_tick = new_tick_u;
                        note.end_tick = new_end_u.max(new_tick_u.saturating_add(1));
                        transitions.push((old, *note));
                        modified += 1;
                    }
                }
            }
        }
        self.push_collab_transform_transitions(transitions);
        if modified > 0 {
            self.record_update_ranges(&indices);
        } else {
            self.history.discard_last();
        }
        modified
    }

    fn build_velocity_points(&self) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = self
            .current_track_notes()
            .iter()
            .enumerate()
            .map(|(note_idx, note)| VelocityPoint {
                note_index: note_idx,
                tick: note.start_tick as f32,
                velocity: note.velocity,
                length: (note.end_tick - note.start_tick) as f32,
            })
            .collect();
        points.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.note_index.cmp(&b.note_index))
        });
        points
    }
}

impl EditorData {
    /// 把单个「旧→新」音符状态变换推入协作同步队列（显式指定音轨）。
    ///
    /// `pub` 以允许 ui-editor 仓的跨音轨变速（arrangement）在变换后从外部入队。
    /// 每个变换对以「删除旧音符 + 添加新音符」两条记录入队，复用已修复的
    /// `LocalNoteDeleted`/`LocalNoteAdded` 通道（最稳、覆盖全部字段）。
    pub fn push_collab_transform_transition(
        &mut self,
        old: lumino_midi_model::NoteEvent,
        new: lumino_midi_model::NoteEvent,
        track: usize,
    ) {
        self.pending_collab_transform_sync.push((
            false,
            old.id,
            old.start_tick as f32,
            old.key as u16,
            old.length() as f32,
            old.velocity,
            old.channel,
            track,
        ));
        self.pending_collab_transform_sync.push((
            true,
            new.id,
            new.start_tick as f32,
            new.key as u16,
            new.length() as f32,
            new.velocity,
            new.channel,
            track,
        ));
    }

    /// 批量推送「删除旧 / 添加新」协作同步条目（ui-editor 层如 Razor 切割使用）。
    ///
    /// `pub`：`pending_collab_transform_sync` 为 `pub(crate)`，ui-editor 仓无法直接访问，
    /// 故开放此批量入口。元组语义与 `pending_collab_transform_sync` 完全一致：
    /// `(is_add, 音符全局唯一 ID, tick, key, length, velocity, channel, track_index)`。
    pub fn push_collab_transform_entries(&mut self, entries: Vec<CollabTransformSyncEntry>) {
        self.pending_collab_transform_sync.extend(entries);
    }

    /// 把前向变换产生的「旧→新」音符状态对批量推入协作同步队列（使用当前音轨）。
    pub(crate) fn push_collab_transform_transitions(
        &mut self,
        transitions: Vec<(lumino_midi_model::NoteEvent, lumino_midi_model::NoteEvent)>,
    ) {
        let track = self.current_track;
        for (old, new) in transitions {
            self.push_collab_transform_transition(old, new, track);
        }
    }
}
