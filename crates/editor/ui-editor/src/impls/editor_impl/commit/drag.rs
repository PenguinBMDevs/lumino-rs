//! pending 批量拖动的提交（异步）。

use super::helpers::broadcast_selection_move;
use crate::Editor;

impl Editor {
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
        let Some(drag_state) = self.transient.pending_drag.as_ref() else {
            return false;
        };
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: pending drag delta 为零，跳过提交");
            self.transient.pending_drag = None;
            return false;
        }
        // 避免重复提交
        if self.editor_state.data.has_pending_commit() {
            return true;
        }

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // first-writer-wins 冲突判定：本地选择被更早的远端选择锁定时让行（远端优先），
        // 既不应用移动也不广播。
        if self.transient.local_selection_is_locked() {
            tracing::debug!("协作: 批量移动被远端抢先选择锁定，跳过（远端优先）");
            return false;
        }

        let ops = self
            .editor_state
            .data
            .move_ops_from_drag_state_with_max_key(drag_state, max_key);
        // 广播协作同步事件：批量移动（框选拖动）必须通知其他客户端，
        // 否则对端音符状态不会改变（B 端收不到任何更新）。
        // 与单音符拖动 `finalize_dragging` 共用同一套协作同步管线：
        // 逐音符发射 `LocalNoteMoved`，由 Runner 转换为 `NoteBatchOperation(Move)`
        // 并广播。此前该路径（commit_pending_drag → apply_move_ops_async）
        // 完全静默，正是「A 端移动框选音符，B 端不动」的根因。
        broadcast_selection_move(self, drag_state);
        match self.editor_state.data.apply_move_ops_async(ops, max_key) {
            Ok(true) => {
                tracing::info!("Editor: 已启动 pending 批量拖动异步提交");
                // 选择会话结束由调用方按场景发射（flush 取消时发 end，
                // 拉伸/保存等保留选区时发 active 更新），此处不预设。
                true
            }
            Ok(false) => {
                self.transient.pending_drag = None;
                false
            }
            Err(e) => {
                tracing::error!("Editor: 异步提交 MoveOp 失败: {}", e);
                self.transient.pending_drag = None;
                false
            }
        }
    }
}
