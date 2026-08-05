//! 工程走带选中音符查询与批量操作
//!
//! 提供以下操作：
//! - `arrangement_selected_notes`: 获取选中音符列表（用于 ghost 预览）
//! - `arrange_delete_selected_notes`: 删除选中音符
//! - `arrange_apply_speed_change`: 选中音符批量变速
//!
//! 2026-08 单一权威源：音符唯一权威是 document，本模块直接读写 MidiDocument，
//! 不再维护 track_notes 缓存。

use std::collections::HashMap;

use super::Editor;

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

        // 2026-08 单一权威源：直接从 document 遍历全部音轨（track_notes 缓存已删除）
        let Some(doc) = &editor_data.document else {
            return result;
        };
        for track_idx in 0..doc.track_count() {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for note_event in editor_data.track_notes(track_idx) {
                if selection.contains(visual_pos as u16, note_event.start_tick, note_event.key) {
                    result.push((
                        note_event.start_tick as f64,
                        note_event.end_tick as f64,
                        visual_pos,
                        note_event.key,
                    ));
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

        let indices_by_track = self.collect_delete_targets();

        if indices_by_track.is_empty() {
            return 0;
        }

        // 精确记录受影响音轨（洋葱皮事件级增量：只重传这些音轨）
        let affected_tracks: std::collections::HashSet<usize> =
            indices_by_track.keys().copied().collect();

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut deleted_count = 0usize;

        for (track_idx, mut indices) in indices_by_track {
            if track_idx == current_track {
                current_track_touched = true;
            }
            // 2026-08 单一权威源：索引降序逐个删除 document 音符
            indices.sort_unstable_by(|a, b| b.cmp(a));
            for idx in indices {
                if self.editor_state.data.remove_note(track_idx, idx).is_some() {
                    deleted_count += 1;
                }
            }
        }

        if deleted_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        if current_track_touched {
            self.mark_notes_changed();
        }
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected_tracks));
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

        let selection = self.editor_state.data.arrange_selection.clone();
        let (track_indices, min_tick) = self.collect_speed_change_targets(&selection);

        if track_indices.is_empty() || min_tick.is_infinite() {
            return 0;
        }

        // 精确记录受影响音轨（洋葱皮事件级增量）
        let affected_tracks: std::collections::HashSet<usize> =
            track_indices.keys().copied().collect();

        let (modified_count, current_track_touched) =
            self.apply_speed_change_internal(track_indices, min_tick, speed_factor);

        if modified_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        if current_track_touched {
            self.mark_notes_changed();
        }
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected_tracks));
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
        // 2026-08 单一权威源：从 document 收集（track_notes 缓存已删除）
        let Some(doc) = &editor_data.document else {
            return indices_by_track;
        };
        for track_idx in 0..doc.track_count() {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for (i, note) in editor_data.track_notes(track_idx).iter().enumerate() {
                if selection.contains(visual_pos as u16, note.start_tick, note.key) {
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

        // 2026-08 单一权威源：直接修改 document 各轨音符（track_notes_mut）
        for (track_idx, indices) in &track_indices {
            if *track_idx == current_track {
                current_track_touched = true;
            }
            if let Some(notes) = self
                .editor_state
                .data
                .document
                .as_mut()
                .and_then(|doc| doc.track_notes_mut(*track_idx))
            {
                for &i in indices {
                    if let Some(note) = notes.get_mut(i) {
                        let tick = note.start_tick as f32;
                        let length = (note.end_tick - note.start_tick) as f32;
                        let nt = min_tick + (tick - min_tick) * speed_factor;
                        let nl = (length * speed_factor).max(MIN_LEN);
                        if (nt - tick).abs() > f32::EPSILON || (nl - length).abs() > f32::EPSILON {
                            let new_start = nt.max(0.0);
                            note.start_tick = new_start as u32;
                            note.end_tick = note.start_tick + nl as u32;
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
        // 2026-08 单一权威源：从 document 收集（track_notes 缓存已删除）
        let Some(doc) = &editor_data.document else {
            return (track_indices, min_tick);
        };
        for track_idx in 0..doc.track_count() {
            let visual_pos = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            for (i, note) in editor_data.track_notes(track_idx).iter().enumerate() {
                if selection.contains(visual_pos as u16, note.start_tick, note.key) {
                    track_indices.entry(track_idx).or_default().push(i);
                    min_tick = min_tick.min(note.start_tick as f32);
                }
            }
        }

        (track_indices, min_tick)
    }
}
