//! 工程走带选中音符查询与批量操作
//!
//! 提供以下操作：
//! - `arrangement_selected_notes`: 获取选中音符列表（用于 ghost 预览）
//! - `arrange_select_all_notes`: 全选（全部音轨 × 全部 tick）
//! - `arrange_delete_selected_notes`: 删除选中音符
//! - `arrange_apply_speed_change`: 选中音符批量变速
//! - `collect_notes_for_export`: 导出用选区收集（两视图仲裁，见该方法文档）
//!
//! 2026-08 单一权威源：音符唯一权威是 document，本模块直接读写 MidiDocument，
//! 不再维护 track_notes 缓存。

use std::collections::HashMap;

use lumino_midi_loader::NoteEvent;

use super::Editor;

impl Editor {
    /// 工程走带全选：选中**全部音轨 × 全部 tick 区间**。
    ///
    /// 走带选区是跨轨矩形，故全选天然覆盖整张工程——与钢琴卷帘
    /// `select_all_notes`（仅当前轨全部音符）语义对齐：都是「当前视图内的一切」。
    ///
    /// 使用矩形模式（非冻结集）：全选是纯几何操作，无需精确成员集合，
    /// 且矩形模式在撤销后仍是活语义（见 `invalidate_arrange_selection_after_history`）。
    ///
    /// 返回是否建立了全选（无工程 / 无音轨时为 `false`）。
    pub fn arrange_select_all_notes(&mut self) -> bool {
        let data = &mut self.editor_state.data;
        let Some(doc) = data.document.as_ref() else {
            tracing::debug!("Arrangement: 全选 - 无工程文档");
            return false;
        };
        let track_count = doc.track_count();
        if track_count == 0 {
            tracing::debug!("Arrangement: 全选 - 工程无音轨");
            return false;
        }
        // tick 上界取工程实际最大终点；空工程兜底一个最小可见区间，
        // 否则 te <= ts 会让 add_rect_track 静默拒绝、用户看不到任何选区反馈
        let max_tick = doc.tracks_max_end_tick().max(1);
        data.arrange_selection.clear();
        data.arrange_selection.add_rect_track(
            0,
            max_tick,
            0,
            127,
            0,
            (track_count - 1).min(u16::MAX as usize) as u16,
        );
        tracing::info!(
            "Arrangement: 全选 {} 条音轨（tick 0..{max_tick}）",
            track_count
        );
        true
    }

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

    /// 收集「导出为素材」用的选中音符：按**视图优先序**取数，主选区空则回退另一套。
    ///
    /// 返回 `(文档音轨索引, 该轨选中音符)`，仅含命中音符的音轨。
    ///
    /// # P0 修复（视图仲裁缺失，此前是三处逻辑错误叠加）
    ///
    /// 旧实现在 `Host::get_selected_notes` 里，犯了两个错：
    /// 1. **优先序错误**：`if has_selection() { return; }` —— 卷帘选区非空即提前返回，
    ///    走带选区被完全忽略。而菜单项的启用条件是 `卷帘非空 || 走带非空`（OR），
    ///    于是**菜单能点、导出却是另一套选区**（用户框了 A，导出得到 B）。
    ///    两套选区（`selected_notes` / `arrange_selection`）彼此独立、互不清理，
    ///    从卷帘切到走带后卷帘选区仍留存，这个错误极易触发。
    /// 2. **坐标空间错误**：走带分支把**文档音轨索引**当**视觉音轨**传进
    ///    `ArrangeSelection::contains`。而选区（含冻结集）存的是视觉轨
    ///    （见 `move_notes::frozen_entries_of_moved` 用 `visual_position_of` 转换）。
    ///    `track_visual_order` 非恒等时（删轨 / 加轨 / 排序 / 分组显示）判定全错。
    ///    这与 2025-07 修过的 `arrangement-y-axis-movement` 是**同一个坑换个入口复现**。
    ///
    /// 现在：主选区 = 当前视图的选区（`prefer_arrangement` 决定），空则回退另一套，
    /// 与菜单启用条件的 OR 语义对齐——不会「能点却导不出」。
    pub fn collect_notes_for_export(
        &self,
        prefer_arrangement: bool,
    ) -> Vec<(usize, Vec<NoteEvent>)> {
        if prefer_arrangement {
            let arranged = self.collect_arrangement_selected_notes();
            if !arranged.is_empty() {
                return arranged;
            }
        }
        let roll = self.collect_roll_selected_notes();
        if !roll.is_empty() {
            return roll;
        }
        // 主选区（走带）为空且回退（卷帘）也为空时才走到这里；若调用方是
        // prefer_arrangement = false 且卷帘为空，仍需尝试走带（OR 语义）
        if !prefer_arrangement {
            self.collect_arrangement_selected_notes()
        } else {
            Vec::new()
        }
    }

    /// 走带选区命中的音符（跨轨，视觉空间判定）
    fn collect_arrangement_selected_notes(&self) -> Vec<(usize, Vec<NoteEvent>)> {
        let editor_data = &self.editor_state.data;
        let selection = &editor_data.arrange_selection;
        let mut result: Vec<(usize, Vec<NoteEvent>)> = Vec::new();
        if selection.is_empty() {
            return result;
        }
        let Some(doc) = &editor_data.document else {
            return result;
        };
        for track_idx in 0..doc.track_count() {
            // 关键：选区存的是**视觉轨**，必须经 visual_position_of 转换
            let visual = editor_data
                .visual_position_of(track_idx)
                .unwrap_or(track_idx);
            let notes = editor_data.track_notes(track_idx);
            let selected: Vec<NoteEvent> = notes
                .iter()
                .filter(|n| selection.contains(visual as u16, n.start_tick, n.key))
                .copied()
                .collect();
            if !selected.is_empty() {
                result.push((track_idx, selected));
            }
        }
        result
    }

    /// 卷帘选区命中的音符（单轨，索引位图）
    fn collect_roll_selected_notes(&self) -> Vec<(usize, Vec<NoteEvent>)> {
        let editor_data = &self.editor_state.data;
        if !self.has_selection() {
            return Vec::new();
        }
        let track = editor_data.current_track;
        let notes = editor_data.track_notes(track);
        let selected: Vec<NoteEvent> = self
            .get_selected_indices()
            .into_iter()
            .filter_map(|idx| notes.get(idx).copied())
            .collect();
        if selected.is_empty() {
            Vec::new()
        } else {
            vec![(track, selected)]
        }
    }
}
