//! 工程走带视图顶部时间标尺（小节号文本层）。
//!
//! 走带主体由 WGPU `ArrangementRenderer` 绘制，它只画背景/lane/网格线/音符/
//! 指示线，没有文字层（`arrangement.wgsl` 纯色块 SDF）。本 Canvas 补上
//! 24–28px 的顶部标尺：小节号文本 + 刻度线 + 点击/拖拽跳转 + 滚轮缩放，
//! 与钢琴卷帘标尺（`grid/ruler.rs`）共用**已去重**的 `MeasureIterator`，
//! 保证同 tick 重复拍号不会让走带小节号再次从 2 起跳（#146）。
//!
//! 坐标约定：Canvas 局部 x=0 对齐走带区域左缘（布局上与左侧 160px 音轨列表
//! 并列），`tick_to_screen_x = tick * zoom_x - scroll_x`，与 WGPU 网格同源。

use iced_core::{Point, Rectangle, alignment, mouse};
use iced_widget::canvas::{self, Frame, Geometry, Path, Program, Stroke, Text};
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor::MEASURE_NUMBER_FONT_SIZE;

use crate::arrangement::ArrangementViewport;
use crate::grid::ruler::MeasureIterator;
use crate::grid::theme::ThemeExt;
use crate::{Message, Theme};

/// 相邻小节号标签的最小间距（像素）；更密时只画刻度线，避免文字糊成一片。
const MIN_LABEL_SPACING: f32 = 38.0;

/// 标尺交互状态：左键按住后拖动持续跳转。
#[derive(Debug, Default)]
pub struct ArrangementRulerState {
    dragging: bool,
}

/// 工程走带顶部时间标尺 Canvas。
pub struct ArrangementRulerCanvas {
    /// 走带视口（每帧从 `arrangement_view.viewport` 克隆）
    pub viewport: ArrangementViewport,
    /// 每四分音符 tick 数
    pub ppq: u16,
    /// 拍号变化列表（原样传入；迭代器内部负责同 tick 去重）
    pub time_signatures: Vec<(u32, u8, u8)>,
}

impl ArrangementRulerCanvas {
    /// Canvas 局部 x（屏幕像素）→ tick；负值钳到 0，越界钳到 `total_ticks`。
    fn tick_at_x(&self, x: f32, bounds: Rectangle) -> f64 {
        let local = (x - bounds.x).max(0.0) as f64;
        let scroll = self.viewport.scroll_x.max(0.0) as f64;
        let zoom = self.viewport.zoom_x.max(1e-6) as f64;
        let tick = ((local + scroll) / zoom).max(0.0);
        tick.min(self.viewport.total_ticks.max(1) as f64)
    }
}

impl Program<Message, Theme, Renderer> for ArrangementRulerCanvas {
    type State = ArrangementRulerState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        puffin::profile_function!();

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position()?;
                if !bounds.contains(pos) {
                    return None;
                }
                state.dragging = true;
                Some(canvas::Action::publish(Message::ArrangementCursorSet(
                    self.tick_at_x(pos.x, bounds),
                )))
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if !state.dragging {
                    return None;
                }
                let pos = cursor.position()?;
                Some(canvas::Action::publish(Message::ArrangementCursorSet(
                    self.tick_at_x(pos.x, bounds),
                )))
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                None
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let pos = cursor.position()?;
                if !bounds.contains(pos) {
                    return None;
                }
                let factor = crate::zoom::zoom_factor_from_delta(delta)?;
                let local_x = (pos.x - bounds.x).max(0.0);
                Some(canvas::Action::publish(Message::ArrangementZoomX {
                    zoom: self.viewport.zoom_x * factor,
                    fixed_ratio: crate::zoom::fixed_ratio_from_viewport(local_x, 0.0, bounds.width),
                }))
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        puffin::profile_function!();

        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), theme.ruler_background_color());

        let border_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(theme.border_color());
        frame.stroke(
            &Path::line(
                Point::new(0.0, bounds.height - 0.5),
                Point::new(bounds.width, bounds.height - 0.5),
            ),
            border_stroke,
        );

        let view = &self.viewport;
        let zoom = view.zoom_x.max(1e-6);
        let start_tick = (view.scroll_x / zoom).max(0.0);
        let end_tick = (view.scroll_x + bounds.width) / zoom;
        let text_color = theme.text_color();

        let mut iter = MeasureIterator::new(&self.time_signatures, self.ppq as f32, start_tick);
        let mut last_label_x = f32::NEG_INFINITY;
        while let Some((tick, measure_number)) = iter.next() {
            if tick > end_tick {
                break;
            }
            let screen_x = tick * zoom - view.scroll_x;
            if screen_x < 0.0 || screen_x > bounds.width {
                continue;
            }

            frame.stroke(
                &Path::line(
                    Point::new(screen_x, 0.0),
                    Point::new(screen_x, bounds.height),
                ),
                border_stroke,
            );

            if screen_x - last_label_x < MIN_LABEL_SPACING {
                continue;
            }
            let text = Text {
                content: measure_number.to_string(),
                position: Point::new(screen_x + 4.0, 4.0),
                max_width: (bounds.width - screen_x - 4.0).max(0.0),
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(MEASURE_NUMBER_FONT_SIZE),
                color: text_color,
                font: lumino_ui_core::font::ui_font(),
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(text);
            last_label_x = screen_x;
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::Pointer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::{Size, mouse};

    fn canvas(scroll_x: f32, zoom_x: f32) -> ArrangementRulerCanvas {
        ArrangementRulerCanvas {
            viewport: ArrangementViewport {
                scroll_x,
                zoom_x,
                total_ticks: 100_000,
                ..ArrangementViewport::default()
            },
            ppq: 960,
            time_signatures: vec![(0, 4, 4)],
        }
    }

    fn bounds() -> Rectangle {
        Rectangle::new(Point::new(0.0, 0.0), Size::new(800.0, 28.0))
    }

    #[test]
    fn test_tick_at_x_accounts_for_scroll_and_zoom() {
        let canvas = canvas(100.0, 2.0);
        // 局部 x=100：tick = (100 + 100) / 2 = 100
        assert!((canvas.tick_at_x(100.0, bounds()) - 100.0).abs() < 1e-6);
        // 局部 x=0：tick = (0 + 100) / 2 = 50
        assert!((canvas.tick_at_x(0.0, bounds()) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_tick_at_x_clamps_negative_and_total() {
        let canvas = canvas(0.0, 2.0);
        assert_eq!(canvas.tick_at_x(-10.0, bounds()), 0.0);
        assert_eq!(canvas.tick_at_x(1.0e9, bounds()), 100_000.0);
    }

    #[test]
    fn test_left_click_publishes_arrangement_cursor_set() {
        let mut state = ArrangementRulerState::default();
        let canvas = canvas(100.0, 2.0);
        let action = canvas
            .update(
                &mut state,
                &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds(),
                mouse::Cursor::Available(Point::new(100.0, 14.0)),
            )
            .expect("标尺左键应产生跳转动作");
        let (message, _, _) = action.into_inner();
        match message {
            Some(Message::ArrangementCursorSet(tick)) => {
                assert!((tick - 100.0).abs() < 1e-6, "tick = {tick}");
            }
            other => panic!("应发 ArrangementCursorSet，实际为: {other:?}"),
        }
    }

    #[test]
    fn test_drag_after_press_keeps_seeking() {
        let mut state = ArrangementRulerState::default();
        let canvas = canvas(100.0, 2.0);
        let _ = canvas.update(
            &mut state,
            &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds(),
            mouse::Cursor::Available(Point::new(100.0, 14.0)),
        );
        let action = canvas
            .update(
                &mut state,
                &canvas::Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(200.0, 14.0),
                }),
                bounds(),
                mouse::Cursor::Available(Point::new(200.0, 14.0)),
            )
            .expect("按住拖动应继续跳转");
        let (message, _, _) = action.into_inner();
        match message {
            Some(Message::ArrangementCursorSet(tick)) => {
                assert!((tick - 150.0).abs() < 1e-6, "tick = {tick}");
            }
            other => panic!("应发 ArrangementCursorSet，实际为: {other:?}"),
        }
    }
}
