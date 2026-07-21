//! 选择框渲染

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
    // 性能优化：
    // - Selecting 状态下，框选矩形的边界已被 `update_selection` 缓存到
    //   `cached_selection_bounds`，直接从中计算 bbox 即可，避免全量遍历。
    // - 非 Selecting 状态，使用 `get_selection_box_bounds()`（增量维护的 O(1) 缓存，
    //   或 ghost 路径的 O(N) 计算），消除 selection_box::draw 中重复的 O(N) ghost 路径。
    // - 2026-07-20 重构：消除与 note_ops::get_selection_box_bounds 重复的 ghost 逻辑。
    let selected = &editor.editor_state.interaction.selected_notes;
    let has_selection =
        !selected.is_empty() || editor.editor_state.interaction.selection_bitset.is_some();
    if has_selection {
        puffin::profile_scope!("draw::selection_box_bbox");

        // 优先使用 cached_selection_bounds（Selecting 状态下由 update_selection 增量维护）
        let has_bbox = if let Some((c_min_t, c_max_t, c_min_k, c_max_k)) =
            editor.cached_selection_bounds.get()
        {
            // cached_selection_bounds 是 (min_tick, max_tick, min_key, max_key)
            // 注意：max_tick 是框选矩形右边界的 tick，不是 note.tick+length。
            // 需要加一个小 padding（snap_precision）来包含可能超出右边界的音符长度。
            let snap = editor.editor_state.view.snap_precision.max(1.0);
            let max_tick_end = c_max_t + snap; // 至少一个网格，覆盖音符 length 超出部分
            let min_x = editor.tick_to_x(c_min_t);
            let max_x = editor.tick_to_x(max_tick_end);
            let min_y = editor.key_to_y(c_max_k);
            let max_y = editor.key_to_y(c_min_k) + editor.editor_state.view.zoom_y;
            Some((min_x, max_x, min_y, max_y))
        } else {
            // 非 Selecting 状态：使用 get_selection_box_bounds（O(1) 缓存或 O(N) 兜底）
            // 消除 selection_box::draw 中重复的 ghost 路径
            editor.get_selection_box_bounds()
        };

        if let Some((min_x, max_x, min_y, max_y)) = has_bbox {
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
