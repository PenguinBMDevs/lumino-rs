//! 事件浏览器 Canvas 绘制辅助组件。
//!
//! 从 draw.rs 拆分：分页器、空表提示、文本/矩形/颜色等通用绘制。

use iced_core::{Color, Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Text, path::Builder};

use crate::Renderer;
use crate::Theme;
use crate::editor::grid::theme::ThemeExt;
use crate::sidebar::event_browser::canvas::{
    FONT_SIZE, HEADER_HEIGHT, PAGER_BUTTON_WIDTH, PAGER_HEIGHT, ROW_HEIGHT,
};

/// 绘制底部翻页器（上一页/下一页 + 页码）。
pub(super) fn draw_pager(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    table_x: f32,
    table_content_h: f32,
    table_w: f32,
    page: usize,
    total_pages: usize,
) {
    let (_, _, text_color, line_color) = colors(theme);
    let y = table_content_h;
    frame.fill_rectangle(
        Point::new(table_x, y),
        Size::new(table_w, PAGER_HEIGHT),
        theme.extended_palette().background.weak.color,
    );

    // 左按钮
    draw_pager_button(frame, "<", Point::new(table_x + 4.0, y + 4.0), theme);
    // 右按钮
    draw_pager_button(
        frame,
        ">",
        Point::new(table_x + table_w - PAGER_BUTTON_WIDTH - 4.0, y + 4.0),
        theme,
    );

    let label = format!("{} / {}", page + 1, total_pages);
    draw_text(
        frame,
        &label,
        table_x + table_w * 0.5,
        y + PAGER_HEIGHT * 0.5,
        text_color,
        table_w - PAGER_BUTTON_WIDTH * 2.0 - 16.0,
        alignment::Horizontal::Center,
    );

    let mut path = Builder::new();
    path.move_to(Point::new(table_x, y));
    path.line_to(Point::new(table_x + table_w, y));
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(line_color)
            .with_width(1.0),
    );
}

/// 绘制单个分页器按钮。
fn draw_pager_button(frame: &mut Frame<Renderer>, label: &str, pos: Point, theme: &Theme) {
    let (_, _, text_color, border_color) = colors(theme);
    let rect = Rectangle::new(pos, Size::new(PAGER_BUTTON_WIDTH, PAGER_HEIGHT - 8.0));
    frame.fill_rectangle(
        rect.position(),
        rect.size(),
        theme.extended_palette().background.strong.color,
    );
    let mut path = Builder::new();
    draw_rect_path(&mut path, &rect);
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(border_color)
            .with_width(1.0),
    );
    draw_text(
        frame,
        label,
        pos.x + PAGER_BUTTON_WIDTH * 0.5,
        pos.y + rect.height * 0.5,
        text_color,
        PAGER_BUTTON_WIDTH,
        alignment::Horizontal::Center,
    );
}

/// 绘制空表加号提示。
pub(super) fn draw_empty_hint(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    table_x: f32,
    table_w: f32,
) {
    let (_, _, text_color, _) = colors(theme);
    draw_text(
        frame,
        "+",
        table_x + table_w * 0.5,
        HEADER_HEIGHT + ROW_HEIGHT * 2.0,
        text_color,
        table_w,
        alignment::Horizontal::Center,
    );
}

/// 绘制文本。
pub(super) fn draw_text(
    frame: &mut Frame<Renderer>,
    content: &str,
    x: f32,
    y: f32,
    color: Color,
    max_width: f32,
    align: alignment::Horizontal,
) {
    frame.fill_text(Text {
        content: content.to_string(),
        position: Point::new(x, y),
        color,
        size: iced_core::Pixels(FONT_SIZE),
        line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(FONT_SIZE + 2.0)),
        font: iced_core::Font::default(),
        max_width,
        align_x: align.into(),
        align_y: alignment::Vertical::Center,
        shaping: iced_widget::text::Shaping::Basic,
    });
}

/// 构建矩形路径。
pub(super) fn draw_rect_path(path: &mut Builder, rect: &Rectangle) {
    let p = rect.position();
    let s = rect.size();
    path.move_to(p);
    path.line_to(Point::new(p.x + s.width, p.y));
    path.line_to(Point::new(p.x + s.width, p.y + s.height));
    path.line_to(Point::new(p.x, p.y + s.height));
    path.close();
}

/// 主题颜色四元组：(表头背景, 表头前景, 正文, 分隔线)。
pub(super) fn colors(theme: &Theme) -> (Color, Color, Color, Color) {
    let palette = theme.extended_palette();
    let is_light = theme.is_light();
    let header_bg = if is_light {
        Color::from_rgb(0.92, 0.92, 0.92)
    } else {
        Color::from_rgb(0.18, 0.18, 0.18)
    };
    let header_fg = if is_light {
        Color::from_rgb(0.2, 0.2, 0.2)
    } else {
        Color::from_rgb(0.85, 0.85, 0.85)
    };
    let text_color = palette.background.base.text;
    let line_color = if is_light {
        Color::from_rgba(0.0, 0.0, 0.0, 0.15)
    } else {
        Color::from_rgba(1.0, 1.0, 1.0, 0.08)
    };
    (header_bg, header_fg, text_color, line_color)
}
