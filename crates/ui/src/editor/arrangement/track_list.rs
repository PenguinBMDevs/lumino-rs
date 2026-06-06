//! 工程走带左侧音轨列表 Canvas —— 按 yinhe 风格绘制音轨名称和选中状态
//!
//! 与右侧走带 Canvas 共享 scroll_y，实现同步滚动。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Geometry, Program};

use crate::editor::grid::theme::ThemeExt;
use crate::{Message, Renderer, Theme};

/// 工程走带左侧音轨列表 Canvas
pub struct TrackListCanvas {
    /// 音轨列表：(id, name)
    pub tracks: Vec<(usize, String)>,
    /// 当前选中的音轨 ID
    pub selected_track: usize,
    /// 垂直滚动偏移
    pub scroll_y: f32,
    /// 每轨高度
    pub track_height: f32,
    /// 总高度
    pub total_height: f32,
}

impl TrackListCanvas {
    pub fn new(
        tracks: Vec<(usize, String)>,
        selected_track: usize,
        scroll_y: f32,
        track_height: f32,
        total_height: f32,
    ) -> Self {
        Self {
            tracks,
            selected_track,
            scroll_y,
            track_height,
            total_height,
        }
    }
}

impl Program<Message, Theme, Renderer> for TrackListCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &iced_widget::canvas::Event,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Option<iced_widget::canvas::Action<Message>> {
        // 鼠标滚轮滚动
        if let iced_widget::canvas::Event::Mouse(iced_core::mouse::Event::WheelScrolled { delta }) =
            event
        {
            use crate::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};
            let (_, dy) = match delta {
                iced_core::mouse::ScrollDelta::Lines { x, y } => {
                    (x * SCROLL_LINES_SCALE, y * SCROLL_LINES_SCALE)
                }
                iced_core::mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
            };
            let dy = dy.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
            // 注意：dy > 0 表示滚轮向下，scroll_y 应减小（内容向上滚动）
            return Some(iced_widget::canvas::Action::publish(
                Message::ArrangementScrollY(self.scroll_y - dy),
            ));
        }

        // 点击选轨
        if let iced_widget::canvas::Event::Mouse(iced_core::mouse::Event::ButtonPressed(
            iced_core::mouse::Button::Left,
        )) = event
        {
            if let Some(pos) = cursor.position() {
                let rel_y = pos.y - bounds.y + self.scroll_y;
                let clicked_idx = (rel_y / self.track_height) as usize;
                if let Some((id, _)) = self.tracks.get(clicked_idx) {
                    return Some(iced_widget::canvas::Action::publish(
                        crate::sidebar::Event::track_selected(*id),
                    ));
                }
            }
        }
        None
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let canvas_w = bounds.size().width;
        let canvas_h = bounds.size().height;
        let palette = theme.extended_palette();
        let is_light = theme.is_light();

        // 计算可见范围
        let first = (self.scroll_y / self.track_height).floor() as usize;
        let visible_count = (canvas_h / self.track_height).ceil() as usize + 2;
        let last = (first + visible_count).min(self.tracks.len());

        for idx in first..last {
            let Some((track_id, name)) = self.tracks.get(idx) else {
                continue;
            };

            let y = idx as f32 * self.track_height - self.scroll_y;

            // 视锥裁剪
            if y + self.track_height < 0.0 || y > canvas_h {
                continue;
            }

            let is_selected = *track_id == self.selected_track;

            // 交替背景色
            let bg_color = if is_selected {
                palette.primary.weak.color
            } else if idx % 2 == 0 {
                if is_light {
                    Color::from_rgb(0.97, 0.97, 0.97)
                } else {
                    Color::from_rgb(0.20, 0.20, 0.20)
                }
            } else {
                if is_light {
                    Color::from_rgb(0.94, 0.94, 0.94)
                } else {
                    Color::from_rgb(0.15, 0.15, 0.15)
                }
            };

            frame.fill_rectangle(
                Point::new(0.0, y),
                Size::new(canvas_w, self.track_height),
                bg_color,
            );

            // 音轨名称文字
            let text_color = if is_selected {
                palette.primary.strong.color
            } else {
                palette.background.base.text
            };

            // 绘制音轨名称
            frame.fill_text(iced_widget::canvas::Text {
                content: name.clone(),
                position: Point::new(8.0, y + self.track_height * 0.5),
                color: text_color,
                size: iced_core::Pixels(13.0),
                line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(13.0 * 1.2)),
                font: iced_core::Font::default(),
                max_width: f32::INFINITY,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Center,
                shaping: iced_widget::text::Shaping::Basic,
            });
        }

        // 绘制底部分隔线
        let line_color = if is_light {
            Color::from_rgba(0.0, 0.0, 0.0, 0.08)
        } else {
            Color::from_rgba(1.0, 1.0, 1.0, 0.06)
        };
        let mut lb = iced_widget::canvas::path::Builder::new();
        lb.move_to(Point::new(canvas_w - 1.0, 0.0));
        lb.line_to(Point::new(canvas_w - 1.0, canvas_h.min(self.total_height)));
        frame.stroke(
            &lb.build(),
            iced_widget::canvas::Stroke::default()
                .with_color(line_color)
                .with_width(1.0),
        );

        vec![frame.into_geometry()]
    }
}
