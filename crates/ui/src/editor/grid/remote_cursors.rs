//! 远程光标渲染

use super::utils::parse_color;
use crate::Renderer;
use crate::constants::editor as editor_constants;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Frame, Geometry, Path, Stroke};

/// 绘制远程光标
pub fn draw(editor: &Editor, renderer: &Renderer, bounds: Rectangle) -> Vec<Geometry<Renderer>> {
    let mut geometries = Vec::new();

    tracing::debug!(
        "远程光标绘制：remote_cursors 数量 = {}",
        editor.remote_cursors.len()
    );

    for (user_id, (pos, color_str, username)) in editor.remote_cursors.iter() {
        tracing::debug!(
            "绘制用户 {} 的光标：位置=({}, {}), 颜色={}, 用户名={}",
            user_id,
            pos.x,
            pos.y,
            color_str,
            username
        );

        let color = parse_color(color_str).unwrap_or(iced_core::Color::WHITE);
        let mut frame = Frame::new(renderer, bounds.size());

        // 减去滚动偏移，将内容空间坐标转换到当前视图
        let view = &editor.editor_state.view;
        let cursor_x = pos.x - view.scroll_x;
        let cursor_y = pos.y - view.scroll_y;

        tracing::debug!(
            "转换后坐标：scroll=({}, {}), 绘制位置=({}, {})",
            view.scroll_x,
            view.scroll_y,
            cursor_x,
            cursor_y
        );

        // 绘制圆圈光标
        draw_cursor_circle(
            &mut frame,
            cursor_x,
            cursor_y,
            color,
            editor_constants::REMOTE_CURSOR_CIRCLE_RADIUS,
        );

        // 绘制用户名牌
        draw_username_label(
            &mut frame,
            cursor_x,
            cursor_y,
            username,
            color,
            editor_constants::REMOTE_CURSOR_CIRCLE_RADIUS,
        );

        geometries.push(frame.into_geometry());
    }

    geometries
}

/// 绘制圆圈光标
fn draw_cursor_circle(
    frame: &mut Frame<Renderer>,
    x: f32,
    y: f32,
    color: iced_core::Color,
    radius: f32,
) {
    // 半透明填充
    let circle_path = Path::circle(Point::new(x, y), radius);
    frame.fill(&circle_path, iced_core::Color { a: 0.3, ..color });

    // 实线边框
    frame.stroke(
        &circle_path,
        Stroke::default()
            .with_width(editor_constants::REMOTE_CURSOR_BORDER_WIDTH)
            .with_color(iced_core::Color { a: 0.8, ..color }),
    );
}

/// 绘制用户名牌
fn draw_username_label(
    frame: &mut Frame<Renderer>,
    cursor_x: f32,
    cursor_y: f32,
    username: &str,
    color: iced_core::Color,
    circle_radius: f32,
) {
    let text_padding = editor_constants::USERNAME_LABEL_PADDING;
    let username_len = username.len() as f32 * 7.0; // 估算文本宽度
    let label_width = username_len + text_padding * 2.0;
    let label_height = editor_constants::USERNAME_LABEL_HEIGHT;
    let label_x = cursor_x + circle_radius + editor_constants::USERNAME_LABEL_ARROW_OFFSET;
    let label_y = cursor_y - label_height * 0.5;

    let label_rect = Rectangle::new(
        Point::new(label_x, label_y),
        Size::new(label_width, label_height),
    );
    let label_path = Path::rounded_rectangle(
        label_rect.position(),
        label_rect.size(),
        iced_core::border::Radius::from(editor_constants::USERNAME_LABEL_BORDER_RADIUS),
    );
    frame.fill(&label_path, color);

    // 绘制用户名文本
    let text = canvas::Text {
        content: username.to_string(),
        position: Point::new(label_x + text_padding, label_y + 2.0),
        max_width: label_width,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(editor_constants::CURSOR_LABEL_FONT_SIZE),
        color: iced_core::Color::WHITE,
        font: iced_core::Font::DEFAULT,
        align_x: iced_core::alignment::Horizontal::Left.into(),
        align_y: iced_core::alignment::Vertical::Top,
        shaping: iced_core::text::Shaping::Basic,
    };
    frame.fill_text(text);
}
