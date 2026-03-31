//! 钢琴键盘绘制

use super::theme::ThemeExt;
use crate::Renderer;
use crate::constants::editor::RULER_HEIGHT;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Path, Stroke};

/// 绘制钢琴键盘（左侧键位指示器）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &crate::Theme) {
    let view = &editor.state;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;
    let max_key_index = (view.visible_key_count - 1) as f32;

    // 绘制键盘区域背景（时间轴标尺下方）
    let keyboard_bg_rect = Rectangle::new(
        Point::new(0.0, ruler_height),
        Size::new(keyboard_width, bounds.height - ruler_height),
    );
    let keyboard_bg_path = Path::rectangle(keyboard_bg_rect.position(), keyboard_bg_rect.size());
    let bg_color = theme.keyboard_background_color();
    frame.fill(&keyboard_bg_path, bg_color);

    // 绘制每个琴键
    for i in 0..view.visible_key_count {
        let keynum = i as isize;
        let world_y = (max_key_index - keynum as f32) * view.zoom_y;
        let screen_y = world_y - view.scroll_y + ruler_height;

        if screen_y + view.zoom_y >= ruler_height && screen_y <= bounds.height {
            let is_black_key = super::is_key_dark(keynum);
            let key_color = if is_black_key {
                theme.black_key_color()
            } else {
                theme.white_key_color()
            };

            let key_rect = Rectangle::new(
                Point::new(0.0, screen_y),
                Size::new(keyboard_width, view.zoom_y),
            );
            let key_path = Path::rectangle(key_rect.position(), key_rect.size());
            frame.fill(&key_path, key_color);

            let border_stroke = Stroke::default()
                .with_width(1.0)
                .with_color(theme.border_color());
            frame.stroke(&key_path, border_stroke);
        }
    }
}
