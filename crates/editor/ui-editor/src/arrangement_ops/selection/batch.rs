//! 工程走带选区的**批量操作**（删除 / 变速）
//!
//! 与 `selection.rs`（选区查询与视图无关解析）分离：批量操作是**写路径**
//! （收集目标 → push_history → 改 document → 标脏），查询是**读路径**
//! （走 `selection_cache` 派生缓存）。两者变更节奏与失效条件完全不同，
//! 混在一处会让读路径的缓存约束被写路径的细节淹没。
//!
//! 2026-09 自 `selection.rs` 拆出，保持主文件 ≤400 行。
//!
//! 选区命中判定统一走**视觉轨**（`visual_position_of` 转换）——
//! `track_visual_order` 非恒等时按文档索引判定会全错，
//! 与 2025-07 修过的 `arrangement-y-axis-movement` 是同一个坑。

use std::collections::HashMap;

use super::super::Editor;

impl Editor {
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

        for (track_idx, indices) in indices_by_track {
            if track_idx == current_track {
                current_track_touched = true;
            }
            // 区间归并删除：连续删除段合并为单条事件（当前轨走主轨段内增量，
            // 非当前轨走 `TrackRemoveRanges` 区间增量），不再逐音符一条事件
            // （K 次 GPU 尾部搬移 + bind group 重建）。
            deleted_count += self
                .editor_state
                .data
                .remove_notes_merged(track_idx, &indices);
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

        // P0 修复（历史链断链）：同 `arrange_move_notes`——`apply_speed_change_internal`
        // 走的也是 `insert_note`/`remove_note`，契约要求调用方先 push。原实现漏 push，
        // 导致走带批量变速**撤不掉**，且下方 modified_count == 0 的兜底 discard 会
        // 吞掉用户上一次真实编辑的撤销点。
        self.push_history();

        // 主选择漂移防护：变速可越过未选中音符 → 当前轨重排会位移主选择索引
        let selection_identity = self.capture_selection_identity();

        // 冻结条目：被变速音符的**新位置**（视觉音轨 + 文档权威 tick）
        let mut frozen_entries: Vec<(u16, u32, u32, u8)> = Vec::new();
        let (modified_count, current_track_touched, current_track_ranges) = self
            .apply_speed_change_internal(
                track_indices,
                min_tick,
                speed_factor,
                &mut frozen_entries,
            );

        if modified_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        // 框选冻结（框选误伤修复）：变速改变了选择几何，旧矩形既框不住已缩放
        // 的音符、又会继续按区间命中区间内其他音符；选择集收敛为被变速音符的新位置。
        self.editor_state
            .data
            .arrange_selection
            .freeze(frozen_entries);

        self.remap_selection_by_identity(&selection_identity);

        if let Some(ranges) = &current_track_ranges {
            // 当前轨重排：按受影响闭区间增量更新（替代全量重建）
            self.editor_state.data.push_reorder_ranges_events(ranges);
        }
        if current_track_touched {
            self.mark_notes_changed();
        }
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(affected_tracks));
        // 2026-09 协作修复：广播变速结果给对端（B 端按同序先删后加，终态与 A 一致）。
        self.broadcast_pending_collab_transform_sync();
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
    /// 返回 (modified_count, current_track_touched, current_track_ranges)。
    ///
    /// `frozen_entries` 出参：被处理音符的**新位置** `(视觉音轨, start, end, key)`，
    /// 供调用方把选择集冻结为变速后的精确集合（见 `arrange_apply_speed_change`）。
    fn apply_speed_change_internal(
        &mut self,
        track_indices: HashMap<usize, Vec<usize>>,
        min_tick: f32,
        speed_factor: f32,
        frozen_entries: &mut Vec<(u16, u32, u32, u8)>,
    ) -> (usize, bool, Option<lumino_midi_model::SortedRestoreRanges>) {
        let current_track = self.editor_state.data.current_track;
        let mut current_track_touched = false;
        let mut current_track_ranges: Option<lumino_midi_model::SortedRestoreRanges> = None;
        let mut modified_count = 0usize;
        const MIN_LEN: f32 = 1.0;
        // 2026-09 协作修复：收集「旧→新」音符状态用于广播（避免与 notes 可变借用冲突，
        // 循环结束后再 push，跨音轨各自携带 track_idx）。
        let mut transitions: Vec<_> = Vec::new();

        // 2026-08 单一权威源：直接修改 document 各轨音符（track_notes_mut）
        for (track_idx, indices) in &track_indices {
            if *track_idx == current_track {
                current_track_touched = true;
            }
            let visual = self
                .editor_state
                .data
                .visual_position_of(*track_idx)
                .unwrap_or(*track_idx) as u16;
            if let Some(notes) = self
                .editor_state
                .data
                .document
                .as_mut()
                .and_then(|doc| doc.track_notes_mut(*track_idx))
            {
                let mut modified_indices: Vec<usize> = Vec::new();
                for &i in indices {
                    if let Some(note) = notes.get_mut(i) {
                        let old = *note;
                        let tick = note.start_tick as f32;
                        let length = (note.end_tick - note.start_tick) as f32;
                        let nt = min_tick + (tick - min_tick) * speed_factor;
                        let nl = (length * speed_factor).max(MIN_LEN);
                        if (nt - tick).abs() > f32::EPSILON || (nl - length).abs() > f32::EPSILON {
                            let new_start = nt.max(0.0);
                            note.start_tick = new_start as u32;
                            note.end_tick = note.start_tick + nl as u32;
                            transitions.push((old, *note, *track_idx));
                            modified_indices.push(i);
                            modified_count += 1;
                        }
                        // 冻结条目取**处理后**的值：未实际变更的音符也保留在选中集内
                        frozen_entries.push((visual, note.start_tick, note.end_tick, note.key));
                    }
                }
                // 子集变速可越过未选中音符的 tick → 恢复「按 start_tick 升序」不变式
                // （window_range/position_of_id 二分依赖，破坏后渲染/命中漏检音符）
                if let Some(ranges) = notes.restore_sorted_ranges(&modified_indices)
                    && *track_idx == current_track
                {
                    current_track_ranges = Some(ranges);
                }
            }
        }

        for (old, new, track) in transitions {
            self.editor_state
                .data
                .push_collab_transform_transition(old, new, track);
        }

        (modified_count, current_track_touched, current_track_ranges)
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
