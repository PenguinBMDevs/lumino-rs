//! 工程走带音符移动操作（跨音轨）
//!
//! 支持 delta_ticks 和 delta_tracks 偏移。
//! 移动后调用方需自行同步选择矩形。

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

        self.load_missing_tracks_from_document();
        let selection = self.editor_state.data.arrange_selection.clone();

        let (indices_by_source, moved_by_dest) =
            self.collect_move_sources_and_dests(&selection, delta_ticks, delta_tracks);

        if indices_by_source.is_empty() {
            return 0;
        }

        let (moved_count, current_track_touched) =
            self.apply_move_internal(indices_by_source, moved_by_dest);

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

    /// 执行移动：从源音轨移除音符，插入目标音轨。
    /// 返回 (moved_count, current_track_touched)。
    fn apply_move_internal(
        &mut self,
        indices_by_source: HashMap<usize, HashSet<usize>>,
        moved_by_dest: HashMap<usize, Vec<Note>>,
    ) -> (usize, bool) {
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut moved_count = 0usize;

        let editor_data = &mut self.editor_state.data;
        for (source_track, indices) in indices_by_source {
            if source_track == current_track {
                current_track_touched = true;
            }
            if let Some(notes) = editor_data.track_notes.get_mut(&source_track) {
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
            let track_entry = editor_data.track_notes.entry(dest_track).or_default();
            for note in notes_to_add {
                track_entry.push_back(note);
            }
        }

        (moved_count, current_track_touched)
    }

    /// 收集移动操作的源音轨索引和目标音符（第一遍扫描）。
    fn collect_move_sources_and_dests(
        &self,
        selection: &lumino_core::ArrangeSelection,
        delta_ticks: i64,
        delta_tracks: i32,
    ) -> (HashMap<usize, HashSet<usize>>, HashMap<usize, Vec<Note>>) {
        let mut indices_by_source: HashMap<usize, HashSet<usize>> = HashMap::new();
        let mut moved_by_dest: HashMap<usize, Vec<Note>> = HashMap::new();

        let editor_data = &self.editor_state.data;
        for (&track_idx, notes) in &editor_data.track_notes {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for (i, note) in notes.iter().enumerate() {
                if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                    let dest_visual = (visual_pos as i32 + delta_tracks).max(0) as usize;
                    let dest_track = editor_data
                        .track_visual_order
                        .get(dest_visual)
                        .copied()
                        .unwrap_or(dest_visual);
                    let new_tick = (note.tick as f64 + delta_ticks as f64).max(0.0) as f32;
                    let mut moved = note.clone();
                    moved.tick = new_tick;
                    indices_by_source.entry(track_idx).or_default().insert(i);
                    moved_by_dest.entry(dest_track).or_default().push(moved);
                }
            }
        }

        (indices_by_source, moved_by_dest)
    }
}
