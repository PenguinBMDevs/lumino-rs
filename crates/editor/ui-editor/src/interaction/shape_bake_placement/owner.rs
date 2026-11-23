//! DRAW-049 阶段 2：Bake 放置态的**归属轨**判定。
//!
//! 放置会话只对 `ComposedKind::Single` 的单支 pending 生效；归属轨取模型
//! 自带的 `track`。所有「进入放置态 / ✓ 提交 / 撤销后自动布防手柄」的入口
//! 都必须先核对当前轨：切轨后会话已收口，残留的 pending 仍属于旧轨，
//! 此时按 ✓ 会把旧轨模型写进新轨（DRAW-049 复核指出的阻塞缺陷）。
//!
//! 只认 `Single`：`Insert`（形状抽屉）与 `Live`/`Join`/`Slice` 等组合各有
//! 自己的提交语义，不能被归属轨拦截，否则就是功能回归。

use lumino_editor_state::{ComposedKind, PendingComposed};

use crate::Editor;

impl Editor {
    /// 当前 pending 中唯一 Single 单支的归属轨；不满足返回 `None`。
    pub(crate) fn pending_single_owner_track(&self) -> Option<usize> {
        if self.editor_state.pending_composed.len() != 1 {
            return None;
        }
        let composed = self.editor_state.pending_composed.first()?;
        Self::single_owner_track(composed)
    }

    /// 当前放置会话的归属轨；会话 / 下标 / 形态失效返回 `None`。
    pub(crate) fn bake_session_owner_track(&self) -> Option<usize> {
        let session = self.bake_placement.as_ref()?;
        let composed = self
            .editor_state
            .pending_composed
            .get(session.pending_index)?;
        Self::single_owner_track(composed)
    }

    /// 是否存在「归属在别的轨」的待确认单支——非归属轨按 ✓ 时用来挡住提交。
    ///
    /// 会话优先（放置态自身的归属），无会话时回退到 pending：切轨已收口会话，
    /// 但 pending 还在，此时非归属轨的 ✓ 必须 no-op，绝不能落到
    /// `confirm_shape_tool()` 的「按当前轨写入」路径。
    pub(crate) fn bake_owned_elsewhere(&self) -> bool {
        self.bake_session_owner_track()
            .or_else(|| self.pending_single_owner_track())
            .is_some_and(|owner| owner != self.current_track())
    }

    /// 单支组合的归属轨；类型或支数不符返回 `None`。
    fn single_owner_track(composed: &PendingComposed) -> Option<usize> {
        if composed.kind != ComposedKind::Single || composed.results.len() != 1 {
            return None;
        }
        composed
            .results
            .first()
            .map(|part| part.model.track as usize)
    }
}
