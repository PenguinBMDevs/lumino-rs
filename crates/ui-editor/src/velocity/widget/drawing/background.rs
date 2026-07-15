//! 背景绘制函数
use super::*;

/// 绘制面板背景（网格线 + 力度刻度）
pub fn draw_background(frame: &mut Frame<Renderer>, theme: &Theme, size: Size) {
    let width = size.width;
    let height = size.height;
    let draw_top = RESIZE_HANDLE_HEIGHT;
    let line_color = velocity_grid_line_color(theme);
    let text_color = velocity_text_color(theme);
    let velocity_levels = [0u8, 32, 64, 96, 127];

    for &v in &velocity_levels {
        let y = VelocityCanvas::velocity_to_y(v, height);
        let mut line_builder = path::Builder::new();
        line_builder.move_to(Point::new(PANEL_PADDING_X, y));
        line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
        frame.stroke(
            &line_builder.build(),
            canvas::Stroke::default()
                .with_color(line_color)
                .with_width(1.0),
        );

        frame.fill_text(canvas::Text {
            content: format!("{}", v),
            position: Point::new(4.0, y - 6.0),
            max_width: width,
            line_height: iced_core::text::LineHeight::Relative(1.0),
            size: iced_core::Pixels(9.0),
            color: text_color,
            font: iced_core::Font::DEFAULT,
            align_x: alignment::Horizontal::Left.into(),
            align_y: alignment::Vertical::Top,
            shaping: iced_core::text::Shaping::Basic,
        });
    }

    let border_color = velocity_border_color(theme);
    frame.fill_rectangle(
        Point::new(0.0, draw_top),
        Size::new(width, 1.0),
        border_color,
    );
}

/// 绘制顶部 resize 拖拽手柄
pub fn draw_resize_handle(frame: &mut Frame<Renderer>, theme: &Theme, size: Size, hovered: bool) {
    let handle_color = velocity_handle_bg_color(theme, hovered);
    let grab_bar_color = velocity_grab_bar_color(theme);

    frame.fill_rectangle(
        Point::new(0.0, 0.0),
        Size::new(size.width, RESIZE_HANDLE_HEIGHT),
        handle_color,
    );

    let bar_width = 40.0;
    let bar_height = 3.0;
    let bar_x = (size.width - bar_width) / 2.0;
    let bar_y = (RESIZE_HANDLE_HEIGHT - bar_height) / 2.0;
    frame.fill_rectangle(
        Point::new(bar_x, bar_y),
        Size::new(bar_width, bar_height),
        grab_bar_color,
    );
}
