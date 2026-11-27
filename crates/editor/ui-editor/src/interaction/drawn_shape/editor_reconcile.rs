//! 删除与对账：把编辑器里选中的形状写回工程并清理失效项。
//!
//! 方法仍在 `impl Editor`，只是按职责放在子模块。

use super::*;

impl Editor {
    /// 删除**整个选中集**的绘制图形（图形对象 + 它们生成的音符），返回是否删除了内容
    ///
    /// 待确认镜像（未 √ 的几何）只摘几何：无音符 → 不动文档、不记文档历史。
    pub(crate) fn delete_selected_drawn_shape(&mut self) -> bool {
        // 取消任何未完成的拖拽：删除后拖拽若残留，会把 is_editing() 永久卡在 true
        // （进而阻塞撤销/重做）。删除是终结操作，拖拽状态必须一并收敛。
        let _ = self.editor_state.shape_select.end_drag();
        // 选中集快照（ID + 各自音符 + 待确认来源）
        let selected: Vec<(u64, Vec<ShapeNote>, Option<PendingShapeRef>)> = self
            .editor_state
            .shape_select
            .selected_shapes()
            .map(|s| (s.id, s.notes.clone(), s.pending))
            .collect();
        if selected.is_empty() {
            return false;
        }

        // ① 待确认镜像：几何从 owning tool 摘除（含镜像退场与同容器下标前移）
        let mut pending_deleted = false;
        for (_, _, pending) in &selected {
            if pending.is_some_and(|r| remove_pending(self, r)) {
                pending_deleted = true;
            }
        }
        let selected: Vec<(u64, Vec<ShapeNote>)> = selected
            .into_iter()
            .filter_map(|(id, notes, pending)| pending.is_none().then_some((id, notes)))
            .collect();
        if selected.is_empty() {
            return pending_deleted;
        }

        // ② 已确认图形：先按值定位待删索引（按轨分组；`used` 在整组范围内共享，
        //    避免重复认领同一事件）
        let mut per_track: HashMap<usize, (HashSet<usize>, Vec<usize>)> = HashMap::new();
        for (_, notes) in &selected {
            for n in notes {
                let lookup = lumino_editor_state::note_to_event(Note::new(n.tick, n.key, n.length));
                let entry = per_track.entry(n.track).or_default();
                let doc_notes = self.editor_state.data.track_notes(n.track);
                if let Some(idx) = doc_notes.position_of_unused(&lookup, &entry.0) {
                    entry.0.insert(idx);
                    entry.1.push(idx);
                }
            }
        }
        let has_notes = per_track.values().any(|(_, idx)| !idx.is_empty());
        if has_notes {
            // 历史快照必须在变更**之前**入栈（快照 = 变更前状态，撤销才能恢复）
            self.editor_state
                .data
                .push_history_with_op_kind(OpKind::NoteDelete);
        }
        let mut deleted = 0usize;
        for (track, (_, indices)) in &per_track {
            if indices.is_empty() {
                continue;
            }
            deleted += self.editor_state.data.remove_notes_merged(*track, indices);
        }
        let ids: Vec<u64> = selected.into_iter().map(|(id, _)| id).collect();
        if deleted == 0 {
            if has_notes {
                self.editor_state.data.discard_last_history();
            }
            // 音符已不存在（例如用户先手动删了）：仍然收掉图形对象本身
            for id in ids {
                self.editor_state.shape_select.mark_deleted(id);
            }
            return true;
        }
        for id in ids {
            self.editor_state.shape_select.mark_deleted(id);
        }
        self.mark_notes_changed();
        true
    }

    /// 撤销/重做后与文档对账：修正「已删除图形」的可见性
    ///
    /// 删除走 Snapshot 历史——`History::undo` 推入 redo 栈的是**当前状态快照**，
    /// 不携带分组 ID，因此无法用分组对称回放。改为按事实对账：
    /// 「该图形是否还有音符残留在文档里」——方向无关，天然对称。
    /// 只对曾被本功能删除过的图形生效（见 `ShapeSelectState::reconcile_alive`）。
    pub(crate) fn reconcile_drawn_shapes(&mut self) {
        const PROBE_LIMIT: usize = 8;
        let probes: Vec<(u64, bool)> = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .map(|s| {
                let alive = s.notes.iter().take(PROBE_LIMIT).any(|n| {
                    let lookup =
                        lumino_editor_state::note_to_event(Note::new(n.tick, n.key, n.length));
                    self.editor_state
                        .data
                        .track_notes(n.track)
                        .position_of(&lookup)
                        .is_some()
                });
                (s.id, alive)
            })
            .collect();
        self.editor_state.shape_select.reconcile_alive(&probes);
    }
}
