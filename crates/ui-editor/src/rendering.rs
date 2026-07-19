use iced_core::{Length, Point};
use iced_widget::canvas::Canvas;

use crate::grid::PianoRollGrid;
use crate::scrollbar_widget;
use crate::{Element, Message};

use super::{EditState, Editor};

mod note_instances;

/// 计算音符 i 在当前编辑状态下的 ghost 偏移量
///
/// 合并规则（延迟提交方案）：
/// - `Idle` + `pending_drag_state`：返回 `pending.delta`（表示有未提交的拖动）
/// - `Dragging`：返回 `drag_state.delta`（仅对 drag_state.selected 中的音符）
/// - `DraggingSelection`：返回 `pending.delta + drag_state.delta`（仅对选中音符）
/// - 其他状态：返回 `None`
///
/// **关键修复**：原实现中 `pending_drag_state` 在 `DraggingSelection` 期间覆盖了
/// 当前 `drag_state` 的渲染，导致第二次拖动时 ghost 位置不随鼠标移动。
/// 现在合并两个 delta，确保拖动期间视觉反馈正确。
pub(crate) fn ghost_delta_for_index(
    i: usize,
    pending: &Option<lumino_core::DragState>,
    edit_state: &EditState,
) -> Option<(i64, i16)> {
    let mut delta_tick = 0i64;
    let mut delta_key = 0i16;
    let mut has_delta = false;

    // Idle 或 DraggingSelection 时，pending delta 生效
    if let Some(pending) = pending
        && matches!(
            edit_state,
            EditState::Idle | EditState::DraggingSelection { .. }
        )
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
    /// - 仅在音符数据变化时重建空间索引
    /// - 使用空间索引 O(log N) 查询替代全量扫描
    pub fn collect_visible_note_data(
        &self,
        result: &mut Vec<(f32, u16, f32)>,
        overscan_factor: f32,
    ) -> usize {
        result.clear();

        let (visible_tick_start, visible_tick_end, visible_key_min, visible_key_max) =
            self.compute_visible_range(overscan_factor);

        // 重建空间索引（仅当数据变化时）
        // 优化：使用 from_note_refs 直接从 im::Vector 构建，避免克隆 Note 到 Vec<Note>
        if self.spatial.note_index_dirty.get() {
            let notes = &self.editor_state.data.notes;
            let note_refs: Vec<lumino_core::NoteRef> = notes
                .iter()
                .enumerate()
                .map(|(i, n)| lumino_core::NoteRef {
                    tick: n.tick,
                    key: n.key,
                    length: n.length,
                    index: i,
                })
                .collect();
            *self.spatial.note_index.borrow_mut() = Some(
                crate::spatial_index::NoteSpatialIndex::from_note_refs(&note_refs),
            );
            self.spatial.note_index_dirty.set(false);
            tracing::debug!(
                "Editor: rebuild spatial index for {} notes",
                self.editor_state.data.notes.len()
            );
        }

        // 查询可见范围内的音符索引，再应用 ghost 偏移写入 result
        let mut indices = Vec::new();
        if let Some(index) = &*self.spatial.note_index.borrow() {
            index.update_query(
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                &mut indices,
            );
        }

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;

        for &i in &indices {
            if let Some(note) = self.editor_state.data.notes.get(i) {
                let mut tick = note.tick;
                let mut key = note.key;
                if let Some((dt, dk)) = ghost_delta_for_index(i, pending, edit_state) {
                    tick = (tick + dt as f32).max(0.0);
                    key = (key as i32 + dk as i32).clamp(0, max_key as i32) as u16;
                }
                result.push((tick, key, note.length));
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
    fn ghost_delta_selecting_or_resizing_returns_none() {
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

        assert_eq!(ghost_delta_for_index(0, &pending, &selecting), None);
        assert_eq!(ghost_delta_for_index(0, &pending, &resizing), None);
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
