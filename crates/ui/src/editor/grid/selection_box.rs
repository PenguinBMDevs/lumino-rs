//! 选择框渲染

use crate::Renderer;
use crate::constants::editor::SELECTION_BOX_FILL_ALPHA;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};

/// 绘制选择框
///
/// 两种情况会绘制选择框：
/// 1. 正在拖拽框选时（`EditState::Selecting`）——绘制半透明填充的选择框
/// 2. 有已选中的音符时——绘制围绕所有选中音符的方形边界框
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &crate::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
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

    // 情况 2：有已选中的音符——绘制围绕所有选中音符的方形边界框
    let selected = &editor.editor_state.interaction.selected_notes;
    if !selected.is_empty() {
        let notes = &editor.editor_state.data.notes;
        let mut min_tick = f32::INFINITY;
        let mut max_tick_end = f32::NEG_INFINITY;
        let mut max_key = u16::MIN;
        let mut min_key = u16::MAX;
        let mut has_visible = false;

        for &i in selected.iter() {
            if let Some(note) = notes.get(i) {
                min_tick = min_tick.min(note.tick);
                max_tick_end = max_tick_end.max(note.tick + note.length);
                max_key = max_key.max(note.key);
                min_key = min_key.min(note.key);
                has_visible = true;
            }
        }

        if has_visible {
            let min_x = editor.tick_to_x(min_tick);
            let max_x = editor.tick_to_x(max_tick_end);
            let min_y = editor.key_to_y(max_key);
            let zoom_y = editor.editor_state.view.zoom_y;
            let max_y = editor.key_to_y(min_key) + zoom_y;

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
