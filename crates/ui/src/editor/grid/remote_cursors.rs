//! 远程光标渲染

use crate::Renderer;
use crate::constants::editor as editor_constants;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Frame, Geometry, Path, Stroke};

/// 绘制远程光标
pub fn draw(editor: &Editor, renderer: &Renderer, bounds: Rectangle) -> Vec<Geometry<Renderer>> {
    let mut geometries = Vec::new();

    for (pos, color_str, username) in editor.remote_cursors.values() {
        let color = super::parse_color(color_str).unwrap_or(iced_core::Color::WHITE);
        let mut frame = Frame::new(renderer, bounds.size());

        let cursor_x = pos.x;
        let cursor_y = pos.y;

        // 绘制游标线（贯穿整个高度）
        draw_cursor_line(
            &mut frame,
            cursor_x,
            bounds.height,
            color,
            editor_constants::REMOTE_CURSOR_LINE_WIDTH,
        );

        // 绘制鼠标指针（箭头形状）
        draw_cursor_arrow(
            &mut frame,
            cursor_x,
            cursor_y,
            color,
            editor_constants::CURSOR_ARROW_SIZE_PX,
        );

        // 绘制用户名牌
        draw_username_label(
            &mut frame,
            cursor_x,
            cursor_y,
            username,
            color,
            editor_constants::CURSOR_ARROW_SIZE_PX,
        );

        geometries.push(frame.into_geometry());
    }

    geometries
}

/// 绘制光标竖线
fn draw_cursor_line(
    frame: &mut Frame<Renderer>,
    x: f32,
    height: f32,
    color: iced_core::Color,
    width: f32,
) {
    let path = Path::line(Point::new(x, 0.0), Point::new(x, height));
    frame.stroke(
        &path,
        Stroke::default()
            .with_width(width)
            .with_color(iced_core::Color { a: 0.6, ..color }),
    );
}

/// 绘制光标箭头
fn draw_cursor_arrow(
    frame: &mut Frame<Renderer>,
    x: f32,
    y: f32,
    color: iced_core::Color,
    size: f32,
) {
    // 箭头主体
    let arrow_path = create_arrow_path(x, y, size);
    frame.fill(&arrow_path, color);

    // 白色边框
    let arrow_border = create_arrow_path(x, y, size);
    frame.stroke(
        &arrow_border,
        Stroke::default()
            .with_width(editor_constants::REMOTE_CURSOR_BORDER_WIDTH)
            .with_color(iced_core::Color::WHITE),
    );
}

/// 创建箭头路径
fn create_arrow_path(x: f32, y: f32, size: f32) -> Path {
    Path::new(|builder| {
        // 箭头指向左上方
        builder.move_to(Point::new(x, y));
        builder.line_to(Point::new(x, y + size));
        builder.line_to(Point::new(x + size * 0.5, y + size * 0.8));
        builder.line_to(Point::new(x + size * 0.8, y + size * 1.5));
        builder.line_to(Point::new(x + size * 1.2, y + size * 1.2));
        builder.line_to(Point::new(x + size * 0.9, y + size * 0.5));
        builder.line_to(Point::new(x + size, y));
        builder.close();
    })
}

/// 绘制用户名牌
fn draw_username_label(
    frame: &mut Frame<Renderer>,
    cursor_x: f32,
    cursor_y: f32,
    username: &str,
    color: iced_core::Color,
    arrow_size: f32,
) {
    let text_padding = editor_constants::USERNAME_LABEL_PADDING;
    let username_len = username.len() as f32 * 7.0; // 估算文本宽度
    let label_width = username_len + text_padding * 2.0;
    let label_height = editor_constants::USERNAME_LABEL_HEIGHT;
    let label_x = cursor_x + arrow_size + editor_constants::USERNAME_LABEL_ARROW_OFFSET;
    let label_y = cursor_y - editor_constants::USERNAME_LABEL_TEXT_Y_OFFSET;

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
        align_y: iced_core::alignment::Vertical::Top.into(),
        shaping: iced_core::text::Shaping::Basic,
    };
    frame.fill_text(text);
}
