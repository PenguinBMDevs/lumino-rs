//! 选择框渲染

use crate::EditState;
use crate::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_ui_constants::editor::SELECTION_BOX_FILL_ALPHA;
use lumino_ui_core::Renderer;

/// 绘制选择框
///
/// 两种情况会绘制选择框：
/// 1. 正在拖拽框选时（`EditState::Selecting`）——绘制半透明填充的选择框
/// 2. 有已选中的音符时——绘制围绕所有选中音符的方形边界框
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    crate::puffin_profiler::selection_box_draw();
    let palette = theme.extended_palette();
    let selection_stroke_color = palette.secondary.strong.color;
    let selection_fill_color = iced_core::Color {
        a: SELECTION_BOX_FILL_ALPHA,
        ..palette.secondary.weak.color
    };
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;

    // 情况 1：正在拖拽框选——绘制半透明填充的选择框
    // 根据框选框模式决定显示方式：
    // - Direct 模式：直接使用当前实际位置（无动画延迟）
    // - Spring 模式：优先使用动画显示位置（弹簧弹性效果）
    let selection_box =
        if editor.selection_box_mode() == lumino_core::storage::config::SelectionBoxMode::Direct {
            editor.get_selection_box()
        } else {
            editor
                .selection_box_anim
                .get()
                .map(|anim| (anim.start_pos, anim.current_pos))
                .or_else(|| editor.get_selection_box())
        };

    if let Some((start_pos, current_pos)) = selection_box {
        let min_x = start_pos.x.min(current_pos.x);
        let max_x = start_pos.x.max(current_pos.x);
        let min_y = start_pos.y.min(current_pos.y);
        let max_y = start_pos.y.max(current_pos.y);

        let width = (max_x - min_x).max(1.0);
        let height = (max_y - min_y).max(1.0);

        let rect = Rectangle::new(Point::new(min_x, min_y), Size::new(width, height));
        let path = Path::rectangle(rect.position(), rect.size());

        frame.fill(&path, selection_fill_color);

        let stroke = Stroke::default()
            .with_width(1.0)
            .with_color(selection_stroke_color);
        frame.stroke(&path, stroke);

        has_content = true;
    }

    // 情况 2：有已选中的音符——绘制围绕所有选中音符的方形边界框。
    // 拖动期间使用 ghost 位置，使选择框跟随被拖动的音符一起移动。
    let selected = &editor.editor_state.interaction.selected_notes;
    if !selected.is_empty() {
        let notes = &editor.editor_state.data.notes;
        let edit_state = &editor.editor_state.interaction.edit_state;
        let pending = &editor.pending_drag_state;
        let max_key = editor.editor_state.view.visible_key_count.saturating_sub(1);

        // 性能优化：先判断是否需要 ghost delta，避免在循环中每元素调用。
        // DraggingSelection 期间不应用 ghost——变化量只在松开鼠标时计算一次。
        let needs_ghost = pending.is_some()
            || matches!(edit_state, EditState::Dragging { .. });

        let mut min_tick = f32::INFINITY;
        let mut max_tick_end = f32::NEG_INFINITY;
        let mut max_key_bound = u16::MIN;
        let mut min_key_bound = u16::MAX;
        let mut has_visible = false;

        if needs_ghost {
            // 性能优化：提取 drag_state delta 一次，避免在循环中每元素调用
            // ghost_delta_for_index。因为我们知道所有迭代的音符都是 selected 的，
            // drag_state.selected[i] 恒为 true，无需在循环中重复检查。
            let (drag_dt, drag_dk) = match edit_state {
                EditState::Dragging { drag_state, .. } => {
                    (drag_state.delta_tick, drag_state.delta_key)
                }
                _ => (0i64, 0i16),
            };

            for &i in selected.iter() {
                if let Some(note) = notes.get(i) {
                    let mut dt = drag_dt;
                    let mut dk = drag_dk;
                    // pending 可能包含之前未提交的拖动的 delta，需要叠加
                    if let Some(pending) = pending
                        && i < pending.selected.len() && pending.selected[i]
                    {
                        dt = dt.saturating_add(pending.delta_tick);
                        dk = dk.saturating_add(pending.delta_key);
                    }
                    let tick = (note.tick + dt as f32).max(0.0);
                    let key = (note.key as i32 + dk as i32).clamp(0, max_key as i32) as u16;
                    min_tick = min_tick.min(tick);
                    max_tick_end = max_tick_end.max(tick + note.length);
                    max_key_bound = max_key_bound.max(key);
                    min_key_bound = min_key_bound.min(key);
                    has_visible = true;
                }
            }
        } else {
            for &i in selected.iter() {
                if let Some(note) = notes.get(i) {
                    min_tick = min_tick.min(note.tick);
                    max_tick_end = max_tick_end.max(note.tick + note.length);
                    max_key_bound = max_key_bound.max(note.key);
                    min_key_bound = min_key_bound.min(note.key);
                    has_visible = true;
                }
            }
        }

        if has_visible {
            let min_x = editor.tick_to_x(min_tick);
            let max_x = editor.tick_to_x(max_tick_end);
            let min_y = editor.key_to_y(max_key_bound);
            let zoom_y = editor.editor_state.view.zoom_y;
            let max_y = editor.key_to_y(min_key_bound) + zoom_y;

            let width = max_x - min_x;
            let height = max_y - min_y;

            if width >= 1.0 && height >= 1.0 {
                let rect = Rectangle::new(Point::new(min_x, min_y), Size::new(width, height));
                let path = Path::rectangle(rect.position(), rect.size());

                // 只绘制边框，不填充
                let stroke = Stroke::default()
                    .with_width(3.0)
                    .with_color(selection_stroke_color);
                frame.stroke(&path, stroke);

                has_content = true;
            }
        }
    }

    if has_content {
        Some(frame.into_geometry())
    } else {
        None
    }
}
