//! 当前编辑收尾：按交互类型提交或落 pending。

use crate::Editor;

impl Editor {
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
        // （显式完成点：Save/Play/Export 必须落盘，选区保留）。
        let pending_committed = self.commit_pending_drag();
        // Save/Play/Export 前必须等待异步提交完成
        let drained = self.drain_async_commit();
        // 保留选区时远端高亮应跟随到新位置（提交后选中已按新值重选）。
        if (pending_committed || drained)
            && !self.editor_state.interaction.selected_notes.is_empty()
            && self.editor_state.data.collab_sync_enabled()
        {
            let fps = self.selected_fingerprints();
            self.transient.emit_local_selection_changed(true, &fps);
        }
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
