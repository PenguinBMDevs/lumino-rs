//! 音轨总览 Canvas —— 绘制走带区域的分隔线和音轨区域背景

use iced_core::{Color, Point, Rectangle, Size, mouse};
use iced_widget::canvas::{Action, Event, Frame, Geometry, Program, path};

use crate::editor::grid::theme::ThemeExt;
use crate::{Message, Renderer, Theme};

/// 音轨总览画布 —— 绘制横向分隔线和音轨区域背景
pub struct ArrangementCanvas {
    /// 音轨数量
    track_count: usize,
    /// 每轨高度（像素）
    track_height: f32,
}

impl ArrangementCanvas {
    /// 创建新的走带区域 Canvas
    pub fn new(track_count: usize, track_height: f32) -> Self {
        Self {
            track_count,
            track_height,
        }
    }
}

impl Program<Message, Theme, Renderer> for ArrangementCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        _event: &Event,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let offset = Point::new(bounds.x, bounds.y);
        let size = iced_core::Size::new(bounds.width, bounds.height);

        Some(Action::publish(Message::ArrangementCanvasBoundsChanged {
            offset,
            size,
        }))
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = bounds.size();

        let _palette = theme.extended_palette();
        let is_light = theme.is_light();

        // 绘制每个音轨的区域背景和分隔线
        for track_idx in 0..self.track_count {
            let y = track_idx as f32 * self.track_height;

            // 交替背景色
            let bg_color = if track_idx % 2 == 0 {
                if is_light {
                    Color::from_rgb(0.97, 0.97, 0.97)
                } else {
                    Color::from_rgb(0.18, 0.18, 0.18)
                }
            } else {
                if is_light {
                    Color::from_rgb(0.94, 0.94, 0.94)
                } else {
                    Color::from_rgb(0.15, 0.15, 0.15)
                }
            };

            // 填充音轨区域背景
            frame.fill_rectangle(
                Point::new(0.0, y),
                Size::new(size.width, self.track_height),
                bg_color,
            );

            // 绘制底部分隔线
            let line_color = if is_light {
                Color::from_rgba(0.0, 0.0, 0.0, 0.08)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.06)
            };

            let mut line_builder = path::Builder::new();
            line_builder.move_to(Point::new(0.0, y + self.track_height));
            line_builder.line_to(Point::new(size.width, y + self.track_height));
            let line_path = line_builder.build();

            frame.stroke(
                &line_path,
                iced_widget::canvas::Stroke::default()
                    .with_color(line_color)
                    .with_width(1.0),
            );
        }

        // 绘制垂直时间轴标线（每小节一条）
        let bar_color = if is_light {
            Color::from_rgba(0.0, 0.0, 0.0, 0.06)
        } else {
            Color::from_rgba(1.0, 1.0, 1.0, 0.04)
        };
        const BAR_WIDTH_PX: f32 = 120.0; // 每小节默认宽度
        let bar_count = (size.width / BAR_WIDTH_PX).ceil() as usize;
        for i in 1..bar_count {
            let x = i as f32 * BAR_WIDTH_PX;
            let mut bar_builder = path::Builder::new();
            bar_builder.move_to(Point::new(x, 0.0));
            bar_builder.line_to(Point::new(x, size.height));
            let bar_path = bar_builder.build();

            frame.stroke(
                &bar_path,
                iced_widget::canvas::Stroke::default()
                    .with_color(bar_color)
                    .with_width(1.0),
            );
        }

        vec![frame.into_geometry()]
    }
}
