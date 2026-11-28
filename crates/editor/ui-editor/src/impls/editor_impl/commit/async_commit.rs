//! 异步提交结果：轮询与排空。

use super::helpers::{
    capture_ghost_aware_identity, pending_drag_olds_of, pending_matches_interaction,
    remap_selection_by_ghost_identity, reselect_moved_news,
};
use crate::Editor;

impl Editor {
    /// 轮询异步提交结果
    ///
    /// 若完成：应用结果到 data，清空 pending_drag_state，并返回修改数。
    /// 若未完成：返回 `None`。
    ///
    /// 选中跟随条件分支（幽灵直到取消）：
    /// 保留旧框（interaction 选中值 == pending 原始值）时按新值重选；
    /// 已取消或新框时走通用重映射。全程窗口定位，无全扫。
    ///
    /// 飞行中新框选（空白点击提交后立即框选）按视觉（幽灵）位置构建，
    /// 重映射必须按幽灵值（旧值 + pending delta）在新文档中定位，否则框选丢失/错位。
    pub fn poll_async_commit(&mut self) -> Option<usize> {
        crate::puffin_profiler::poll_async_commit();
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let identity = self.capture_selection_identity();
        let ghost_identity = capture_ghost_aware_identity(self, max_key);
        let (olds, delta_tick, delta_key) = pending_drag_olds_of(self);
        let retained = pending_matches_interaction(&identity, &olds);
        match self.editor_state.data.poll_async_commit() {
            Some(Ok(modified)) => {
                if retained {
                    reselect_moved_news(self, &olds, delta_tick, delta_key, max_key);
                } else {
                    remap_selection_by_ghost_identity(self, &ghost_identity);
                }
                if modified > 0 {
                    self.mark_notes_changed();
                    tracing::info!("Editor: 异步提交完成 - 修改 {} 个音符", modified);
                }
                self.transient.pending_drag = None;
                Some(modified)
            }
            Some(Err(e)) => {
                tracing::error!("Editor: 异步提交结果处理失败: {}", e);
                self.transient.pending_drag = None;
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
        // DEBT-03 #120：等待上限——提交线程 panic/卡死时绝不无限 yield 自旋占核。
        // 取 30s：百万级工程的正常提交也可能耗秒级，不能被误杀；上限只用于兜底
        // "永不返回"的病态情况。
        const DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
        let deadline = std::time::Instant::now() + DRAIN_TIMEOUT;
        while self.editor_state.data.has_pending_commit() {
            // 同 poll_async_commit 的条件分支（保留旧框→新值重选，否则幽灵值重映射）。
            let identity = self.capture_selection_identity();
            let max_key_now = self.editor_state.view.visible_key_count.saturating_sub(1);
            let ghost_identity = capture_ghost_aware_identity(self, max_key_now);
            let (olds, delta_tick, delta_key) = pending_drag_olds_of(self);
            let retained = pending_matches_interaction(&identity, &olds);
            match self.editor_state.data.poll_async_commit() {
                Some(Ok(modified)) => {
                    if retained {
                        reselect_moved_news(
                            self,
                            &olds,
                            delta_tick,
                            delta_key,
                            self.editor_state.view.visible_key_count.saturating_sub(1),
                        );
                    } else {
                        remap_selection_by_ghost_identity(self, &ghost_identity);
                    }
                    if modified > 0 {
                        self.mark_notes_changed();
                        any_modified = true;
                    }
                    self.transient.pending_drag = None;
                }
                Some(Err(e)) => {
                    tracing::error!("Editor: drain 异步提交失败: {}", e);
                    self.transient.pending_drag = None;
                }
                None => {
                    if std::time::Instant::now() >= deadline {
                        // DEBT-03 #120：超时降级——丢弃 pending、标记刷新，绝不在 UI 线程
                        // 无限循环（worker panic/卡死时旧实现 100% 占核不可恢复）。
                        tracing::error!(
                            "Editor: 异步提交等待超时（>{:?}），丢弃 pending 并降级刷新",
                            DRAIN_TIMEOUT
                        );
                        self.editor_state.data.cancel_async_commit();
                        self.transient.pending_drag = None;
                        self.mark_notes_changed();
                        break;
                    }
                    // 避免忙等：让出时间片
                    std::thread::yield_now();
                }
            }
        }
        any_modified
    }
}
