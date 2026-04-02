//! 琴键分隔线绘制

use super::theme::ThemeExt;
use crate::Renderer;
use crate::editor::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Path, Stroke};

/// 绘制琴键分隔线（横向线）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &crate::Theme) {
    let palette = theme.extended_palette().background;
    let view = &editor.state;

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

    for i in 0..view.visible_key_count {
        let keynum = i as isize;
        let world_y = (max_key_index - keynum as f32) * view.zoom_y;
        let screen_y = world_y - view.scroll_y + ruler_height;

        if screen_y + view.zoom_y >= ruler_height && screen_y <= bounds.height {
            let line_y = screen_y + view.zoom_y;
            let path = Path::line(
                Point::new(keyboard_width, line_y),
                Point::new(bounds.width, line_y),
            );
            frame.stroke(&path, line_stroke);
        }
    }
}
