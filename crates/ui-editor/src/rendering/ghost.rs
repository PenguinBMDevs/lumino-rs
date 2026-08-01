//! Ghost 拖动偏移计算辅助函数
//!
//! 拆分原因：rendering.rs 接近 400 行限制，按职责拆分。
//!
//! 这些函数在 hot loop 中被调用（每帧可达百万次），绝对不要在此处添加
//! puffin::profile_scope! 等 per-element 开销。puffin scope 应放在外层的
//! 循环函数（collect_visible_note_data 等）中。

use crate::EditState;

/// 计算音符 i 在当前编辑状态下的 ghost 偏移量
///
/// 合并规则（延迟提交方案）：
/// - 存在 `pending_drag_state` 且音符在 pending 选中集合中：返回 `pending.delta`
///   （pending 代表已启动异步提交但尚未完成的数据更新，在异步完成前始终可见）
/// - `Dragging`：额外加上 `drag_state.delta`（仅对 drag_state.selected 中的音符）
/// - `DraggingSelection`：额外加上 `drag_state.delta`，即 `pending.delta + drag_state.delta`
/// - 未命中任何选中集合：返回 `None`
///
/// **关键修复 1**：原实现中 `pending_drag_state` 在 `DraggingSelection` 期间覆盖了
/// 当前 `drag_state` 的渲染，导致第二次拖动时 ghost 位置不随鼠标移动。
/// 现在合并两个 delta，确保拖动期间视觉反馈正确。
///
/// **关键修复 2**：原实现只在 `Idle` / `DraggingSelection` 应用 pending delta，
/// 导致用户点击空白处开始新框选（`Selecting`）时，异步提交尚未完成就回撤。
/// 现在 pending delta 在异步完成前对所有状态都生效。
pub(crate) fn ghost_delta_for_index(
    i: usize,
    pending: &Option<lumino_editor_state::DragState>,
    edit_state: &EditState,
) -> Option<(i64, i16)> {
    // 注意：此函数在 hot loop 中被调用（每帧可达百万次），
    // 绝对不要在此处添加 puffin::profile_scope! 等 per-element 开销。
    // puffin scope 应放在外层的循环函数（collect_visible_note_data 等）中。
    let mut delta_tick = 0i64;
    let mut delta_key = 0i16;
    let mut has_delta = false;

    // 存在 pending 拖动且音符在 pending 选中集合中时，pending delta 生效。
    // 注意：pending 在异步提交完成前一直保留，因此不能限定为 Idle/DraggingSelection，
    // 否则用户点击空白处开始新框选（Selecting）时，已移动的音符会回撤。
    if let Some(pending) = pending
        && i < pending.selected.len()
        && pending.selected[i]
    {
        delta_tick = delta_tick.saturating_add(pending.delta_tick);
        delta_key = delta_key.saturating_add(pending.delta_key);
        has_delta = true;
    }

    // Dragging 或 DraggingSelection 时，当前 drag_state delta 生效
    match edit_state {
        EditState::Dragging { drag_state, .. } | EditState::DraggingSelection { drag_state }
            if i < drag_state.selected.len() && drag_state.selected[i] =>
        {
            delta_tick = delta_tick.saturating_add(drag_state.delta_tick);
            delta_key = delta_key.saturating_add(drag_state.delta_key);
            has_delta = true;
        }
        _ => {}
    }

    has_delta.then_some((delta_tick, delta_key))
}

/// 检查是否存在需要 ghost delta 的活跃状态
///
/// 检查 `pending_drag_state`、单音符 `Dragging` 和批量拖动 `DraggingSelection`。
/// `DraggingSelection` 也纳入检查，确保第二次拖动（已有 pending 时）当前 drag delta
/// 能正确渲染，避免音符视觉位置不随鼠标移动。性能方面：此函数在 hot loop 之外调用，
/// 仅用于判断是否走 ghost 路径，开销可忽略。
#[inline]
pub(crate) fn has_active_ghost_delta(
    pending: &Option<lumino_editor_state::DragState>,
    edit_state: &EditState,
) -> bool {
    pending.is_some()
        || matches!(
            edit_state,
            EditState::Dragging { .. } | EditState::DraggingSelection { .. }
        )
}

/// 检查音符在当前状态下是否处于"幽灵"位置（即被拖动或 pending）
///
/// 调用方在已知 `has_active_ghost_delta` 为 true 时，先用此函数判断是否需要
/// 应用偏移，再使用预提取的 delta 计算最终位置。
/// **DraggingSelection 也纳入检查**，确保第二次拖动（已有 pending 时）
/// 当前 drag_state 的选中音符也能正确渲染 ghost 位置。
#[inline]
pub(crate) fn is_note_ghosted(
    i: usize,
    pending: &Option<lumino_editor_state::DragState>,
    edit_state: &EditState,
) -> bool {
    // 检查当前拖动状态的选中集合（含 Dragging 和 DraggingSelection）
    match edit_state {
        EditState::Dragging { drag_state, .. } | EditState::DraggingSelection { drag_state }
            if i < drag_state.selected.len() && drag_state.selected[i] =>
        {
            return true;
        }
        _ => {}
    }
    // 检查 pending 拖动是否包含此音符
    if let Some(pending) = pending
        && i < pending.selected.len()
        && pending.selected[i]
    {
        return true;
    }
    false
}
