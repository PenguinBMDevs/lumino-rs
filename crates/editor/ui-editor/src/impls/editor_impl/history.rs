//! Editor 撤销/重做
//!
//! **路径历史优先**：曲线工具的路径编辑（创建/弯曲/锚点增删）有独立历史栈，
//! 优先撤销最近的路径编辑；无路径历史时才回退 document 音符历史。

use crate::Editor;

impl Editor {
    /// Undo the last action
    ///
    /// **拦截策略**：如果当前正在主动编辑（Dragging/Drawing/Resizing 等），拦截并返回 `false`。
    /// 若存在未完成的 pending 批量拖动（异步提交中），先阻塞等待其完成，再执行 undo，
    /// 否则 undo 会操作旧数据并提示"未退出编辑状态"。
    pub fn undo(&mut self) -> bool {
        // 先排空异步提交，避免 pending 状态阻塞 undo
        if self.has_pending_drag() {
            tracing::info!("Editor: Undo 前发现 pending 异步提交，先 drain");
            self.drain_async_commit();
        }
        if self.is_editing() {
            tracing::warn!("Editor: 拦截 Undo —— 当前正在编辑，请先完成当前编辑");
            return false;
        }
        // 曲线工具路径编辑历史优先（最近的曲线操作）
        if self.editor_state.line_tool.undo_path() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("Editor: 撤销曲线路径编辑");
            return true;
        }
        // 移动专路预判：Operation undo 后轨道为 originals（按值），
        // 通用旧值重映射必空（旧值已删），此处按目标值重选，无全扫。
        // 仅当撤销前确有选中时才重选；空选中保持空（不凭空建框）。
        // 非移动路径仍用通用守卫（值持久的插入/删除，旧值仍存在）。
        let selection_identity = self.capture_selection_identity();
        let had_selection = selection_identity.entries().is_some_and(|e| !e.is_empty());
        let move_targets: Option<Vec<lumino_midi_loader::NoteEvent>> = if had_selection {
            match self.editor_state.data.history.undo_back() {
                Some(lumino_note_core::history::HistoryEntry::Operation(entry)) => Some(
                    entry
                        .ops
                        .iter()
                        .flat_map(|op| op.originals.clone())
                        .collect(),
                ),
                _ => None,
            }
        } else {
            None
        };
        if self.editor_state.data.undo() {
            if let Some(targets) = move_targets {
                self.selection_clear();
                for ev in &targets {
                    if let Some(idx) = self
                        .editor_state
                        .data
                        .track_notes(self.editor_state.data.current_track)
                        .position_of(ev)
                    {
                        // 仅当目标仍在当前轨（跨轨移动 target 轨不同则跳过，避免误选）。
                        self.selection_insert(idx);
                    }
                }
            } else {
                self.remap_selection_by_identity(&selection_identity);
            }
            self.grid_cache.clear();
            self.mark_notes_changed();
            self.broadcast_pending_collab_sync();
            self.broadcast_pending_collab_create_sync();
            self.broadcast_pending_collab_transform_sync();
            tracing::info!("Editor: Undo 成功");
            true
        } else {
            tracing::info!("Editor: 没有可撤销的操作");
            false
        }
    }

    /// Redo the last undone action
    ///
    /// **拦截策略**：同 `undo()`，编辑中拦截；存在 pending 时先 drain。
    /// **路径历史优先**：先重做最近的路径编辑，无则回退 document 历史。
    pub fn redo(&mut self) -> bool {
        if self.has_pending_drag() {
            tracing::info!("Editor: Redo 前发现 pending 异步提交，先 drain");
            self.drain_async_commit();
        }
        if self.is_editing() {
            tracing::warn!("Editor: 拦截 Redo —— 当前正在编辑，请先完成当前编辑");
            return false;
        }
        // 曲线工具路径编辑历史优先
        if self.editor_state.line_tool.redo_path() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("Editor: 重做曲线路径编辑");
            return true;
        }
        // 移动专路预判：Operation redo 后轨道为 moved（originals + delta），
        // 按目标新值重选。仅当重做前确有选中时才重选；空选中保持空。
        let selection_identity = self.capture_selection_identity();
        let had_selection = selection_identity.entries().is_some_and(|e| !e.is_empty());
        let move_targets: Option<Vec<lumino_midi_loader::NoteEvent>> = if had_selection {
            match self.editor_state.data.history.redo_back() {
                Some(lumino_note_core::history::HistoryEntry::Operation(entry)) => {
                    // 与回放侧一致取存量 moved（创建时 clamp 已固化，不再重算）。
                    const HIST_MAX_KEY: u16 = 255;
                    Some(
                        entry
                            .ops
                            .iter()
                            .flat_map(|op| op.moved_notes(HIST_MAX_KEY))
                            .collect(),
                    )
                }
                _ => None,
            }
        } else {
            None
        };
        if self.editor_state.data.redo() {
            if let Some(targets) = move_targets {
                self.selection_clear();
                for ev in &targets {
                    if let Some(idx) = self
                        .editor_state
                        .data
                        .track_notes(self.editor_state.data.current_track)
                        .position_of(ev)
                    {
                        self.selection_insert(idx);
                    }
                }
            } else {
                self.remap_selection_by_identity(&selection_identity);
            }
            self.grid_cache.clear();
            self.mark_notes_changed();
            self.broadcast_pending_collab_sync();
            self.broadcast_pending_collab_create_sync();
            self.broadcast_pending_collab_transform_sync();
            tracing::info!("Editor: Redo 成功");
            true
        } else {
            tracing::info!("Editor: 没有可重做的操作");
            false
        }
    }

    /// 把 `EditorData` 中累积的「撤销/重做音符移动」广播给协作对端。
    ///
    /// 撤销/重做 MoveOp 直接改本地 document 但不经拖动管线，因此不会自动发射
    /// `LocalNoteMoved`；若不在此补广播，B 端在 A 撤销后本地坐标与 A 端失同步，
    /// 下一次操作按 A 端本地坐标引用会在 B 端 0/N 失配。本方法 drain
    /// `pending_collab_move_sync` 并以与拖动一致的语义发射 `LocalNoteMoved`。
    fn broadcast_pending_collab_sync(&mut self) {
        let pending = self.editor_state.data.take_pending_collab_move_sync();
        if pending.is_empty() {
            return;
        }
        for (tick, key, tick_offset, key_offset, track_index) in pending {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_moved(
                    tick,
                    key,
                    0.0,
                    tick_offset,
                    key_offset,
                    track_index,
                ),
            ));
        }
    }

    /// 把 `EditorData` 中累积的「撤销/重做音符创建/删除」广播给协作对端。
    ///
    /// 撤销创建（`inverse=true`）本地删除被创建音符，但不经绘制/删除管线，
    /// 不会自动发射 `LocalNoteDeleted`，导致 B 端残留该音符（本次修复的缺陷）。
    /// 重做创建（`inverse=false`）本地重新插入，需补发射 `LocalNoteAdded`。
    /// 本方法 drain `pending_collab_create_sync` 并按 `is_added` 发射对应同步事件。
    fn broadcast_pending_collab_create_sync(&mut self) {
        let pending = self.editor_state.data.take_pending_collab_create_sync();
        if pending.is_empty() {
            return;
        }
        for (tick, key, length, velocity, channel, track_index, is_added) in pending {
            let event = if is_added {
                lumino_message::events::window::Event::local_note_added(
                    tick,
                    key,
                    length,
                    velocity,
                    channel,
                    track_index,
                )
            } else {
                lumino_message::events::window::Event::local_note_deleted(
                    tick,
                    key,
                    length,
                    velocity,
                    channel,
                    track_index,
                )
            };
            lumino_message::events::emit(lumino_message::events::Event::Window(event));
        }
    }

    /// 把 `EditorData` 中累积的「变换类操作（移调/翻转/变速/批量编辑）」广播给协作对端。
    ///
    /// 这些操作直接改 document 且走整轨快照历史，前向应用与 undo/redo 回放均不经
    /// 拖动/绘制/删除管线，因此不会自动发射同步事件——导致 B 端在 A 用变速等工具后
    /// 永久失同步（用户报告）。`pending_collab_transform_sync` 以「删除旧 + 添加新」入队，
    /// 本方法 drain 并按 **先删后加** 顺序发射 `LocalNoteDeleted` / `LocalNoteAdded`，
    /// 复用已修复通道（覆盖全部字段），使 B 端终态与 A 完全一致。
    pub fn broadcast_pending_collab_transform_sync(&mut self) {
        let pending = self.editor_state.data.take_pending_collab_transform_sync();
        if pending.is_empty() {
            return;
        }
        // 先发射全部删除，再发射全部添加：避免「添加落在尚未删除的旧音符位置上」
        // 造成瞬时重复（同位置出现两个音符）。按值引用，操作者标识由信封承载。
        let mut deletes: Vec<(f32, u16, f32, u8, u8, usize)> = Vec::new();
        let mut adds: Vec<(f32, u16, f32, u8, u8, usize)> = Vec::new();
        for (is_add, tick, key, length, velocity, channel, track_index) in pending {
            if is_add {
                adds.push((tick, key, length, velocity, channel, track_index));
            } else {
                deletes.push((tick, key, length, velocity, channel, track_index));
            }
        }
        for (tick, key, length, velocity, channel, track_index) in deletes {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_deleted(
                    tick,
                    key,
                    length,
                    velocity,
                    channel,
                    track_index,
                ),
            ));
        }
        for (tick, key, length, velocity, channel, track_index) in adds {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_added(
                    tick,
                    key,
                    length,
                    velocity,
                    channel,
                    track_index,
                ),
            ));
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.editor_state.line_tool.can_undo_path() || self.editor_state.data.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.editor_state.data.history.can_redo()
    }
}
