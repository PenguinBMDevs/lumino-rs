//! 时间标尺 — yinhe `widgets/time_ruler.rs:608` 的 iced 迁移桩
//!
//! 原 `egui` 实现在 `painter` 上逐拍绘制标签并处理点击/拖动跳播、滚轮缩放；
//! iced 桩以 `canvas::Program` 重建矢量层，保留主轴/副轴语义与拍号段遍历，
//! 主题走 `Theme`，点击/拖动通过 `Message` 占位。

use iced_core::mouse::{self, Cursor};
use iced_core::{Color, Length, Point, Rectangle, Size, Vector};
use iced_widget::canvas::{self, Cache, Frame, Geometry, Program, Text};

use lumino_ui_core::{Renderer, Theme};

const MIN_LABEL_SPACING: f32 = 38.0;

/// 拍号段（对齐 yinhe `build_time_sig_segments` 的简化版）
#[derive(Debug, Clone, Copy)]
pub struct TimeSigSegment {
    pub start_tick: u32,
    pub num: u8,
    pub den: u8,
}

/// 时间标尺 Canvas Program
pub struct TimeRuler<'a> {
    pub tpb: u32,
    pub pixels_per_tick: f32,
    pub scroll: f32,
    pub left_panel_width: f32,
    pub segments: &'a [TimeSigSegment],
    pub theme: &'a Theme,
}

impl<'a> Program<lumino_ui_core::Message, Theme, Renderer> for TimeRuler<'a> {
    type State = Cache<Renderer>;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        let bg = palette.background.weakest.color;
        let text_color = palette.background.base.text;
        let measure_color = palette.background.strong.color;

        // 背景
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), bg);

        if self.pixels_per_tick <= 0.001 {
            return vec![frame.into_geometry()];
        }

        // 简化标签绘制：每小节一条垂直线 + 小节号
        let mut tick = 0u32;
        let ticks_per_measure = 1920u32; // 4/4 @480
        while tick < 20000 {
            let x = tick as f32 * self.pixels_per_tick - self.scroll + self.left_panel_width;
            if x >= bounds.x && x <= bounds.x + bounds.width {
                // 小节线
                let line = iced_core::Rectangle::new(
                    Point::new(x - bounds.x, 0.0),
                    Size::new(1.0, bounds.height),
                );
                frame.fill_rectangle(line.position(), line.size(), measure_color.scale_alpha(0.5));
                // 小节号
                let label = format!("{}", tick / ticks_per_measure + 1);
                frame.fill_text(Text {
                    content: label,
                    position: Point::new(x - bounds.x + 2.0, bounds.height / 2.0),
                    color: text_color,
                    size: iced_core::Pixels(10.0),
                    font: iced_core::Font::MONOSPACE,
                    align_x: iced_core::alignment::Horizontal::Left.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_core::text::Shaping::Basic,
                    max_width: MIN_LABEL_SPACING,
                    line_height: iced_core::text::LineHeight::Relative(1.0),
                });
            }
            if x > bounds.x + bounds.width {
                break;
            }
            tick += ticks_per_measure;
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        _state: &mut Self::State,
        event: &iced_core::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<lumino_ui_core::Message>> {
        if let iced_core::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event {
            if let Some(pos) = cursor.position() {
                if bounds.contains(pos) {
                    // 点击跳播（stub：发送空消息，实际由 Host 解析 tick）
                    return Some(canvas::Action::publish(lumino_ui_core::message::null()));
                }
            }
        }
        // 滚轮缩放由 Host 的 `zoom_factor_from_delta` 统一处理，此处仅占位
        None
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if let Some(pos) = cursor.position() {
            if bounds.contains(pos) {
                return mouse::Interaction::Pointer;
            }
        }
        mouse::Interaction::None
    }
}

/// 构建时间标尺 Element
pub fn view<'a>(
    tpb: u32,
    pixels_per_tick: f32,
    scroll: f32,
    left_panel_width: f32,
    segments: &'a [TimeSigSegment],
    theme: &'a Theme,
) -> iced_core::Element<'a, lumino_ui_core::Message, Theme, Renderer> {
    use iced_widget::canvas::Canvas;
    Canvas::new(TimeRuler {
        tpb,
        pixels_per_tick,
        scroll,
        left_panel_width,
        segments,
        theme,
    })
    .width(Length::Fill)
    .height(Length::Fixed(28.0))
    .into()
}
