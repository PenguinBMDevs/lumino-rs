//! Puffin 性能监测瞄点
//!
//! 集中管理钢琴卷帘编辑器中音符框选与移动的关键路径性能监测点，
//! 方便在 puffin profiler UI 中定位热点。
//!
//! 命名约定：
//! - `selection_move::*` — 内部处理逻辑（鼠标移动、状态计算、提交等）
//! - `selection_move_ui::*` — UI 层渲染逻辑（绘制、可见数据收集、动画等）
//!
//! 注意：`puffin::profile_scope!` 创建的 scope 持续到当前函数返回，
//! 因此这些函数仅作为 profiler 时间线上的标记点，标记关键路径被命中的时刻。

// ===== 内部处理逻辑 =====

/// 鼠标移动事件处理入口（`interaction/moved.rs`）
pub fn moved_handle() {
    puffin::profile_scope!("selection_move::handle_moved");
}

/// 编辑变化计算（`moved.rs` → `calculate_edit_changes`）
pub fn calculate_edit_changes() {
    puffin::profile_scope!("selection_move::calculate_edit_changes");
}

/// 状态变化计算主分发（`drag.rs` → `compute_state_changes`）
pub fn compute_state_changes() {
    puffin::profile_scope!("selection_move::compute_state_changes");
}

/// 框选更新（`drag.rs` → `update_selection`）
pub fn update_selection() {
    puffin::profile_scope!("selection_move::update_selection");
}

/// 选择集拖动状态更新（`drag.rs` → `DraggingSelection` 分支）
pub fn dragging_selection() {
    puffin::profile_scope!("selection_move::dragging_selection");
}

/// 鼠标释放事件处理（`interaction/released.rs` → `handle_released`）
pub fn released_handle() {
    puffin::profile_scope!("selection_move::handle_released");
}

/// 批量拖动松手保存到 pending（`released.rs` → `DraggingSelection` 分支）
pub fn released_dragging_selection() {
    puffin::profile_scope!("selection_move::released_dragging_selection");
}

/// 提交 pending 批量拖动到 data.notes（`editor_impl.rs` → `commit_pending_drag`）
pub fn commit_pending_drag() {
    puffin::profile_scope!("selection_move::commit_pending_drag");
}

/// 轮询异步提交结果（`editor_impl.rs` → `poll_async_commit`）
pub fn poll_async_commit() {
    puffin::profile_scope!("selection_move::poll_async_commit");
}

/// 完成单音符拖动（`drag.rs` → `finalize_dragging`）
pub fn finalize_dragging() {
    puffin::profile_scope!("selection_move::finalize_dragging");
}

/// 尝试转换到拖动状态（`drag.rs` → `try_transition_to_dragging`）
pub fn try_transition_to_dragging() {
    puffin::profile_scope!("selection_move::try_transition_to_dragging");
}

// ===== UI 层渲染逻辑 =====

/// 网格部件主绘制函数（`grid/program_impl.rs` → `draw`）
pub fn grid_widget_draw() {
    puffin::profile_scope!("selection_move_ui::grid_widget_draw");
}

/// 选择框绘制（`grid/selection_box.rs` → `draw`）
pub fn selection_box_draw() {
    puffin::profile_scope!("selection_move_ui::selection_box_draw");
}

/// 收集可见音符数据（`rendering.rs` → `collect_visible_note_data`）
pub fn collect_visible_note_data() {
    puffin::profile_scope!("selection_move_ui::collect_visible_note_data");
}

/// 框选框弹簧物理动画更新（`impls/selection_box_anim.rs` → `update_selection_box_animation`）
pub fn update_selection_box_animation() {
    puffin::profile_scope!("selection_move_ui::update_selection_box_animation");
}

/// 音符变化应用（`moved.rs` → `apply_note_changes`）
pub fn apply_note_changes() {
    puffin::profile_scope!("selection_move_ui::apply_note_changes");
}

/// 标记 ghost 脏（触发 wgpu 重绘）
pub fn mark_ghost_dirty() {
    puffin::profile_scope!("selection_move_ui::mark_ghost_dirty");
}
