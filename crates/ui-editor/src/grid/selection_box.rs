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
    //
    // 性能优化（P0-C，2026-07-20）：
    // - Selecting 状态下，框选矩形的边界已被 `update_selection` 缓存到
    //   `editor.cached_selection_bounds`，直接从中计算 bbox 即可，避免 O(N) 遍历。
    // - 非 Selecting 状态（选完松手后）才走原 O(N) 路径——但松手后只绘一帧，不构成性能问题。
    let edit_state = &editor.editor_state.interaction.edit_state;

    let bbox_from_cache = editor.cached_selection_bounds.get().map(|(mt, mxt, mk, mxk)| {
        // cached_selection_bounds 是 (min_tick, max_tick, min_key, max_key)
        // 注意：max_tick 是框选矩形右边界的 tick，不是 note.tick+length。
        // 需要加一个小 padding（snap_precision）来包含可能超出右边界的音符长度。
        let snap = editor.editor_state.view.snap_precision.max(1.0);
        let max_tick_end = mxt + snap; // 至少一个网格，覆盖音符 length 超出部分
        (mt, max_tick_end, mk, mxk)
    });

    let selected = &editor.editor_state.interaction.selected_notes;
    if !selected.is_empty() {
        puffin::profile_scope!("draw::selection_box_bbox");
        let notes = &editor.editor_state.data.notes;
        let pending = &editor.pending_drag_state;
        let max_key = editor.editor_state.view.visible_key_count.saturating_sub(1);

        let (min_tick, max_tick_end, min_key_bound, max_key_bound, has_visible) =
            if let Some((c_min_t, c_max_t_end, c_min_k, c_max_k)) = bbox_from_cache {
                // 性能关键路径：Selecting 状态下直接从缓存获取边界，避免 O(N) 遍历
                (c_min_t, c_max_t_end, c_min_k, c_max_k, true)
            } else {
                // 非 Selecting 状态（或缓存不存在）：原 O(N) 退路，通常只执行一帧
                let mut min_t = f32::INFINITY;
                let mut max_t_end = f32::NEG_INFINITY;
                let mut max_k = u16::MIN;
                let mut min_k = u16::MAX;
                let mut vis = false;

                // ghost 方案：先判断是否需要 ghost delta
                let needs_ghost =
                    pending.is_some() || matches!(edit_state, EditState::Dragging { .. });

                if needs_ghost {
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
                            if let Some(pending) = pending
                                && i < pending.selected.len()
                                && pending.selected[i]
                            {
                                dt = dt.saturating_add(pending.delta_tick);
                                dk = dk.saturating_add(pending.delta_key);
                            }
                            let tick = (note.tick + dt as f32).max(0.0);
                            let key =
                                (note.key as i32 + dk as i32).clamp(0, max_key as i32) as u16;
                            min_t = min_t.min(tick);
                            max_t_end = max_t_end.max(tick + note.length);
                            max_k = max_k.max(key);
                            min_k = min_k.min(key);
                            vis = true;
                        }
                    }
                } else {
                    for &i in selected.iter() {
                        if let Some(note) = notes.get(i) {
                            min_t = min_t.min(note.tick);
                            max_t_end = max_t_end.max(note.tick + note.length);
                            max_k = max_k.max(note.key);
                            min_k = min_k.min(note.key);
                            vis = true;
                        }
                    }
                }

                (min_t, max_t_end, min_k, max_k, vis)
            };

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
