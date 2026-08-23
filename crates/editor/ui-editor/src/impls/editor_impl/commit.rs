//! Editor 编辑状态判定与提交（pending 拖动/复制、异步提交、当前编辑收尾）
//!
//! 从 `impls/editor_impl.rs` 抽出，控制文件行数并保持单一职责。

use crate::note::Note;
use crate::{EditState, Editor};
use lumino_editor_state::DragState;

impl Editor {
    /// Push current state to history
    pub fn push_history(&mut self) {
        self.editor_state.data.push_history();
    }

    /// 检查当前是否处于编辑状态（拦截 Undo/Redo/Save/Play/Export 用）
    ///
    /// 返回 `true` 当用户正在进行音符编辑（拖动/绘制/调整大小），
    /// 或有未提交的批量拖动/批量复制（pending_drag_state / pending_copy_drag_state），
    /// 或正在进行曲线路径编辑（锚点/控制柄拖动）。
    pub fn is_editing(&self) -> bool {
        self.pending_drag_state.is_some()
            || self.pending_copy_drag_state.is_some()
            || self.editor_state.data.has_pending_commit()
            || self.editor_state.line_tool.interaction
                != lumino_editor_state::LineToolInteraction::None
            || matches!(
                self.editor_state.interaction.edit_state,
                EditState::Dragging { .. }
                    | EditState::DraggingSelection { .. }
                    | EditState::DraggingSelectionCopy { .. }
                    | EditState::PendingDrag { .. }
                    | EditState::Drawing { .. }
                    | EditState::ResizingStart { .. }
                    | EditState::ResizingEnd { .. }
                    | EditState::ResizingSelectionStart { .. }
                    | EditState::ResizingSelectionEnd { .. }
            )
    }

    /// 是否有未提交的批量拖动（pending commit 状态，含批量复制）
    pub fn has_pending_drag(&self) -> bool {
        self.pending_drag_state.is_some()
            || self.pending_copy_drag_state.is_some()
            || self.editor_state.data.has_pending_commit()
    }

    /// 丢弃未提交的批量拖动/批量复制（不含异步提交中的 pending commit）
    ///
    /// 图片转 MIDI √ 写入后调用：写入改变了 document 音符数量与顺序，
    /// 残留的 `pending_drag_state.selected` / `pending_copy_drag_state.selected`
    /// 是写入前的全局索引，继续保留会导致后续 resize/拖动按旧索引取位、
    /// 误伤周围音符（连带改变长度）。
    pub fn clear_pending_drag(&mut self) {
        self.pending_drag_state = None;
        self.pending_copy_drag_state = None;
    }

    /// 提交 pending 批量拖动到 document（音符唯一权威）
    ///
    /// 在以下场景调用：
    /// - 用户点击空白处取消框选时
    /// - `commit_current_edit()` 自动提交（Save/Play/Export 前的 fallback）
    ///
    /// 返回 `true` 表示已启动异步提交。如果 pending_drag_state 为 None 或 delta 为零，
    /// 返回 false。
    ///
    /// **异步提交**：实际数据更新在后台线程执行，UI 层需每帧调用 `poll_async_commit`
    /// 获取结果。pending_drag_state 会保留到异步提交完成，以维持 ghost 视觉位置。
    pub fn commit_pending_drag(&mut self) -> bool {
        crate::puffin_profiler::commit_pending_drag();
        let Some(drag_state) = self.pending_drag_state.as_ref() else {
            return false;
        };
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: pending drag delta 为零，跳过提交");
            self.pending_drag_state = None;
            return false;
        }
        // 避免重复提交
        if self.editor_state.data.has_pending_commit() {
            return true;
        }

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // first-writer-wins 冲突判定：本地选择被更早的远端选择锁定时让行（远端优先），
        // 既不应用移动也不广播。
        if self.local_selection_is_locked() {
            tracing::debug!("协作: 批量移动被远端抢先选择锁定，跳过（远端优先）");
            return false;
        }

        let ops = self.editor_state.data.move_ops_from_drag_state(drag_state);
        // 广播协作同步事件：批量移动（框选拖动）必须通知其他客户端，
        // 否则对端音符状态不会改变（B 端收不到任何更新）。
        // 与单音符拖动 `finalize_dragging` 共用同一套协作同步管线：
        // 逐音符发射 `LocalNoteMoved`，由 Runner 转换为 `NoteBatchOperation(Move)`
        // 并广播。此前该路径（commit_pending_drag → apply_move_ops_async）
        // 完全静默，正是「A 端移动框选音符，B 端不动」的根因。
        self.broadcast_selection_move(drag_state);
        match self.editor_state.data.apply_move_ops_async(ops, max_key) {
            Ok(true) => {
                tracing::info!("Editor: 已启动 pending 批量拖动异步提交");
                // 编辑已提交：结束本地选择会话（通知对端）
                self.emit_local_selection_changed(false);
                true
            }
            Ok(false) => {
                self.pending_drag_state = None;
                false
            }
            Err(e) => {
                tracing::error!("Editor: 异步提交 MoveOp 失败: {}", e);
                self.pending_drag_state = None;
                false
            }
        }
    }

    /// 广播批量移动（框选拖动提交）到协作客户端。
    ///
    /// 批量移动与单音符拖动（`finalize_dragging`）共用同一套协作同步管线：
    /// 逐音符发射 `LocalNoteMoved` 同步事件，由 Runner 转换为
    /// `NoteBatchOperation(Move)` 并广播给其他客户端。`commit_pending_drag`
    /// 此前完全不广播，导致协作对端无法感知框选移动——表现为「A 端移动了，
    /// B 端音符状态不变」。本函数补全该缺失的广播，与移动提交一一对应。
    ///
    /// 每个被选中音符都发射一次，携带其**原始**位置（移动前 tick/key）与本次
    /// 拖动的统一偏移，对端据此匹配本地音符并叠加相同偏移完成同步。
    fn broadcast_selection_move(&self, drag_state: &DragState) {
        let track_index = self.editor_state.data.current_track;
        let tick_offset = drag_state.delta_tick as f32;
        let key_offset = drag_state.delta_key;
        for idx in drag_state.selected_indices() {
            // 2026-08 单一权威源：id 与原始位置取自 document 当前轨权威 NoteEvent
            let (id, tick, key, length) = {
                let notes = self.editor_state.data.track_notes(track_index);
                let Some(note) = notes.get(idx) else {
                    continue;
                };
                (
                    note.id,
                    note.start_tick as f32,
                    note.key as u16,
                    (note.end_tick - note.start_tick) as f32,
                )
            };
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_moved(
                    id,
                    tick,
                    key,
                    length,
                    tick_offset,
                    key_offset,
                    track_index,
                ),
            ));
        }
    }

    /// 提交 pending 批量复制到 document（音符唯一权威）
    ///
    /// 在以下场景调用：
    /// - 复制拖动松手时（`handle_released` 的 `DraggingSelectionCopy` 分支，松手即提交）
    /// - `flush_pending_drag`（点击空白处，兜底）
    /// - `commit_current_edit()` 自动提交（Save/Play/Export 前的 fallback）
    ///
    /// 复制模式：将选中音符按 `pending_copy_drag_state.delta` 偏移后
    /// `batch_insert_notes` 写入内存层，并**只选中新插入的副本**
    /// （最新件框选；原件不再保留框选状态）。返回 `true` 表示已写入。
    /// 如果 pending 为 None 或 delta 为零，返回 false。
    pub fn commit_pending_copy(&mut self) -> bool {
        crate::puffin_profiler::commit_pending_copy();
        let Some(drag_state) = self.pending_copy_drag_state.as_ref() else {
            return false;
        };
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: pending 复制 delta 为零，跳过提交");
            self.pending_copy_drag_state = None;
            return false;
        }

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // first-writer-wins 冲突判定：本地选择被更早的远端选择锁定时让行（远端优先）。
        if self.local_selection_is_locked() {
            tracing::debug!("协作: 批量复制被远端抢先选择锁定，跳过（远端优先）");
            return false;
        }

        // 构造副本音符列表（原始位置 + delta，tick/key clamp 到合法范围）
        let notes: Vec<Note> = drag_state
            .selected_indices_fast()
            .into_iter()
            .filter_map(|i| self.editor_state.data.get_note_view(i))
            .map(|n| {
                let tick = (n.tick + drag_state.delta_tick as f32).max(0.0);
                let key =
                    (n.key as i32 + drag_state.delta_key as i32).clamp(0, max_key as i32) as u16;
                Note::from_raw(tick, key, n.length, n.velocity, n.channel)
            })
            .collect();
        if notes.is_empty() {
            self.pending_copy_drag_state = None;
            return false;
        }

        // 与粘贴提交一致：push history → 批量归并（单次重建，峰值仅单块 8MB）
        self.push_history();
        let inserted = self.editor_state.data.batch_insert_notes(&notes);
        // 插入位移了既有音符索引，旧选中索引全部失效：清空后按参数全等
        // 重选「副本」（最新件框选；副本 tick 可能落在现有音符之间，索引散布
        // 而非连续追加，不能按 start..start+inserted 连续区间选中）。
        self.selection_clear();
        self.select_notes_by_params(&notes);
        self.mark_notes_changed();
        // 2026-09 协作修复：复制拖拽（生成副本）属「增音符」，须广播给对端，
        // 否则 B 端完全缺失被复制的副本。
        let track = self.editor_state.data.current_track;
        for n in &notes {
            // 复制副本已批量插入文档并分配真实 id，按位置反查取回后随事件发出。
            let id = self.editor_state.data.note_id_at(track, n.tick, n.key).unwrap_or(0);
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_added(
                    id, n.tick, n.key, n.length, n.velocity, n.channel, track,
                ),
            ));
        }
        self.pending_copy_drag_state = None;
        // 编辑已提交：结束本地选择会话（通知对端）
        self.emit_local_selection_changed(false);
        tracing::info!("Editor: 已复制 {} 个音符", inserted);
        true
    }

    /// 轮询异步提交结果
    ///
    /// 若完成：应用结果到 data，清空 pending_drag_state，并返回修改数。
    /// 若未完成：返回 `None`。
    pub fn poll_async_commit(&mut self) -> Option<usize> {
        crate::puffin_profiler::poll_async_commit();
        match self.editor_state.data.poll_async_commit() {
            Some(Ok(modified)) => {
                if modified > 0 {
                    self.mark_notes_changed();
                    tracing::info!("Editor: 异步提交完成 - 修改 {} 个音符", modified);
                }
                self.pending_drag_state = None;
                Some(modified)
            }
            Some(Err(e)) => {
                tracing::error!("Editor: 异步提交结果处理失败: {}", e);
                self.pending_drag_state = None;
                None
            }
            None => None,
        }
    }

    /// 阻塞等待所有异步提交完成
    ///
    /// 用于 Save/Play/Export 等需要立即可用数据的场景。
    /// 返回 `true` 表示有数据被修改。
    pub fn drain_async_commit(&mut self) -> bool {
        let mut any_modified = false;
        while self.editor_state.data.has_pending_commit() {
            match self.editor_state.data.poll_async_commit() {
                Some(Ok(modified)) => {
                    if modified > 0 {
                        self.mark_notes_changed();
                        any_modified = true;
                    }
                    self.pending_drag_state = None;
                }
                Some(Err(e)) => {
                    tracing::error!("Editor: drain 异步提交失败: {}", e);
                    self.pending_drag_state = None;
                }
                None => {
                    // 避免忙等：让出时间片
                    std::thread::yield_now();
                }
            }
        }
        any_modified
    }

    /// 提交当前编辑（Save/Play/Export 前自动调用）
    ///
    /// 如果用户正在编辑（ghost 拖动/绘制/调整大小），先提交到 document。
    /// 等价于"模拟用户松开鼠标"。返回 `true` 表示有数据被提交。
    ///
    /// **延迟提交方案**：`DraggingSelection` 的 `handle_released` 只把 delta 保存到
    /// `pending_drag_state`，不真正 apply。这里必须再调 `commit_pending_drag`，
    /// 否则 Save/Play/Export 时数据会丢失。`DraggingSelectionCopy` 同理
    /// （`pending_copy_drag_state` → `commit_pending_copy`）。
    ///
    /// **异步提交**：Save/Play/Export 前会调用 `drain_async_commit` 确保数据已落盘。
    pub fn commit_current_edit(&mut self) -> bool {
        if !self.is_editing() {
            return false;
        }
        let before = self.editor_state.data.current_track_note_count();
        // handle_released: Dragging/Drawing/Resizing 直接 apply；DraggingSelection 保存到 pending
        self.handle_released();
        // 延迟提交方案：如果 handle_released 产生了 pending_drag_state，启动异步提交
        let pending_committed = self.commit_pending_drag();
        // Save/Play/Export 前必须等待异步提交完成
        let drained = self.drain_async_commit();
        // 复制模式：未写入的副本在保存/播放/导出前必须写入内存层。
        // 必须在 drain 之后（异步提交整轨替换音符，先插入副本会被覆盖）
        let copy_committed = self.commit_pending_copy();
        let after = self.editor_state.data.current_track_note_count();
        tracing::debug!(
            "Editor: 自动提交编辑（commit_current_edit），notes len {} -> {}, pending_committed={}, copy_committed={}, drained={}",
            before,
            after,
            pending_committed,
            copy_committed,
            drained
        );
        true
    }
}
