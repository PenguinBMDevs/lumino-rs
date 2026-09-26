//! 远程音符批量操作（新增/更新/删除/移动，按值，无全扫）
//!
//! **本地编辑临界区串行化**：本地拖动/待提交/异步提交期间，同轨远端结构编辑
//! 会漂移本地索引引用（`DragState.selected` / `note_index`），导致拖动/提交
//! 引用错误音符。此类操作入队 `Root::deferred_remote_ops`，待临界区结束后
//! 按到达顺序补放（`drain_deferred_remote_ops`）。
//!
//! 去 ID 后远端定位一律窗口二分（`position_of` / `window_range` + 同 tick 段扫描），
//! 禁止任何全轨 `iter().position` / 全量 id 表构建（即使一次也不行）。
//! 操作者标识由信封 `user_id + timestamp` 承载（见 `NoteBatchOperation.timestamp`），
//! 音符身份即其音乐内容。

use crate::root::Root;

impl Root {
    /// 远端音符操作涉及的音轨集合（含 source/target 轨）
    fn remote_op_affected_tracks(
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) -> Vec<usize> {
        let mut tracks: Vec<usize> = operation.notes.iter().map(|n| n.track_index).collect();
        if let Some(s) = operation.source_track {
            tracks.push(s);
        }
        if let Some(t) = operation.target_track {
            tracks.push(t);
        }
        tracks
    }

    /// 应用远程笔记操作到本地编辑器
    ///
    /// 本地编辑临界区（拖动/待提交/异步提交）期间，同轨远端结构编辑入队延迟；
    /// 其余情况先补放积压、再应用当前操作，保证到达顺序。
    pub fn apply_remote_note_operation(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        let affected = Self::remote_op_affected_tracks(operation);
        if self.editor.should_defer_remote_note_ops(&affected) {
            tracing::info!(
                "协作: 本地编辑进行中，远端音符操作延迟（涉及音轨 {:?}）",
                affected
            );
            self.deferred_remote_ops.push(operation.clone());
            return;
        }
        // 顺序保证：临界区已结束则先补放积压，再应用当前操作
        self.drain_deferred_remote_ops();
        if self.editor.should_defer_remote_note_ops(&affected) {
            // 补放触发了待提交手势的自动提交 → 重新进入临界区，当前操作继续排队
            self.deferred_remote_ops.push(operation.clone());
            return;
        }
        self.apply_remote_note_operation_now(operation);
    }

    /// 补放延迟的远端音符操作（每帧调用，见 `state_update`）。
    ///
    /// 状态机（保证与本地编辑临界区串行化 + 幽灵直到取消）：
    /// 1. 异步提交进行中 → 等待（其整轨写回会覆盖临界区内的变更）；
    /// 2. 已松手的待提交拖动/复制未取消（仍保留框选）→ 等待取消
    ///    （blank-click `flush_pending_drag` 统一提交；此处不代提交，
    ///    否则音符数据在取消框选前落盘，违背幽灵语义）；
    /// 3. 仍有活跃手势（拖动/绘制/调整/曲线编辑）→ 等待松手；
    /// 4. 空闲（无待提交、无活跃手势）→ 按到达顺序补放全部积压操作。
    pub(crate) fn drain_deferred_remote_ops(&mut self) {
        if self.deferred_remote_ops.is_empty() {
            return;
        }
        if self.editor.editor_state.data.has_pending_commit() {
            return;
        }
        if self.editor.has_uncommitted_drag()
            || self.editor.has_uncommitted_copy()
            || self.editor.is_editing()
        {
            // 待提交未取消或活跃手势中：等待取消/松手，不代提交本地拖动
            return;
        }
        let ops = std::mem::take(&mut self.deferred_remote_ops);
        tracing::info!("协作: 补放 {} 个延迟的远端音符操作", ops.len());
        for op in &ops {
            self.apply_remote_note_operation_now(op);
        }
    }

    /// 立即应用远程笔记操作（不做临界区判定）
    fn apply_remote_note_operation_now(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        use lumino_collaboration::types::NoteAction;

        // 协作音轨对齐：远端操作引用的音轨索引（含移动/复制的源轨与目标轨）
        // 若本地尚不存在，先补齐对应音轨，避免两方音轨数量不一致时音符错位。
        for t in Self::remote_op_affected_tracks(operation) {
            self.ensure_collab_track(t);
        }

        // 主选择漂移防护：远端结构编辑（增/删/移）会位移当前轨索引，
        // 先捕获选中音符值快照，应用后按值重映射（P3 临界区保护只覆盖手势期间）。
        // Move 专路：被移动的选中音符旧值已不存在，通用重映射必空，
        // 此处额外按新值（ref+off）重选，无全扫.
        let selection_identity = self.editor.capture_selection_identity();
        // 预计算 Move 新值目标（供应用后重选被移动的选中音符）。
        let move_targets: Option<Vec<lumino_midi_loader::NoteEvent>> =
            if operation.action == NoteAction::Move {
                let tick_off = operation.tick_offset.unwrap_or(0.0);
                let key_off = operation.key_offset.unwrap_or(0);
                // 仅为当前轨且被选中的旧值计算新值（窗口定位用全值，需长度/力度/通道）。
                // 长度/力度/通道取自本地旧值快照（移动不改这些），tick/key 按旧值+off
                // （与远端 ref+off 等价，本地旧值即 ref，省去 ref 匹配浮点误差）。
                // 匹配条件：旧值 (tick,key) 等于任一 incoming ref (tick,key) 且同轨。
                let current_track = self.editor.editor_state.data.current_track;
                let mut targets = Vec::new();
                if let Some(entries) = selection_identity.entries_for_move() {
                    for old in entries {
                        let mut matched = false;
                        for ref_note in &operation.notes {
                            if ref_note.track_index != current_track {
                                continue;
                            }
                            let ref_tick = lumino_editor_state::f32_to_tick(ref_note.tick);
                            let ref_key = ref_note.key.min(255) as u8;
                            if old.start_tick == ref_tick && old.key == ref_key {
                                matched = true;
                                break;
                            }
                        }
                        if !matched {
                            continue;
                        }
                        let new_tick = (old.start_tick as f32 + tick_off).max(0.0).round() as u32;
                        let new_key = (old.key as i16 + key_off).max(0) as u8;
                        let mut news = *old;
                        let len = old.end_tick.saturating_sub(old.start_tick).max(1);
                        news.start_tick = new_tick;
                        news.end_tick = new_tick.saturating_add(len);
                        news.key = new_key;
                        targets.push(news);
                    }
                }
                Some(targets)
            } else {
                None
            };

        match operation.action {
            NoteAction::Add => self.handle_remote_notes_add(operation),
            NoteAction::Update => self.handle_remote_notes_update(operation),
            NoteAction::Delete => self.handle_remote_notes_delete(operation),
            NoteAction::Move => self.handle_remote_notes_move(operation),
            _ => {
                tracing::debug!("协作: 未处理的笔记操作类型: {:?}", operation.action);
            }
        }

        self.editor.remap_selection_by_identity(&selection_identity);
        // Move 专路：通用重映射已保留未移动选中，此处补选被移动的新值。
        if let Some(targets) = move_targets {
            for ev in &targets {
                if let Some(idx) = self
                    .editor
                    .editor_state
                    .data
                    .track_notes(self.editor.editor_state.data.current_track)
                    .position_of(ev)
                {
                    self.editor.selection_insert(idx);
                }
            }
        }

        // 标记音符已变化，重建当前音轨的空间索引
        self.editor.mark_notes_changed();
    }

    fn handle_remote_notes_add(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        use std::collections::{HashMap, HashSet};

        // 按音轨分组批量插入（O(N+M) 单次归并）。
        // 去重按值窗口定位（`position_of`），禁止全轨 id 表构建全扫。
        let mut by_track: HashMap<usize, Vec<crate::editor::note::Note>> = HashMap::new();
        // 同批次内去重（重传同一值多次）：值键为整数元组（tick_bits,key,len_bits,vel,chan）。
        let mut seen_in_batch: HashSet<(usize, u32, u8, u32, u8, u8)> = HashSet::new();
        for note in &operation.notes {
            let start = lumino_editor_state::f32_to_tick(note.tick);
            let end = start.saturating_add(lumino_editor_state::f32_to_tick(note.length));
            let key = note.key.min(255) as u8;
            let seen_key = (
                note.track_index,
                start,
                key,
                end,
                note.velocity,
                note.channel,
            );
            if !seen_in_batch.insert(seen_key) {
                continue;
            }
            // 本地已存在同值 → 跳过（重传幂等，窗口定位，无全扫）。
            let exists = {
                let target = lumino_midi_loader::NoteEvent::new(
                    start,
                    end,
                    key,
                    note.velocity,
                    note.channel,
                );
                self.editor
                    .editor_state
                    .data
                    .track_notes(note.track_index)
                    .position_of(&target)
                    .is_some()
            };
            if exists {
                continue;
            }
            let editor_note = crate::editor::note::Note::from_raw(
                note.tick,
                note.key,
                note.length,
                note.velocity,
                note.channel,
            );
            by_track
                .entry(note.track_index)
                .or_default()
                .push(editor_note);
        }
        // 分轨批量写入（复用 EditorData 的批量接口，自动处理 dirty/增量）
        for (track_idx, notes) in by_track {
            let _ = self
                .editor
                .editor_state
                .data
                .batch_insert_notes_to_track_with_ids(track_idx, &notes);
        }
        // 精确标记受影响音轨（洋葱皮事件级增量）
        let affected: HashSet<usize> = operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        tracing::info!(
            "协作: 已添加 {} 个远程音符（批量，按值）",
            operation.notes.len()
        );
    }

    fn handle_remote_notes_update(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        // 批量更新（长度变更）：按 (tick,key) 窗口定位目标（无全扫），
        // 保持其他字段不变，仅更新长度。`update_note` 按索引删加，保持有序。
        for note in &operation.notes {
            let track_idx = note.track_index;
            let tick_u32 = lumino_editor_state::f32_to_tick(note.tick);
            let key_u8 = note.key.min(255) as u8;
            // 窗口：[tick, tick+1) 同 tick 段扫 key（O(log N + 段长)，无全扫）。
            let (lo, hi) = {
                let track = self.editor.editor_state.data.track_notes(track_idx);
                track.window_range(tick_u32, tick_u32.saturating_add(1), 0)
            };
            let mut match_idx: Option<usize> = None;
            {
                let track = self.editor.editor_state.data.track_notes(track_idx);
                for (idx, ev) in track.iter_window(lo, hi) {
                    if ev.start_tick == tick_u32 && ev.key == key_u8 {
                        match_idx = Some(idx);
                        break;
                    }
                }
            }
            let Some(idx) = match_idx else {
                continue;
            };
            let notes = self.editor.editor_state.data.track_notes(track_idx);
            let current = notes[idx];
            self.editor.editor_state.data.update_note(
                track_idx,
                idx,
                crate::editor::note::Note::from_raw(
                    current.start_tick as f32,
                    current.key as u16,
                    note.length,
                    current.velocity,
                    current.channel,
                ),
            );
        }
        // 精确标记受影响音轨（洋葱皮事件级增量）
        let affected: std::collections::HashSet<usize> =
            operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        tracing::info!(
            "协作: 已更新 {} 个远程音符（批量窗口定位，按值）",
            operation.notes.len()
        );
    }

    fn handle_remote_notes_delete(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        // 批量删除（按值）：每值窗口定位（`position_of`，无全扫），
        // 收集索引降序批量删除（`remove_note_ranges` 单次重建）。
        use std::collections::{HashMap, HashSet};
        let mut idx_by_track: HashMap<usize, Vec<usize>> = HashMap::new();
        for n in &operation.notes {
            let start = lumino_editor_state::f32_to_tick(n.tick);
            let len = lumino_editor_state::f32_to_tick(n.length);
            let end = start.saturating_add(len.max(1));
            let target = lumino_midi_loader::NoteEvent::new(
                start,
                end,
                n.key.min(255) as u8,
                n.velocity,
                n.channel,
            );
            // 长度可能因四舍五入差 1：先全值匹配，未命中回退 (tick,key) 窗口首个。
            let track_idx = n.track_index;
            let found = self
                .editor
                .editor_state
                .data
                .track_notes(track_idx)
                .position_of(&target)
                .or_else(|| {
                    let tick_u32 = start;
                    let key_u8 = n.key.min(255) as u8;
                    let track = self.editor.editor_state.data.track_notes(track_idx);
                    let (lo, hi) = track.window_range(tick_u32, tick_u32.saturating_add(1), 0);
                    for (idx, ev) in track.iter_window(lo, hi) {
                        if ev.start_tick == tick_u32 && ev.key == key_u8 {
                            return Some(idx);
                        }
                    }
                    None
                });
            if let Some(idx) = found {
                idx_by_track.entry(track_idx).or_default().push(idx);
            }
        }
        for (track_idx, mut idxs) in idx_by_track {
            // 降序 + 合并连续区间 → 单次 `remove_note_ranges`（无逐条全扫）。
            idxs.sort_unstable_by(|a, b| b.cmp(a));
            idxs.dedup();
            // 合并连续降序段为 (start,count)
            let mut ranges: Vec<(usize, usize)> = Vec::new();
            let mut i = 0;
            while i < idxs.len() {
                let seg_max = idxs[i];
                let mut seg_min = seg_max;
                while i + 1 < idxs.len() && idxs[i + 1] + 1 == seg_min {
                    seg_min -= 1;
                    i += 1;
                }
                ranges.push((seg_min, seg_max - seg_min + 1));
                i += 1;
            }
            // `remove_note_ranges` 要求降序；ranges 已按 seg_min 降序（因 idxs 降序）。
            let _ = self
                .editor
                .editor_state
                .data
                .document
                .as_mut()
                .map(|doc| doc.remove_note_ranges(track_idx, &ranges))
                .unwrap_or(0);
            self.editor
                .editor_state
                .data
                .mark_track_notes_changed_for(Some(HashSet::from([track_idx])));
        }
        // 精确标记受影响音轨
        let affected: HashSet<usize> = operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        tracing::info!(
            "协作: 已删除 {} 个远程音符（批量，按值）",
            operation.notes.len()
        );
    }

    fn handle_remote_notes_move(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        let tick_offset = operation.tick_offset.unwrap_or(0.0);
        let key_offset = operation.key_offset.unwrap_or(0);
        tracing::debug!(
            "协作: Move 操作 - tick_offset={}, key_offset={}, notes数量={}, source_track={:?}（按值）",
            tick_offset,
            key_offset,
            operation.notes.len(),
            operation.source_track
        );
        let mut matched_count = 0;
        // 逐值窗口定位 ref（tick,key），叠加偏移后 `update_note`（删加，保持有序）。
        // 每次重查（update 会重排），仍 O(1) 窗口 + 一次定位，无全扫。
        for note in &operation.notes {
            let track_idx = note.track_index;
            let tick_u32 = lumino_editor_state::f32_to_tick(note.tick);
            let key_u8 = note.key.min(255) as u8;
            let (lo, hi) = {
                let track = self.editor.editor_state.data.track_notes(track_idx);
                track.window_range(tick_u32, tick_u32.saturating_add(1), 0)
            };
            let mut match_idx: Option<usize> = None;
            {
                let track = self.editor.editor_state.data.track_notes(track_idx);
                for (idx, ev) in track.iter_window(lo, hi) {
                    if ev.start_tick == tick_u32 && ev.key == key_u8 {
                        match_idx = Some(idx);
                        break;
                    }
                }
            }
            let Some(idx) = match_idx else {
                tracing::warn!("协作: track {} 音符未按值匹配", track_idx);
                continue;
            };
            let notes = self.editor.editor_state.data.track_notes(track_idx);
            let current = notes[idx];
            let new_tick = current.start_tick as f32 + tick_offset;
            let new_key = (current.key as i16 + key_offset).max(0) as u16;
            self.editor.editor_state.data.update_note(
                track_idx,
                idx,
                crate::editor::note::Note::from_raw(
                    new_tick,
                    new_key,
                    current.length() as f32,
                    current.velocity,
                    current.channel,
                ),
            );
            matched_count += 1;
        }
        // 精确标记受影响音轨（洋葱皮事件级增量）
        let affected: std::collections::HashSet<usize> =
            operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        // 音符由 wgpu 渲染，不需要清 grid cache
        tracing::info!(
            "协作: Move 完成 - 匹配 {}/{} 个音符, current_track={}（按值）",
            matched_count,
            operation.notes.len(),
            self.editor.editor_state.data.current_track
        );
    }
}
