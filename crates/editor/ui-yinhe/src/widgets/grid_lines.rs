//! 网格线 — yinhe `widgets/grid_lines.rs:512` 的 iced 迁移桩
//!
//! 原 `egui` 实现按拍号段以不同密度绘制小节/拍/十六分线；iced 桩以
//! `canvas::Program` 重建，主轴语义对齐 `Orientation`（横向=竖线，纵向=横线），
//! 颜色走 `Theme`。

use iced_core::mouse::Cursor;
use iced_core::{Length, Point, Rectangle, Size};
use iced_widget::canvas::{Cache, Frame, Geometry, Program};

use lumino_ui_core::{Renderer, Theme};

/// 网格颜色集（对齐 yinhe `GridColors`）
#[derive(Debug, Clone)]
pub struct GridColors {
    pub measure: iced_core::Color,
    pub beat: iced_core::Color,
    pub sub_beat: Option<iced_core::Color>,
    pub tick: Option<iced_core::Color>,
}

impl GridColors {
    pub fn pianoroll(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        Self {
            measure: palette.background.strong.color,
            beat: palette.background.strong.color,
            sub_beat: Some(palette.background.weak.color.scale_alpha(0.6)),
            tick: Some(palette.background.weak.color.scale_alpha(0.35)),
        }
    }

    pub fn arrangement(theme: &Theme) -> Self {
        let palette = theme.extended_palette();
        Self {
            measure: palette.background.strong.color,
            beat: palette.background.strong.color,
            sub_beat: None,
            tick: None,
        }
    }
}

/// 网格方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridOrientation {
    Horizontal,
    Vertical,
}

/// 网格线 Canvas Program
pub struct GridLines {
    pub tpb: u32,
    pub pixels_per_tick: f32,
    pub scroll: f32,
    pub left_panel_width: f32,
    pub colors: GridColors,
    pub orientation: GridOrientation,
}

impl Program<lumino_ui_core::Message, Theme, Renderer> for GridLines {
    type State = Cache<Renderer>;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        if self.pixels_per_tick <= 0.001 {
            return vec![frame.into_geometry()];
        }

        // 简化：每小节/拍画线（与 yinhe 的合并/密度判定对齐的简化版）
        let ticks_per_measure = 1920u32;
        let ticks_per_beat = 480u32;
        let tpb_f = self.tpb as f32;

        // 可见 tick 区间
        let tick_start = (self.scroll / self.pixels_per_tick).max(0.0) as u32;
        let tick_end =
            ((self.scroll + bounds.width) / self.pixels_per_tick) as u32 + ticks_per_measure;

        let mut tick = (tick_start / ticks_per_beat) * ticks_per_beat;
        while tick <= tick_end {
            let x = tick as f32 * self.pixels_per_tick - self.scroll + self.left_panel_width;
            let in_bounds = x >= bounds.x && x <= bounds.x + bounds.width;
            if in_bounds {
                let is_measure = tick % ticks_per_measure == 0;
                let is_beat = tick % ticks_per_beat == 0 && !is_measure;
                let (color, width) = if is_measure {
                    (self.colors.measure, 2.0)
                } else if is_beat {
                    (self.colors.beat, 1.0)
                } else if let Some(c) = self.colors.sub_beat {
                    (c, 1.0)
                } else {
                    tick += ticks_per_beat;
                    continue;
                };
                let rect = match self.orientation {
                    GridOrientation::Horizontal => Rectangle::new(
                        Point::new(x - bounds.x - width / 2.0, 0.0),
                        Size::new(width, bounds.height),
                    ),
                    GridOrientation::Vertical => Rectangle::new(
                        Point::new(0.0, x - bounds.y - width / 2.0),
                        Size::new(bounds.width, width),
                    ),
                };
                frame.fill_rectangle(rect.position(), rect.size(), color);
            }
            tick += ticks_per_beat;
        }

        vec![frame.into_geometry()]
    }
}

pub fn view<'a>(
    tpb: u32,
    pixels_per_tick: f32,
    scroll: f32,
    left_panel_width: f32,
    colors: GridColors,
    orientation: GridOrientation,
) -> iced_core::Element<'a, lumino_ui_core::Message, Theme, Renderer> {
    use iced_widget::canvas::Canvas;
    Canvas::new(GridLines {
        tpb,
        pixels_per_tick,
        scroll,
        left_panel_width,
        colors,
        orientation,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
