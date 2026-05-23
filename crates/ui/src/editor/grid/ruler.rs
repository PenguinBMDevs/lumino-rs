//! 时间轴标尺绘制

use super::loop_range::LoopRange;
use super::theme::ThemeExt;
use crate::Renderer;
use crate::constants::editor::MEASURE_NUMBER_FONT_SIZE;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};

/// 绘制时间轴标尺到 Geometry（用于 Canvas 绘制）
pub fn draw_to_geometry(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
    theme: &crate::Theme,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(editor, &mut frame, bounds, theme);
    frame.into_geometry()
}

/// 绘制时间轴标尺（小节号显示区域）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &crate::Theme) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;

    let measure_ticks = ppq * 4.0;
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

    // 绘制标尺背景
    let ruler_bg_color = theme.ruler_background_color();
    let ruler_rect = Rectangle::new(
        Point::new(keyboard_width, 0.0),
        Size::new(bounds.width - keyboard_width, ruler_height),
    );
    let ruler_path = Path::rectangle(ruler_rect.position(), ruler_rect.size());
    frame.fill(&ruler_path, ruler_bg_color);

    // 绘制标尺边框
    let border_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.border_color());
    frame.stroke(&ruler_path, border_stroke);

    let text_color = theme.text_color();

    // 绘制小节号和刻度线
    let mut current_measure_tick = ((start_tick / measure_ticks).floor() * measure_ticks).max(0.0);
    let mut measure_number = (current_measure_tick / measure_ticks).ceil() as u32;

    while current_measure_tick <= end_tick {
        let screen_x = (current_measure_tick * view.zoom_x) - view.scroll_x + keyboard_width;

        if screen_x >= keyboard_width && screen_x <= bounds.width {
            // 绘制小节号文本
            let measure_text = Text {
                content: measure_number.to_string(),
                position: Point::new(screen_x + 4.0, 4.0),
                max_width: bounds.width - keyboard_width,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(MEASURE_NUMBER_FONT_SIZE),
                color: text_color,
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(measure_text);

            // 绘制刻度线
            let tick_stroke = Stroke::default()
                .with_width(1.0)
                .with_color(theme.border_color());
            let tick_path = Path::line(
                Point::new(screen_x, 0.0),
                Point::new(screen_x, ruler_height),
            );
            frame.stroke(&tick_path, tick_stroke);
        }

        current_measure_tick += measure_ticks;
        measure_number += 1;
    }

    // 绘制循环区域（在刻度线之上）
    if let Some(loop_range) = &editor.loop_range
        && loop_range.enabled()
    {
        draw_loop_range(
            frame,
            loop_range,
            keyboard_width,
            view.scroll_x,
            view.zoom_x,
            ruler_height,
            bounds.width,
            theme,
        );
    }
}

/// 循环区域颜色常量
const LOOP_FILL_ALPHA: f32 = 0.25;
const LOOP_BORDER_ALPHA: f32 = 0.7;
const LOOP_HANDLE_WIDTH: f32 = 6.0;
const LOOP_HANDLE_HEIGHT: f32 = 16.0;

/// 绘制循环区域高亮和标记点
fn draw_loop_range(
    frame: &mut Frame<Renderer>,
    loop_range: &LoopRange,
    keyboard_width: f32,
    scroll_x: f32,
    zoom_x: f32,
    ruler_height: f32,
    bounds_width: f32,
    theme: &crate::Theme,
) {
    let Some((start_x, end_x)) = loop_range.to_screen_coords(keyboard_width, scroll_x, zoom_x)
    else {
        return;
    };

    // 如果循环区域完全不在可视范围内，不绘制
    if end_x < keyboard_width || start_x > bounds_width {
        return;
    }

    let visible_start = start_x.max(keyboard_width);
    let visible_end = end_x.min(bounds_width);

    if visible_end <= visible_start {
        return;
    }

    // 获取主题主色调作为循环区域颜色
    let palette = theme.extended_palette();
    let primary_color = palette.primary.weak.color;

    // 绘制半透明背景填充
    let fill_color = iced_core::Color {
        r: primary_color.r,
        g: primary_color.g,
        b: primary_color.b,
        a: LOOP_FILL_ALPHA,
    };

    let loop_rect = Rectangle::new(
        Point::new(visible_start, 2.0),
        Size::new(visible_end - visible_start, ruler_height - 4.0),
    );

    let fill_path = Path::rectangle(loop_rect.position(), loop_rect.size());
    frame.fill(&fill_path, fill_color);

    // 绘制边框
    let border_color = iced_core::Color {
        r: primary_color.r,
        g: primary_color.g,
        b: primary_color.b,
        a: LOOP_BORDER_ALPHA,
    };
    let border_stroke = Stroke::default().with_width(2.0).with_color(border_color);
    frame.stroke(&fill_path, border_stroke);

    // 绘制起始手柄（左侧三角形/竖条）
    if start_x >= keyboard_width && start_x <= bounds_width {
        draw_handle(frame, start_x, ruler_height, true, border_color);
    }

    // 绘制结束手柄（右侧三角形/竖条）
    if end_x >= keyboard_width && end_x <= bounds_width {
        draw_handle(frame, end_x, ruler_height, false, border_color);
    }
}

/// 绘制拖拽手柄
fn draw_handle(
    frame: &mut Frame<Renderer>,
    x: f32,
    ruler_height: f32,
    _is_start: bool,
    color: iced_core::Color,
) {
    let handle_y = (ruler_height - LOOP_HANDLE_HEIGHT) / 2.0;

    // 竖直矩形手柄
    let handle_rect = Rectangle::new(
        Point::new(x - LOOP_HANDLE_WIDTH / 2.0, handle_y),
        Size::new(LOOP_HANDLE_WIDTH, LOOP_HANDLE_HEIGHT),
    );
    let handle_path = Path::rectangle(handle_rect.position(), handle_rect.size());

    // 手柄使用更深的颜色
    let handle_fill = iced_core::Color {
        r: color.r,
        g: color.g,
        b: color.b,
        a: 0.9,
    };
    frame.fill(&handle_path, handle_fill);

    // 手柄边框
    let handle_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(iced_core::Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.5,
        });
    frame.stroke(&handle_path, handle_stroke);
}
