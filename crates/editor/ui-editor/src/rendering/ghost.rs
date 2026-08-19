//! Ghost 拖动偏移计算辅助函数
//!
//! 拆分原因：rendering.rs 接近 400 行限制，按职责拆分。
//!
//! 这些函数在 hot loop 中被调用（每帧可达百万次），绝对不要在此处添加
//! puffin::profile_scope! 等 per-element 开销。puffin scope 应放在外层的
//! 循环函数（collect_visible_note_data 等）中。

use crate::EditState;

/// 副本偏移对：`(旧副本偏移, 新副本偏移)`（连续复制时两条并存）
pub(crate) type CopyDeltaPair = (Option<(i64, i16)>, Option<(i64, i16)>);

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
/// 检查 `pending_drag_state`、`pending_copy_drag_state`、单音符 `Dragging`、
/// 批量拖动 `DraggingSelection` 和复制拖动 `DraggingSelectionCopy`。
/// `DraggingSelectionCopy` 也纳入检查，确保复制副本（原位置 + 偏移位置）
/// 能正确渲染。性能方面：此函数在 hot loop 之外调用，
/// 仅用于判断是否走 ghost 路径，开销可忽略。
#[inline]
pub(crate) fn has_active_ghost_delta(
    pending: &Option<lumino_editor_state::DragState>,
    pending_copy: &Option<lumino_editor_state::DragState>,
    edit_state: &EditState,
) -> bool {
    pending.is_some()
        || pending_copy.is_some()
        || matches!(
            edit_state,
            EditState::Dragging { .. }
                | EditState::DraggingSelection { .. }
                | EditState::DraggingSelectionCopy { .. }
                | EditState::ResizingSelectionStart { .. }
                | EditState::ResizingSelectionEnd { .. }
        )
}

/// 判断音符是否处于「复制 ghost」状态（Ctrl+拖动复制模式）
///
/// 与 `is_note_ghosted`（移动语义：音符从原位置"移走"）不同，复制模式下
/// 原始音符**保留在原位置**，副本在 `note + delta` 位置额外渲染一份。
/// 渲染层调用方在 push 原音符后，对 `is_copy_ghosted == true` 的音符
/// 再追加一条副本实例。
#[inline]
pub(crate) fn is_copy_ghosted(
    i: usize,
    pending_copy: &Option<lumino_editor_state::DragState>,
    edit_state: &EditState,
) -> bool {
    match edit_state {
        EditState::DraggingSelectionCopy { drag_state }
            if i < drag_state.selected.len() && drag_state.selected[i] =>
        {
            return true;
        }
        _ => {}
    }
    if let Some(copy) = pending_copy
        && i < copy.selected.len()
        && copy.selected[i]
    {
        return true;
    }
    false
}

/// 计算音符 i 的**所有副本偏移**（连续复制时旧副本与新副本并存）
///
/// 返回 `(旧副本偏移, 新副本偏移)`：
/// - **旧副本**（`pending_copy` 命中的音符）：`移动 delta + pending_copy.delta`
///   ——复制未提交时保持在上次副本位置（不含当前 `DraggingSelectionCopy` 的
///   `drag_state.delta`）。连续复制（第二次 Ctrl+拖动副本框）拖动中，旧副本
///   必须**保持原位**，用户才能看到"复制出下一份"而不是旧副本被吞并。
/// - **新副本**（`DraggingSelectionCopy` 拖动中，`drag_state.selected` 命中的
///   音符）：`移动 delta + pending_copy.delta + drag_state.delta`——从旧副本
///   位置继续偏移，跟随鼠标。
///
/// 组合语义：
/// - `Idle + pending_copy`：仅旧副本（`Some, None`）——松手后副本保持
/// - `DraggingSelectionCopy`（首次复制，无 pending_copy）：仅新副本（`None, Some`）
/// - `DraggingSelectionCopy + pending_copy`（连续复制拖动中）：两条并存（`Some, Some`）
///
/// 性能：仅对副本选中的音符调用（外层已用 `is_copy_ghosted` 过滤），
/// 复制场景帧率要求低，双 Option 元组零分配。
#[inline]
pub(crate) fn copy_deltas_for_index(
    i: usize,
    pending_copy: &Option<lumino_editor_state::DragState>,
    pending_drag: &Option<lumino_editor_state::DragState>,
    edit_state: &EditState,
) -> CopyDeltaPair {
    // 移动 delta（原件移动 → 副本跟随）：pending_drag + Dragging/DraggingSelection
    let mut m_dt = 0i64;
    let mut m_dk = 0i16;
    if let Some(drag) = pending_drag
        && i < drag.selected.len()
        && drag.selected[i]
    {
        m_dt = drag.delta_tick;
        m_dk = drag.delta_key;
    }
    match edit_state {
        EditState::Dragging { drag_state, .. } | EditState::DraggingSelection { drag_state }
            if i < drag_state.selected.len() && drag_state.selected[i] =>
        {
            m_dt = m_dt.saturating_add(drag_state.delta_tick);
            m_dk = m_dk.saturating_add(drag_state.delta_key);
        }
        _ => {}
    }
    // 复制 delta（pending_copy 的旧副本偏移）
    let mut c_dt = 0i64;
    let mut c_dk = 0i16;
    let has_old = if let Some(copy) = pending_copy
        && i < copy.selected.len()
        && copy.selected[i]
    {
        c_dt = copy.delta_tick;
        c_dk = copy.delta_key;
        true
    } else {
        false
    };
    // 旧副本：保持原位（移动 + 旧复制偏移，不含当前复制拖动）
    let old = has_old.then_some((m_dt.saturating_add(c_dt), m_dk.saturating_add(c_dk)));
    // 新副本：从旧副本位置继续偏移（移动 + 旧复制 + 当前复制拖动）
    let new = match edit_state {
        EditState::DraggingSelectionCopy { drag_state }
            if i < drag_state.selected.len() && drag_state.selected[i] =>
        {
            Some((
                m_dt.saturating_add(c_dt)
                    .saturating_add(drag_state.delta_tick),
                m_dk.saturating_add(c_dk)
                    .saturating_add(drag_state.delta_key),
            ))
        }
        _ => None,
    };
    (old, new)
}

/// 将音符 i 的所有副本实例位置推入渲染结果（连续复制时旧副本 + 新副本）
///
/// 在 `collect_visible_note_data` 的 hot loop 中调用，仅对副本选中的音符生效
/// （内部先过 `is_copy_ghosted`，非副本音符零开销）。
///
/// `visible` 闭包做视口过滤（窗口扫描路径需要；索引路径直接放行——原行为）。
/// 参数多为避免 hot path 分配结构体（与 collect_via_index/window 的既有约定一致）。
#[allow(clippy::too_many_arguments)]
#[inline]
pub(crate) fn push_copy_instances<F: Fn(f32, u16) -> bool>(
    result: &mut Vec<(f32, u16, f32)>,
    note_tick: f32,
    note_key: u16,
    length: f32,
    max_key: u16,
    i: usize,
    pending_copy: &Option<lumino_editor_state::DragState>,
    pending_drag: &Option<lumino_editor_state::DragState>,
    edit_state: &EditState,
    visible: F,
) {
    if !is_copy_ghosted(i, pending_copy, edit_state) {
        return;
    }
    let (old_copy, new_copy) = copy_deltas_for_index(i, pending_copy, pending_drag, edit_state);
    for (copy_dt, copy_dk) in [old_copy, new_copy].into_iter().flatten() {
        let copy_tick = (note_tick + copy_dt as f32).max(0.0);
        let copy_key = (note_key as i32 + copy_dk as i32).clamp(0, max_key as i32) as u16;
        if visible(copy_tick, copy_key) {
            result.push((copy_tick, copy_key, length));
        }
    }
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
