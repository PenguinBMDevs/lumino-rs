//! 浮动编辑弹窗绘制与命中测试。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Text, path::Builder};

use crate::Renderer;
use crate::Theme;
use crate::editor::grid::theme::ThemeExt;
use crate::sidebar::event_browser::canvas::{FONT_SIZE, HEADER_HEIGHT, ROW_HEIGHT};
use crate::sidebar::event_browser::edit::PopupState;

const POPUP_WIDTH: f32 = 260.0;
const POPUP_HEIGHT: f32 = 120.0;
const BUTTON_WIDTH: f32 = 60.0;
const BUTTON_HEIGHT: f32 = 24.0;
const ARROW_SIZE: f32 = 18.0;

/// 弹窗点击命中结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupHit {
    None,
    Ok,
    Cancel,
    ChoicePrev,
    ChoiceNext,
}

/// 在 Canvas 中心绘制弹窗叠加层。
pub fn draw_popup(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    popup: &PopupState,
    bounds: Rectangle,
) {
    let palette = theme.extended_palette();
    let is_light = theme.is_light();

    // 半透明遮罩
    frame.fill_rectangle(
        Point::new(0.0, 0.0),
        bounds.size(),
        Color::from_rgba(0.0, 0.0, 0.0, 0.35),
    );

    let cx = bounds.width * 0.5;
    let cy = bounds.height * 0.5;
    let rect = Rectangle::new(
        Point::new(cx - POPUP_WIDTH * 0.5, cy - POPUP_HEIGHT * 0.5),
        Size::new(POPUP_WIDTH, POPUP_HEIGHT),
    );

    let bg = if is_light {
        Color::from_rgb(0.98, 0.98, 0.98)
    } else {
        palette.background.base.color
    };
    let fg = if is_light {
        Color::from_rgb(0.1, 0.1, 0.1)
    } else {
        palette.background.base.text
    };
    let border = if is_light {
        Color::from_rgb(0.7, 0.7, 0.7)
    } else {
        palette.background.strong.color
    };

    frame.fill_rectangle(rect.position(), rect.size(), bg);
    let mut path = Builder::new();
    draw_rect_path(&mut path, &rect);
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(border)
            .with_width(1.0),
    );

    // 标题
    let title_y = rect.y + HEADER_HEIGHT * 0.5;
    frame.fill_text(Text {
        content: popup.title().to_string(),
        position: Point::new(cx, title_y),
        color: fg,
        size: iced_core::Pixels(FONT_SIZE + 1.0),
        line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(FONT_SIZE + 2.0)),
        font: iced_core::Font::default(),
        max_width: POPUP_WIDTH - 16.0,
        align_x: iced_core::alignment::Horizontal::Center.into(),
        align_y: iced_core::alignment::Vertical::Center,
        shaping: iced_widget::text::Shaping::Basic,
    });

    // 输入值
    let value_y = rect.y + HEADER_HEIGHT + ROW_HEIGHT + 4.0;
    let value_text = popup.value_text();
    draw_input_box(
        frame,
        cx,
        value_y,
        POPUP_WIDTH - 24.0,
        ROW_HEIGHT + 4.0,
        &value_text,
        fg,
        border,
    );

    // Choice 时左右箭头
    if matches!(popup, PopupState::Choice { .. }) {
        let arrow_y = value_y;
        let left_center = Point::new(rect.x + 16.0, arrow_y);
        let right_center = Point::new(rect.x + POPUP_WIDTH - 16.0, arrow_y);
        draw_triangle(frame, left_center, ARROW_SIZE, true, fg);
        draw_triangle(frame, right_center, ARROW_SIZE, false, fg);
    }

    // OK / Cancel 按钮
    let btn_y = rect.y + POPUP_HEIGHT - BUTTON_HEIGHT - 12.0;
    draw_button(
        frame,
        "OK",
        Point::new(cx - BUTTON_WIDTH - 8.0, btn_y),
        palette.primary.strong.color,
        Color::WHITE,
    );
    draw_button(
        frame,
        "Cancel",
        Point::new(cx + 8.0, btn_y),
        palette.background.weak.color,
        fg,
    );
}

fn draw_rect_path(path: &mut Builder, rect: &Rectangle) {
    let p = rect.position();
    let s = rect.size();
    path.move_to(p);
    path.line_to(Point::new(p.x + s.width, p.y));
    path.line_to(Point::new(p.x + s.width, p.y + s.height));
    path.line_to(Point::new(p.x, p.y + s.height));
    path.close();
}

#[allow(clippy::too_many_arguments)]
fn draw_input_box(
    frame: &mut Frame<Renderer>,
    cx: f32,
    y: f32,
    width: f32,
    height: f32,
    text: &str,
    fg: Color,
    border: Color,
) {
    let x = cx - width * 0.5;
    let rect = Rectangle::new(Point::new(x, y - height * 0.5), Size::new(width, height));
    frame.fill_rectangle(
        rect.position(),
        rect.size(),
        Color::from_rgba(0.0, 0.0, 0.0, 0.03),
    );
    let mut path = Builder::new();
    draw_rect_path(&mut path, &rect);
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(border)
            .with_width(1.0),
    );

    frame.fill_text(Text {
        content: text.to_string(),
        position: Point::new(cx, y),
        color: fg,
        size: iced_core::Pixels(FONT_SIZE),
        line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(FONT_SIZE + 2.0)),
        font: iced_core::Font::default(),
        max_width: width - 8.0,
        align_x: iced_core::alignment::Horizontal::Center.into(),
        align_y: iced_core::alignment::Vertical::Center,
        shaping: iced_widget::text::Shaping::Basic,
    });
}

fn draw_button(frame: &mut Frame<Renderer>, label: &str, pos: Point, bg: Color, fg: Color) {
    let rect = Rectangle::new(pos, Size::new(BUTTON_WIDTH, BUTTON_HEIGHT));
    frame.fill_rectangle(rect.position(), rect.size(), bg);
    let mut path = Builder::new();
    draw_rect_path(&mut path, &rect);
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(Color::from_rgba(0.0, 0.0, 0.0, 0.2))
            .with_width(1.0),
    );
    frame.fill_text(Text {
        content: label.to_string(),
        position: Point::new(pos.x + BUTTON_WIDTH * 0.5, pos.y + BUTTON_HEIGHT * 0.5),
        color: fg,
        size: iced_core::Pixels(FONT_SIZE),
        line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(FONT_SIZE + 2.0)),
        font: iced_core::Font::default(),
        max_width: BUTTON_WIDTH,
        align_x: iced_core::alignment::Horizontal::Center.into(),
        align_y: iced_core::alignment::Vertical::Center,
        shaping: iced_widget::text::Shaping::Basic,
    });
}

fn draw_triangle(frame: &mut Frame<Renderer>, center: Point, size: f32, left: bool, color: Color) {
    let mut path = Builder::new();
    let half = size * 0.5;
    if left {
        path.move_to(Point::new(center.x + half, center.y - half));
        path.line_to(Point::new(center.x - half, center.y));
        path.line_to(Point::new(center.x + half, center.y + half));
    } else {
        path.move_to(Point::new(center.x - half, center.y - half));
        path.line_to(Point::new(center.x + half, center.y));
        path.line_to(Point::new(center.x - half, center.y + half));
    }
    path.close();
    frame.fill(&path.build(), color);
}

/// 判断鼠标点击是否命中弹窗按钮。
pub fn popup_hit_test(local: Point, bounds: Rectangle) -> PopupHit {
    let cx = bounds.width * 0.5;
    let cy = bounds.height * 0.5;
    let rect = Rectangle::new(
        Point::new(cx - POPUP_WIDTH * 0.5, cy - POPUP_HEIGHT * 0.5),
        Size::new(POPUP_WIDTH, POPUP_HEIGHT),
    );
    if !rect.contains(local) {
        return PopupHit::Cancel;
    }

    let btn_y = rect.y + POPUP_HEIGHT - BUTTON_HEIGHT - 12.0;
    let ok_rect = Rectangle::new(
        Point::new(cx - BUTTON_WIDTH - 8.0, btn_y),
        Size::new(BUTTON_WIDTH, BUTTON_HEIGHT),
    );
    let cancel_rect = Rectangle::new(
        Point::new(cx + 8.0, btn_y),
        Size::new(BUTTON_WIDTH, BUTTON_HEIGHT),
    );

    if ok_rect.contains(local) {
        return PopupHit::Ok;
    }
    if cancel_rect.contains(local) {
        return PopupHit::Cancel;
    }

    // Choice 箭头区域
    let value_y = rect.y + HEADER_HEIGHT + ROW_HEIGHT + 4.0;
    let left_rect = Rectangle::new(
        Point::new(rect.x, value_y - ARROW_SIZE * 0.5),
        Size::new(ARROW_SIZE * 2.0, ARROW_SIZE),
    );
    let right_rect = Rectangle::new(
        Point::new(
            rect.x + POPUP_WIDTH - ARROW_SIZE * 2.0,
            value_y - ARROW_SIZE * 0.5,
        ),
        Size::new(ARROW_SIZE * 2.0, ARROW_SIZE),
    );
    if left_rect.contains(local) {
        return PopupHit::ChoicePrev;
    }
    if right_rect.contains(local) {
        return PopupHit::ChoiceNext;
    }

    PopupHit::None
}
