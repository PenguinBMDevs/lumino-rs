//! 工程走带视图音符操作（跨音轨）
//!
//! 提供 arrange_move_notes / arrange_erase / arrange_razor 三个操作，
//! 直接修改 EditorData::track_notes，并在当前音轨受影响时同步 data.notes。

use std::collections::{HashMap, HashSet};

use super::Editor;
use crate::note::Note;

impl Editor {
    /// 移动工程走带选择区内的音符。
    ///
    /// 支持跨音轨移动（delta_tracks != 0）。移动后调用方需自行同步选择矩形。
    /// 返回实际移动的音符数。
    pub fn arrange_move_notes(&mut self, delta_ticks: i64, delta_tracks: i32) -> usize {
        if self.editor_state.data.arrange_selection.is_empty()
            || (delta_ticks == 0 && delta_tracks == 0)
        {
            return 0;
        }

        let selection = self.editor_state.data.arrange_selection.clone();
        let mut indices_by_source: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut moved_by_dest: HashMap<usize, Vec<Note>> = HashMap::new();

        {
            let data = &self.editor_state.data;
            for (&track_idx, notes) in &data.track_notes {
                for (i, note) in notes.iter().enumerate() {
                    if selection.contains(track_idx as u16, note.tick as u32, note.key as u8) {
                        let dest_track = (track_idx as i32 + delta_tracks).max(0) as usize;
                        let new_tick = (note.tick as f64 + delta_ticks as f64).max(0.0) as f32;
                        let mut moved = note.clone();
                        moved.tick = new_tick;
                        indices_by_source.entry(track_idx).or_default().insert(i);
                        moved_by_dest.entry(dest_track).or_default().push(moved);
                    }
                }
            }
        }

        if indices_by_source.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut moved_count = 0usize;

        {
            let data = &mut self.editor_state.data;
            for (source_track, indices) in indices_by_source {
                if source_track == current_track {
                    current_track_touched = true;
                }
                if let Some(notes) = data.track_notes.get_mut(&source_track) {
                    let before = notes.len();
                    let mut idx = 0usize;
                    notes.retain(|_| {
                        let keep = !indices.contains(&idx);
                        idx += 1;
                        keep
                    });
                    moved_count += before - notes.len();
                }
            }

            for (dest_track, notes_to_add) in moved_by_dest {
                if dest_track == current_track {
                    current_track_touched = true;
                }
                let track_entry = data.track_notes.entry(dest_track).or_default();
                for note in notes_to_add {
                    track_entry.push_back(note);
                }
            }
        }

        if moved_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 移动 {} 个音符 (delta_ticks={}, delta_tracks={})",
            moved_count,
            delta_ticks,
            delta_tracks
        );
        moved_count
    }

    /// 擦除工程走带矩形范围内的音符。
    ///
    /// 返回实际删除的音符数。
    pub fn arrange_erase(
        &mut self,
        tick_start: f64,
        tick_end: f64,
        track_lo: usize,
        track_hi: usize,
    ) -> usize {
        if tick_start >= tick_end || track_lo > track_hi {
            return 0;
        }

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut tracks_to_clean: Vec<usize> = Vec::new();

        {
            let data = &self.editor_state.data;
            for track_idx in track_lo..=track_hi {
                if let Some(notes) = data.track_notes.get(&track_idx) {
                    let has_any = notes
                        .iter()
                        .any(|note| note_in_rect(note, tick_start, tick_end));
                    if has_any {
                        tracks_to_clean.push(track_idx);
                        if track_idx == current_track {
                            current_track_touched = true;
                        }
                    }
                }
            }
        }

        if tracks_to_clean.is_empty() {
            return 0;
        }

        self.push_history();

        let mut deleted_count = 0usize;
        {
            let data = &mut self.editor_state.data;
            for track_idx in tracks_to_clean {
                if let Some(notes) = data.track_notes.get_mut(&track_idx) {
                    let before = notes.len();
                    notes.retain(|note| !note_in_rect(note, tick_start, tick_end));
                    deleted_count += before - notes.len();
                }
            }
        }

        if deleted_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 擦除 {} 个音符 (tick {}..{}, track {}..={})",
            deleted_count,
            tick_start,
            tick_end,
            track_lo,
            track_hi
        );
        deleted_count
    }

    /// 在指定 tick/音轨处分割音符（Razor 工具）。
    ///
    /// 返回实际分割的音符数。
    pub fn arrange_razor(&mut self, tick: f64, track: usize) -> usize {
        let tick_f = tick as f32;

        let indices_to_split: Vec<usize> = {
            let data = &self.editor_state.data;
            let Some(notes) = data.track_notes.get(&track) else {
                return 0;
            };
            notes
                .iter()
                .enumerate()
                .filter_map(|(i, note)| {
                    if note.tick < tick_f && note.tick + note.length > tick_f {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect()
        };

        if indices_to_split.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let current_track_touched = track == current_track;
        let mut split_count = 0usize;

        {
            let data = &mut self.editor_state.data;
            if let Some(notes) = data.track_notes.get_mut(&track) {
                // 从后往前分割，避免索引漂移
                for idx in indices_to_split.into_iter().rev() {
                    if let Some(note) = notes.get(idx).cloned() {
                        let left = Note::from_raw(
                            note.tick,
                            note.key,
                            tick_f - note.tick,
                            note.velocity,
                            note.channel,
                        );
                        let right = Note::from_raw(
                            tick_f,
                            note.key,
                            note.tick + note.length - tick_f,
                            note.velocity,
                            note.channel,
                        );
                        notes.remove(idx);
                        notes.insert(idx, right);
                        notes.insert(idx, left);
                        split_count += 1;
                    }
                }
            }
        }

        if split_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 分割 {} 个音符 (tick={}, track={})",
            split_count,
            tick,
            track
        );
        split_count
    }

    /// 获取当前工程走带选择范围内的音符列表。
    ///
    /// 返回 `(tick_start, tick_end, track, key)`，用于 ghost 预览。
    pub fn arrangement_selected_notes(&self) -> Vec<(f64, f64, usize, u8)> {
        let data = &self.editor_state.data;
        let selection = &data.arrange_selection;
        if selection.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        for (&track_idx, notes) in &data.track_notes {
            for note in notes {
                if selection.contains(track_idx as u16, note.tick as u32, note.key as u8) {
                    result.push((
                        note.tick as f64,
                        (note.tick + note.length) as f64,
                        track_idx,
                        note.key as u8,
                    ));
                }
            }
        }
        result
    }

    /// 在工程走带指定音轨 tick 处添加一个音符。
    ///
    /// 返回是否实际添加。
    pub fn arrange_add_note(
        &mut self,
        track_count: usize,
        track: usize,
        tick: f64,
        duration: f64,
        key: u8,
        velocity: u8,
    ) -> bool {
        if tick < 0.0 || duration <= 0.0 || track >= track_count {
            return false;
        }

        let tick_f = tick as f32;
        let length_f = duration as f32;
        let key_u16 = key as u16;
        let note = Note::from_raw(tick_f, key_u16, length_f, velocity, 0);

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let current_track_touched = track == current_track;

        {
            let data = &mut self.editor_state.data;
            let track_entry = data.track_notes.entry(track).or_default();
            track_entry.push_back(note);
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 添加音符 (tick={}, duration={}, track={}, key={}, velocity={})",
            tick,
            duration,
            track,
            key,
            velocity
        );
        true
    }

    /// 工程走带操作后，若当前音轨受影响则同步 data.notes 与 NoteStore。
    fn sync_current_track_after_arrange_op(&mut self, touched: bool) {
        if !touched {
            return;
        }
        let data = &mut self.editor_state.data;
        data.notes = data
            .track_notes
            .get(&data.current_track)
            .cloned()
            .unwrap_or_default();
        if data.is_note_store_enabled() {
            data.sync_note_store();
        }
        self.mark_notes_changed();
    }
}

/// 判断音符是否与擦除矩形相交（tick 半开区间 [tick_start, tick_end)）。
fn note_in_rect(note: &Note, tick_start: f64, tick_end: f64) -> bool {
    let ne = note.tick + note.length;
    note.tick < tick_end as f32 && ne > tick_start as f32
}
