//! 琴键分隔线绘制

use super::theme::ThemeExt;
use lumino_ui_core::Renderer;
use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Stroke};

/// 绘制琴键分隔线（横向线）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &lumino_ui_core::Theme) {
    use iced_widget::canvas::path::Builder;

    let palette = theme.extended_palette().background;
    let view = &editor.editor_state.view;

    let line_color = if theme.is_light() {
        // 亮色主题：使用较深的颜色
        palette.strong.color
    } else {
        // 暗色主题：使用较浅的颜色
        palette.weak.color
    };

    let line_stroke = Stroke::default().with_width(1.0).with_color(line_color);

    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;
    let max_key_index = (view.visible_key_count - 1) as f32;

    // 使用单个 path builder 批量绘制所有线条，减少绘制调用开销
    let mut path_builder = Builder::new();

    for i in 0..view.visible_key_count {
        let keynum = i as isize;
        let world_y = (max_key_index - keynum as f32) * view.zoom_y;
        let screen_y = world_y - view.scroll_y + ruler_height;

        if screen_y + view.zoom_y >= ruler_height && screen_y <= bounds.height {
            let line_y = screen_y + view.zoom_y;
            // 将线条添加到同一个 path
            path_builder.move_to(Point::new(keyboard_width, line_y));
            path_builder.line_to(Point::new(bounds.width, line_y));
        }
    }

    // 一次性绘制所有线条
    let path = path_builder.build();
    frame.stroke(&path, line_stroke);
}
