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
        if self.editor_state.data.undo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
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
        if self.editor_state.data.redo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("Editor: Redo 成功");
            true
        } else {
            tracing::info!("Editor: 没有可重做的操作");
            false
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
