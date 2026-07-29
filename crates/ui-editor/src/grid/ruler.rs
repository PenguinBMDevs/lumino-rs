//! 时间轴标尺绘制

use super::loop_range::LoopRange;
use super::theme::ThemeExt;
use crate::Editor;
use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};
use lumino_ui_constants::editor::MEASURE_NUMBER_FONT_SIZE;
use lumino_ui_core::Renderer;

/// 绘制时间轴标尺到 Geometry（用于 Canvas 绘制）
pub fn draw_to_geometry(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(editor, &mut frame, bounds, theme);
    frame.into_geometry()
}

/// 绘制时间轴标尺（小节号显示区域）
pub fn draw(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;

    let time_signatures = editor.editor_state.data.time_signatures.as_slice();
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

    // 绘制小节号和刻度线（按拍号变化分段）
    let mut measure_iter = MeasureIterator::new(time_signatures, ppq, start_tick.max(0.0));
    while let Some((measure_tick, measure_number)) = measure_iter.next() {
        if measure_tick > end_tick {
            break;
        }

        let screen_x = (measure_tick * view.zoom_x) - view.scroll_x + keyboard_width;

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
    }

    // 绘制循环区域（在刻度线之上）
    if let Some(loop_range) = &editor.loop_range
        && loop_range.enabled()
    {
        let loop_view = LoopRangeViewParams {
            keyboard_width,
            scroll_x: view.scroll_x,
            zoom_x: view.zoom_x,
            ruler_height,
            bounds_width: bounds.width,
        };
        draw_loop_range(frame, loop_range, &loop_view, theme);
    }
}

/// 循环区域颜色常量
const LOOP_FILL_ALPHA: f32 = 0.25;
const LOOP_BORDER_ALPHA: f32 = 0.7;
const LOOP_HANDLE_WIDTH: f32 = 6.0;
const LOOP_HANDLE_HEIGHT: f32 = 16.0;

/// 绘制循环区域高亮和标记点的视图参数
struct LoopRangeViewParams {
    keyboard_width: f32,
    scroll_x: f32,
    zoom_x: f32,
    ruler_height: f32,
    bounds_width: f32,
}

/// 绘制循环区域高亮和标记点
fn draw_loop_range(
    frame: &mut Frame<Renderer>,
    loop_range: &LoopRange,
    view: &LoopRangeViewParams,
    theme: &lumino_ui_core::Theme,
) {
    let Some((start_x, end_x)) =
        loop_range.to_screen_coords(view.keyboard_width, view.scroll_x, view.zoom_x)
    else {
        return;
    };

    // 如果循环区域完全不在可视范围内，不绘制
    if end_x < view.keyboard_width || start_x > view.bounds_width {
        return;
    }

    let visible_start = start_x.max(view.keyboard_width);
    let visible_end = end_x.min(view.bounds_width);

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
        Size::new(visible_end - visible_start, view.ruler_height - 4.0),
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
    if start_x >= view.keyboard_width && start_x <= view.bounds_width {
        draw_handle(frame, start_x, view.ruler_height, true, border_color);
    }

    // 绘制结束手柄（右侧三角形/竖条）
    if end_x >= view.keyboard_width && end_x <= view.bounds_width {
        draw_handle(frame, end_x, view.ruler_height, false, border_color);
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

/// 计算 tick 位置所在拍号段（空列表时回退到 4/4）
fn time_signature_at(tick: f32, time_signatures: &[(u32, u8, u8)]) -> (u8, u8) {
    let mut active = (4_u8, 4_u8);
    for &(ts_tick, num, den) in time_signatures {
        if tick >= ts_tick as f32 {
            active = (num, den);
        } else {
            break;
        }
    }
    active
}

/// 计算给定拍号下的每小节 tick 数
fn ticks_per_measure(ppq: f32, numerator: u8, denominator: u8) -> f32 {
    let beat_ticks = ppq * 4.0 / denominator.max(1) as f32;
    beat_ticks * numerator.max(1) as f32
}

/// 小节边界迭代器，按拍号变化分段生成小节起始 tick 与编号
struct MeasureIterator<'a> {
    time_signatures: &'a [(u32, u8, u8)],
    ppq: f32,
    current_tick: f32,
    measure_number: u32,
    ts_index: usize,
    measure_ticks: f32,
}

impl<'a> MeasureIterator<'a> {
    fn new(time_signatures: &'a [(u32, u8, u8)], ppq: f32, start_tick: f32) -> Self {
        let (num, den) = time_signature_at(0.0, time_signatures);
        let mut iter = Self {
            time_signatures,
            ppq,
            current_tick: 0.0,
            measure_number: 1,
            ts_index: 0,
            measure_ticks: ticks_per_measure(ppq, num, den),
        };
        iter.advance_to(start_tick);
        iter
    }

    fn advance_to(&mut self, target_tick: f32) {
        while self.current_tick < target_tick {
            self.step();
        }
    }

    fn step(&mut self) {
        let next_measure_tick = self.current_tick + self.measure_ticks;
        if let Some((next_ts_tick, _, _)) = self.time_signatures.get(self.ts_index + 1) {
            let next_ts_tick = *next_ts_tick as f32;
            if next_measure_tick >= next_ts_tick && self.current_tick < next_ts_tick {
                self.ts_index += 1;
                let (num, den) = time_signature_at(next_ts_tick, self.time_signatures);
                self.measure_ticks = ticks_per_measure(self.ppq, num, den);
                self.current_tick = next_ts_tick;
                self.measure_number += 1;
                return;
            }
        }
        self.current_tick = next_measure_tick;
        self.measure_number += 1;
    }

    fn next(&mut self) -> Option<(f32, u32)> {
        let result = (self.current_tick, self.measure_number);
        self.step();
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_signature_at_empty_defaults_to_4_4() {
        assert_eq!(time_signature_at(100.0, &[]), (4, 4));
    }

    #[test]
    fn test_time_signature_at_returns_active_signature() {
        let signatures = [(0, 3, 4), (1920, 4, 4)];
        assert_eq!(time_signature_at(0.0, &signatures), (3, 4));
        assert_eq!(time_signature_at(1919.0, &signatures), (3, 4));
        assert_eq!(time_signature_at(1920.0, &signatures), (4, 4));
    }

    #[test]
    fn test_measure_iterator_4_4() {
        // ppq = 480, 4/4 -> 1920 ticks/measure
        let mut iter = MeasureIterator::new(&[(0, 4, 4)], 480.0, 0.0);
        assert_eq!(iter.next(), Some((0.0, 1)));
        assert_eq!(iter.next(), Some((1920.0, 2)));
        assert_eq!(iter.next(), Some((3840.0, 3)));
    }

    #[test]
    fn test_measure_iterator_advance_to_start() {
        // ppq = 480, 4/4, start at tick 4000
        let mut iter = MeasureIterator::new(&[(0, 4, 4)], 480.0, 4000.0);
        // measure 3 spans [3840, 5760), so the first visible boundary is measure 4 at 5760
        assert_eq!(iter.next(), Some((5760.0, 4)));
        assert_eq!(iter.next(), Some((7680.0, 5)));
    }

    #[test]
    fn test_measure_iterator_time_signature_change() {
        // 4/4 for one measure (1920 ticks), then 3/4 (1440 ticks/measure)
        let signatures = [(0, 4, 4), (1920, 3, 4)];
        let mut iter = MeasureIterator::new(&signatures, 480.0, 0.0);
        assert_eq!(iter.next(), Some((0.0, 1)));
        assert_eq!(iter.next(), Some((1920.0, 2)));
        assert_eq!(iter.next(), Some((1920.0 + 1440.0, 3)));
        assert_eq!(iter.next(), Some((1920.0 + 2880.0, 4)));
    }

    #[test]
    fn test_ticks_per_measure_different_denominators() {
        // ppq = 480, 6/8 -> beat = 480 * 4 / 8 = 240, measure = 240 * 6 = 1440
        assert!((ticks_per_measure(480.0, 6, 8) - 1440.0).abs() < f32::EPSILON);
    }
}
