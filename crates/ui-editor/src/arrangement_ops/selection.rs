//! 工程走带选中音符查询与批量操作
//!
//! 提供以下操作：
//! - `arrangement_selected_notes`: 获取选中音符列表（用于 ghost 预览）
//! - `arrange_delete_selected_notes`: 删除选中音符
//! - `arrange_apply_speed_change`: 选中音符批量变速

use std::collections::HashMap;

use super::Editor;
use super::helpers::note_event_to_note;

impl Editor {
    /// 获取当前工程走带选择范围内的音符列表。
    ///
    /// 返回 `(tick_start, tick_end, track, key)`，用于 ghost 预览。
    /// track 为视觉位置（侧边栏顺序），而非文档音轨索引。
    /// ghost 计算中的 dtr 是视觉空间偏移，与视觉位置相加得到正确的渲染位置。
    pub fn arrangement_selected_notes(&self) -> Vec<(f64, f64, usize, u8)> {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        if selection.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();

        // 1. 从 track_notes 缓存收集
        for (&track_idx, notes) in &editor_data.track_notes {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for note in notes {
                if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                    result.push((
                        note.tick as f64,
                        (note.tick + note.length) as f64,
                        visual_pos,
                        note.key as u8,
                    ));
                }
            }
        }

        // 2. 从 MidiDocument 收集未加载到 track_notes 的音轨中的音符
        if let Some(doc) = &editor_data.document {
            for track_idx in 0..doc.notes.len() {
                if editor_data.track_notes.contains_key(&track_idx) {
                    continue;
                }
                let visual_pos = editor_data
                    .visual_position_of(track_idx)
                    .unwrap_or(track_idx);
                for note_event in doc.track_notes(track_idx) {
                    if selection.contains(visual_pos as u16, note_event.start_tick, note_event.key)
                    {
                        result.push((
                            note_event.start_tick as f64,
                            note_event.end_tick as f64,
                            visual_pos,
                            note_event.key,
                        ));
                    }
                }
            }
        }

        result
    }

    /// 删除工程走带选择区内的所有音符。
    ///
    /// 返回实际删除的音符数。
    pub fn arrange_delete_selected_notes(&mut self) -> usize {
        if self.editor_state.data.arrange_selection.is_empty() {
            return 0;
        }

        self.load_missing_tracks_from_document();

        let indices_by_track = self.collect_delete_targets();

        if indices_by_track.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut deleted_count = 0usize;

        {
            let editor_data = &mut self.editor_state.data;
            for (track_idx, mut indices) in indices_by_track {
                if track_idx == current_track {
                    current_track_touched = true;
                }
                indices.sort_unstable_by(|a, b| b.cmp(a));
                if let Some(notes) = editor_data.track_notes.get_mut(&track_idx) {
                    for idx in indices {
                        notes.remove(idx);
                        deleted_count += 1;
                    }
                }
            }
        }

        if deleted_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!("Arrangement: 删除 {} 个音符", deleted_count);
        deleted_count
    }

    /// 对工程走带选择区内的音符执行批量变速。
    ///
    /// 行为与钢琴卷帘 `apply_speed_change` 一致：以选中音符的最小 tick 为基准，
    /// 按 `speed_factor` 缩放 tick 和 length。支持跨音轨操作。
    /// 返回实际修改的音符数。
    pub fn arrange_apply_speed_change(&mut self, speed_factor: f32) -> usize {
        if self.editor_state.data.arrange_selection.is_empty() {
            return 0;
        }

        self.load_missing_tracks_from_document();

        let selection = self.editor_state.data.arrange_selection.clone();
        let (track_indices, min_tick) = self.collect_speed_change_targets(&selection);

        if track_indices.is_empty() || min_tick.is_infinite() {
            return 0;
        }

        let (modified_count, current_track_touched) =
            self.apply_speed_change_internal(track_indices, min_tick, speed_factor);

        if modified_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 变速 {} 个音符 (factor={})",
            modified_count,
            speed_factor,
        );
        modified_count
    }

    /// 收集删除操作的目标音轨和索引。
    fn collect_delete_targets(&self) -> HashMap<usize, Vec<usize>> {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        let mut indices_by_track: HashMap<usize, Vec<usize>> = HashMap::new();
        for (&track_idx, notes) in &editor_data.track_notes {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for (i, note) in notes.iter().enumerate() {
                if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                    indices_by_track.entry(track_idx).or_default().push(i);
                }
            }
        }
        indices_by_track
    }

    /// 执行变速：按 speed_factor 缩放选中音符的 tick 和 length。
    /// 返回 (modified_count, current_track_touched)。
    fn apply_speed_change_internal(
        &mut self,
        track_indices: HashMap<usize, Vec<usize>>,
        min_tick: f32,
        speed_factor: f32,
    ) -> (usize, bool) {
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut modified_count = 0usize;
        const MIN_LEN: f32 = 1.0;

        let editor_data = &mut self.editor_state.data;
        for (track_idx, indices) in &track_indices {
            if *track_idx == current_track {
                current_track_touched = true;
            }
            if let Some(notes) = editor_data.track_notes.get_mut(track_idx) {
                for &i in indices {
                    if let Some(note) = notes.get_mut(i) {
                        let nt = min_tick + (note.tick - min_tick) * speed_factor;
                        let nl = (note.length * speed_factor).max(MIN_LEN);
                        if (nt - note.tick).abs() > f32::EPSILON
                            || (nl - note.length).abs() > f32::EPSILON
                        {
                            note.tick = nt;
                            note.length = nl;
                            modified_count += 1;
                        }
                    }
                }
            }
        }

        (modified_count, current_track_touched)
    }

    /// 收集变速操作的目标音轨索引和最小 tick。
    fn collect_speed_change_targets(
        &self,
        selection: &lumino_note_core::ArrangeSelection,
    ) -> (HashMap<usize, Vec<usize>>, f32) {
        let mut track_indices: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut min_tick = f32::INFINITY;

        let editor_data = &self.editor_state.data;
        for (&track_idx, notes) in &editor_data.track_notes {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for (i, note) in notes.iter().enumerate() {
                if selection.contains(visual_pos as u16, note.tick as u32, note.key as u8) {
                    track_indices.entry(track_idx).or_default().push(i);
                    min_tick = min_tick.min(note.tick);
                }
            }
        }

        (track_indices, min_tick)
    }
}
