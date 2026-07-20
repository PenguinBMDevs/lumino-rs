use iced_core::{Length, Point};
use iced_widget::canvas::Canvas;

use crate::grid::PianoRollGrid;
use crate::scrollbar_widget;
use crate::{Element, Message};

use super::{EditState, Editor};

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
    pending: &Option<lumino_core::DragState>,
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
/// 仅检查 `pending_drag_state` 和单音符 `Dragging`。
/// **DraggingSelection 期间不返回 ghost delta**——批量拖动时所有选中音符变化量相同，
/// 无需每帧每元素计算，变化量在松开鼠标时一次性算完保存到 `pending_drag_state`。
#[inline]
fn has_active_ghost_delta(
    pending: &Option<lumino_core::DragState>,
    edit_state: &EditState,
) -> bool {
    pending.is_some()
        || matches!(edit_state, EditState::Dragging { .. })
}

/// 检查音符在当前状态下是否处于"幽灵"位置（即被拖动或 pending）
///
/// 调用方在已知 `has_active_ghost_delta` 为 true 时，先用此函数判断是否需要
/// 应用偏移，再使用预提取的 delta 计算最终位置。
/// **DraggingSelection 不走此路径**——变化量只在松开鼠标时计算一次。
#[inline]
fn is_note_ghosted(
    i: usize,
    pending: &Option<lumino_core::DragState>,
    edit_state: &EditState,
) -> bool {
    // 检查当前单音符拖动的选中状态
    if let EditState::Dragging { drag_state, .. } = edit_state
        && i < drag_state.selected.len() && drag_state.selected[i]
    {
        return true;
    }
    // 检查 pending 拖动是否包含此音符
    if let Some(pending) = pending
        && i < pending.selected.len() && pending.selected[i]
    {
        return true;
    }
    false
}

impl Editor {
    /// 计算当前视口内可见的音符数据范围
    ///
    /// 返回 `(visible_tick_start, visible_tick_end, visible_key_min, visible_key_max)`。
    /// `overscan_factor` 用于扩展查询范围，例如 0.5 表示每边扩展 50%。
    pub fn compute_visible_range(&self, overscan_factor: f32) -> (f32, f32, u16, u16) {
        let es = &self.editor_state;
        let view = &es.view;
        let canvas = &es.canvas;

        let viewport_width = canvas.size_x - view.keyboard_width;
        let viewport_height = canvas.size_y - view.ruler_height;

        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let visible_tick_end =
            ((view.scroll_x + viewport_width) / view.zoom_x).max(visible_tick_start);

        let max_key_index = (view.visible_key_count - 1) as f32;

        // key_top 对应屏幕上方 (Y = ruler_height)，值最大（高音）
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        // key_bottom 对应屏幕下方 (Y = canvas_size.y)，值最小（低音）
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);

        let visible_key_max = key_top_f32.ceil() as u16 + 1; // 多取 1 个作为缓冲
        let visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1); // 多取 1 个作为缓冲

        if overscan_factor > 0.0 {
            let tick_span = visible_tick_end - visible_tick_start;
            let key_span = visible_key_max.saturating_sub(visible_key_min);
            let tick_expand = tick_span * overscan_factor;
            let key_expand = ((key_span as f32 * overscan_factor) as u16).max(1);

            let expanded_tick_start = (visible_tick_start - tick_expand).max(0.0);
            let expanded_tick_end = visible_tick_end + tick_expand;
            let expanded_key_min = visible_key_min.saturating_sub(key_expand);
            let expanded_key_max = visible_key_max.saturating_add(key_expand);

            return (
                expanded_tick_start,
                expanded_tick_end,
                expanded_key_min,
                expanded_key_max,
            );
        }

        (
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
        )
    }

    /// 收集当前视口内可见的音符数据（tick, key, length）
    ///
    /// `overscan_factor` 用于扩展查询范围，减少频繁重建。0.0 表示精确视口。
    /// 返回可见音符数量，结果写入传入的 buffer。
    ///
    /// **ghost 方案**：返回的数据已应用 `pending_drag_state` 与当前 `drag_state`
    /// 的偏移，确保拖动期间主音轨音符（蓝色）的渲染位置与视觉反馈一致。
    ///
    /// 性能优化：
    /// - 渲染路径**不触发** `ensure_spatial_index`，避免移动提交后 133ms 全量重建
    /// - `dirty` 时走线性扫描（O(N)，N=50000 仅 ~0.5ms），`!dirty` 且有索引时用索引查询
    /// - 索引重建交给交互路径（`hit_test_note` / `update_selection`）按需触发
    pub fn collect_visible_note_data(
        &self,
        result: &mut Vec<(f32, u16, f32)>,
        overscan_factor: f32,
    ) -> usize {
        crate::puffin_profiler::collect_visible_note_data();
        result.clear();

        let (visible_tick_start, visible_tick_end, visible_key_min, visible_key_max) =
            self.compute_visible_range(overscan_factor);

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;

        // 渲染路径：仅在索引干净（!dirty 且已存在）时复用，否则线性扫描。
        // 避免渲染帧触发 133ms 的全量重建——重建交给交互路径按需完成。
        let has_clean_index =
            !self.spatial.note_index_dirty.get() && self.spatial.note_index.borrow().is_some();

        // 性能优化：当没有活跃拖动时，ghost_delta_for_index 对每个音符都返回 None，
        // 避免 200 万次函数调用开销。先判断再决定走哪条路径。
        let needs_ghost = has_active_ghost_delta(pending, edit_state);

        if has_clean_index {
            let index = self.spatial.note_index.borrow();
            let index = index.as_ref().expect("已校验 is_some");
            let mut indices = Vec::new();
            index.update_query(
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                &mut indices,
            );
            if needs_ghost {
                // 只有 pending 或 Dragging（单音符）会进入此分支。
                // DraggingSelection 不走此路径——变化量只在松开鼠标时计算一次。
                let (drag_dt, drag_dk) = match edit_state {
                    EditState::Dragging { drag_state, .. } => {
                        (drag_state.delta_tick, drag_state.delta_key)
                    }
                    _ => (0i64, 0i16),
                };

                for &i in &indices {
                    if let Some(note) = self.editor_state.data.notes.get(i) {
                        let (tick, key) = if is_note_ghosted(i, pending, edit_state) {
                            let mut dt = drag_dt;
                            let mut dk = drag_dk;
                            if let Some(pending) = pending
                                && i < pending.selected.len() && pending.selected[i]
                            {
                                dt = dt.saturating_add(pending.delta_tick);
                                dk = dk.saturating_add(pending.delta_key);
                            }
                            ((note.tick + dt as f32).max(0.0), (note.key as i32 + dk as i32).clamp(0, max_key as i32) as u16)
                        } else {
                            (note.tick, note.key)
                        };
                        result.push((tick, key, note.length));
                    }
                }
            } else {
                for &i in &indices {
                    if let Some(note) = self.editor_state.data.notes.get(i) {
                        result.push((note.tick, note.key, note.length));
                    }
                }
            }
        } else {
            // 索引脏或不存在：线性扫描视口范围内的音符
            if needs_ghost {
                let (drag_dt, drag_dk) = match edit_state {
                    EditState::Dragging { drag_state, .. } => {
                        (drag_state.delta_tick, drag_state.delta_key)
                    }
                    _ => (0i64, 0i16),
                };

                for (i, note) in self.editor_state.data.notes.iter().enumerate() {
                    let (tick, key) = if is_note_ghosted(i, pending, edit_state) {
                        let mut dt = drag_dt;
                        let mut dk = drag_dk;
                        if let Some(pending) = pending
                            && i < pending.selected.len() && pending.selected[i]
                        {
                            dt = dt.saturating_add(pending.delta_tick);
                            dk = dk.saturating_add(pending.delta_key);
                        }
                        ((note.tick + dt as f32).max(0.0), (note.key as i32 + dk as i32).clamp(0, max_key as i32) as u16)
                    } else {
                        (note.tick, note.key)
                    };
                    let note_end = tick + note.length;
                    if key >= visible_key_min
                        && key <= visible_key_max
                        && note_end >= visible_tick_start
                        && tick <= visible_tick_end
                    {
                        result.push((tick, key, note.length));
                    }
                }
            } else {
                for note in self.editor_state.data.notes.iter() {
                    let note_end = note.tick + note.length;
                    if note.key >= visible_key_min
                        && note.key <= visible_key_max
                        && note_end >= visible_tick_start
                        && note.tick <= visible_tick_end
                    {
                        result.push((note.tick, note.key, note.length));
                    }
                }
            }
        }

        result.len()
    }

    /// 构建编辑器视图
    pub fn view<'a>(
        &'a self,
        on_scroll_x: impl Fn(f32) -> Message + 'static,
        on_scroll_y: impl Fn(f32) -> Message + 'static,
        on_zoom_x: impl Fn(f32, f32) -> Message + 'static,
        on_zoom_y: impl Fn(f32, f32) -> Message + 'static,
    ) -> Element<'a> {
        // 使用 editor_state 获取视图状态
        let es = &self.editor_state;

        // 创建跟随滚动的 Canvas
        let grid = Canvas::new(PianoRollGrid::new(self))
            .width(Length::Fill)
            .height(Length::Fill);

        let horizontal_scrollbar = scrollbar_widget::ScrollbarWidget::horizontal(
            es.view.scroll_x,
            es.max_scroll.0,
            es.view.zoom_x,
            None,
            on_scroll_x,
            on_zoom_x,
        );

        let viewport_height = (es.canvas.size_y - es.view.ruler_height).max(0.0);
        let vertical_scrollbar = scrollbar_widget::ScrollbarWidget::vertical(
            es.view.scroll_y,
            es.max_scroll.1,
            es.view.zoom_y,
            Some(viewport_height),
            on_scroll_y,
            on_zoom_y,
        );

        let content_with_vscroll = iced_widget::row![grid, vertical_scrollbar];

        let editor_content = iced_widget::column![content_with_vscroll, horizontal_scrollbar];

        // 如果右键上下文菜单打开，叠加悬浮面板
        if self.context_menu.open
            && let Some(position) = self.context_menu.position
        {
            return iced_widget::Stack::new()
                .push(editor_content)
                .push(crate::context_menu::background_close_overlay())
                .push(crate::context_menu::view(position))
                .into();
        }

        editor_content.into()
    }

    /// 获取选择框的屏幕坐标（用于渲染选择框）
    ///
    /// 将选择框坐标（tick/key）转换为屏幕坐标，确保选择框与音符对齐
    pub fn get_selection_box(&self) -> Option<(Point, Point)> {
        if let EditState::Selecting {
            start_tick,
            start_key,
            current_tick,
            current_key,
        } = self.editor_state.interaction.edit_state
        {
            let start_x = self.tick_to_x(start_tick);
            let start_y = self.key_to_y(start_key);
            let current_x = self.tick_to_x(current_tick);
            let current_y = self.key_to_y(current_key);
            Some((
                Point::new(start_x, start_y),
                Point::new(current_x, current_y),
            ))
        } else {
            None
        }
    }

    /// 判断是否在 Canvas 有效区域内
    /// 有效区域 = Canvas 除去键盘区域（左侧）、标尺区域（顶部）、滚动条区域（底部和右侧）
    /// 同时避开窗口边框和菜单栏的边界区域
    pub fn is_inside_canvas(&self, local_pos: Point) -> bool {
        let es = &self.editor_state;

        // 检查 Canvas 边界
        if local_pos.x < 0.0 || local_pos.x > es.canvas.size_x {
            return false;
        }
        if local_pos.y < 0.0 || local_pos.y > es.canvas.size_y {
            return false;
        }

        // 检查是否在键盘区域外（x 大于键盘宽度）
        if local_pos.x < es.view.keyboard_width {
            return false;
        }

        // 检查是否在标尺区域下方（y 大于标尺高度）
        if local_pos.y < es.view.ruler_height {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::ghost_delta_for_index;
    use crate::EditState;
    use lumino_core::DragState;

    fn drag_state_with_selected(indices: &[usize], total: usize, dt: i64, dk: i16) -> DragState {
        let mut ds = DragState::from_indices(indices.iter().copied(), total, 0, 0);
        ds.set_delta(dt, dk);
        ds
    }

    #[test]
    fn ghost_delta_idle_with_pending_returns_pending_delta() {
        let pending = Some(drag_state_with_selected(&[1], 4, 120, 3));
        let edit_state = EditState::Idle;

        assert_eq!(
            ghost_delta_for_index(1, &pending, &edit_state),
            Some((120, 3))
        );
        assert_eq!(ghost_delta_for_index(0, &pending, &edit_state), None);
    }

    #[test]
    fn ghost_delta_dragging_returns_drag_state_delta() {
        let drag_state = drag_state_with_selected(&[2], 4, 240, -5);
        let edit_state = EditState::Dragging {
            note_index: 2,
            drag_state,
            last_played_key: 0,
        };

        assert_eq!(
            ghost_delta_for_index(2, &None, &edit_state),
            Some((240, -5))
        );
        assert_eq!(ghost_delta_for_index(1, &None, &edit_state), None);
    }

    #[test]
    fn ghost_delta_dragging_selection_merges_pending_and_drag() {
        let pending = Some(drag_state_with_selected(&[1, 2], 4, 100, 2));
        let drag_state = drag_state_with_selected(&[1, 2], 4, 50, 3);
        let edit_state = EditState::DraggingSelection { drag_state };

        // 选中音符合并两个 delta
        assert_eq!(
            ghost_delta_for_index(1, &pending, &edit_state),
            Some((150, 5))
        );
        // 未选中音符无 delta
        assert_eq!(ghost_delta_for_index(0, &pending, &edit_state), None);
    }

    #[test]
    fn ghost_delta_selecting_or_resizing_applies_pending() {
        // pending 代表已启动异步提交但尚未完成的数据更新，在异步完成前应始终可见，
        // 不应因进入 Selecting / Resizing 等状态而回撤。
        let pending = Some(drag_state_with_selected(&[0], 2, 10, 1));
        let selecting = EditState::Selecting {
            start_tick: 0.0,
            start_key: 0,
            current_tick: 0.0,
            current_key: 0,
        };
        let resizing = EditState::ResizingStart {
            note_index: 0,
            original_tick: 0.0,
            original_length: 100.0,
        };

        assert_eq!(
            ghost_delta_for_index(0, &pending, &selecting),
            Some((10, 1))
        );
        assert_eq!(ghost_delta_for_index(0, &pending, &resizing), Some((10, 1)));
        // 未在 pending 选中集合中的音符无 delta
        assert_eq!(ghost_delta_for_index(1, &pending, &selecting), None);
    }

    #[test]
    fn ghost_delta_saturates_on_overflow() {
        let pending = Some(drag_state_with_selected(&[0], 1, i64::MAX, i16::MAX));
        let mut drag_state = drag_state_with_selected(&[0], 1, i64::MAX, i16::MAX);
        // 单独设置 delta 为 MAX，避免构造时相加溢出
        drag_state.set_delta(i64::MAX, i16::MAX);
        let edit_state = EditState::DraggingSelection { drag_state };

        assert_eq!(
            ghost_delta_for_index(0, &pending, &edit_state),
            Some((i64::MAX, i16::MAX))
        );
    }
}
