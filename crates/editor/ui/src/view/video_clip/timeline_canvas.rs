//! 剪辑带时间轴 Canvas（对标钢琴卷帘「内容区自绘 + 外挂滚动条」结构）
//!
//! 自绘内容：秒标尺、视频/音频轨道条、走带指示线（固定于区域前端
//! [`layout::PLAYHEAD_X`](super::layout::PLAYHEAD_X) 像素处）。
//! 滚动与缩放由底部 `ScrollbarWidget`（卷帘同款）驱动，
//! 本 Canvas 仅消费滚轮事件发射缩放/滚动消息。

use iced_core::mouse;
use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Frame, Geometry, Program, Text};

use lumino_ui_core::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};

use crate::editor::zoom::{fixed_ratio_from_viewport, zoom_factor_from_delta};
use crate::message::VideoClipAction;
use crate::{Message, Renderer, Theme};

/// 每秒基准像素密度（zoom=1 时）
pub const PIXELS_PER_SEC: f32 = 80.0;

/// 标尺区高度
pub const RULER_HEIGHT: f32 = 24.0;

/// 轨道条高度
pub const TRACK_HEIGHT: f32 = 48.0;

/// 轨道条间距
pub const TRACK_SPACING: f32 = 8.0;

/// 主刻度间隔（秒）
pub const MAJOR_INTERVAL: f32 = 5.0;

/// 次刻度间隔（秒）
pub const MINOR_INTERVAL: f32 = 1.0;

/// 时间轴 Canvas 数据与交互
pub struct TimelineCanvas {
    /// MIDI 实际时长（秒）
    pub duration_secs: f32,
    /// 当前水平缩放倍率
    pub zoom: f32,
    /// 当前水平滚动位置（像素）
    pub scroll_x: f32,
    /// Ctrl 键按下状态（Ctrl+滚轮 = 缩放，卷帘同款交互）
    pub ctrl_pressed: bool,
}

impl TimelineCanvas {
    /// 缩放后的内容总宽度（像素）
    pub fn content_width(&self) -> f32 {
        (self.duration_secs * PIXELS_PER_SEC * self.zoom).max(400.0)
    }

    /// 秒 → 内容坐标 x（未含滚动偏移）
    fn sec_to_content_x(&self, sec: f32) -> f32 {
        sec * PIXELS_PER_SEC * self.zoom
    }

    /// 可视窗口覆盖的秒区间 `(start_sec, end_sec)`
    fn visible_seconds(&self, viewport_w: f32) -> (f32, f32) {
        let start = self.scroll_x / (PIXELS_PER_SEC * self.zoom);
        let end = (self.scroll_x + viewport_w) / (PIXELS_PER_SEC * self.zoom);
        (start.max(0.0), end.min(self.duration_secs))
    }
}

/// 纯函数：普通滚轮增量的水平滚动位移（钳制单次最大幅度）
pub fn wheel_scroll_delta(delta_y: f32) -> f32 {
    (delta_y * SCROLL_LINES_SCALE).clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA)
}

impl Program<Message, Theme, Renderer> for TimelineCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let pos = cursor.position()?;
        if !bounds.contains(pos) {
            return None;
        }

        match event {
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let local_x = pos.x - bounds.x;
                if self.ctrl_pressed {
                    // Ctrl+滚轮：以光标为锚点水平缩放（卷帘标尺区同款平滑步进）
                    let factor = zoom_factor_from_delta(delta)?;
                    Some(canvas::Action::publish(Message::VideoClip(
                        VideoClipAction::TimelineZoom {
                            zoom: self.zoom * factor,
                            fixed_ratio: fixed_ratio_from_viewport(local_x, 0.0, bounds.width),
                            viewport_w: bounds.width,
                        },
                    )))
                } else {
                    // 普通滚轮：水平滚动（时间轴仅有横向内容）
                    let dy = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y * SCROLL_LINES_SCALE,
                        mouse::ScrollDelta::Pixels { y, .. } => *y,
                    };
                    let d = wheel_scroll_delta(dy);
                    if d.abs() < f32::EPSILON {
                        None
                    } else {
                        Some(canvas::Action::publish(Message::VideoClip(
                            VideoClipAction::TimelineScroll {
                                x: self.scroll_x + d,
                                viewport_w: bounds.width,
                            },
                        )))
                    }
                }
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
        let palette = theme.extended_palette();
        let weak_color = palette.background.weak.color;
        let strong_color = palette.background.strong.color;
        let weak_text = palette.background.weak.text;

        let mut frame = Frame::new(renderer, bounds.size());

        // ── 背景 ──
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            palette.background.weakest.color,
        );

        // ── 标尺：可视窗口内的秒刻度 ──
        let (start_sec, end_sec) = self.visible_seconds(bounds.width);
        let mut t = start_sec.floor();
        while t <= end_sec {
            let is_major = (t % MAJOR_INTERVAL).abs() < 0.01;
            let tick_h = if is_major { 12.0 } else { 6.0 };
            let x = self.sec_to_content_x(t) - self.scroll_x;
            if x >= -1.0 && x <= bounds.width + 1.0 {
                let color = if is_major { strong_color } else { weak_color };
                frame.fill_rectangle(
                    Point::new(x, RULER_HEIGHT - tick_h),
                    Size::new(1.0, tick_h),
                    color,
                );
                if is_major {
                    frame.fill_text(Text {
                        content: format!("{t:.0}s"),
                        position: Point::new(x + 3.0, 2.0),
                        size: 10.0.into(),
                        color: weak_text,
                        ..Text::default()
                    });
                }
            }
            t += MINOR_INTERVAL;
        }

        // ── 视频轨（蓝色）与音频轨（绿色），等长 = duration*pps*zoom ──
        let track_w = self.sec_to_content_x(self.duration_secs);
        let video_color = Color::from_rgb(0.2, 0.6, 0.95);
        let audio_color = Color::from_rgb(0.25, 0.75, 0.35);
        let video_y = RULER_HEIGHT + TRACK_SPACING;
        let audio_y = video_y + TRACK_HEIGHT + TRACK_SPACING;

        for (y, color, label) in [
            (video_y, video_color, "视频"),
            (audio_y, audio_color, "音频"),
        ] {
            let rect_w = (track_w - self.scroll_x).min(bounds.width).max(0.0);
            if rect_w > 0.0 {
                frame.fill_rectangle(Point::new(0.0, y), Size::new(rect_w, TRACK_HEIGHT), color);
                // 轨道标签（跟随内容滚动）
                let label_x = 8.0 - self.scroll_x;
                if label_x > -60.0 && label_x < bounds.width {
                    frame.fill_text(Text {
                        content: label.to_string(),
                        position: Point::new(label_x, y + (TRACK_HEIGHT - 14.0) / 2.0),
                        size: 11.0.into(),
                        color: Color::WHITE,
                        ..Text::default()
                    });
                }
            }
        }

        // ── 走带指示线：固定在时间轴区域前端 PLAYHEAD_X 处，纵贯全高 ──
        frame.fill_rectangle(
            Point::new(super::layout::PLAYHEAD_X, 0.0),
            Size::new(2.0, bounds.height),
            Color::from_rgb(0.95, 0.25, 0.25),
        );

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wheel_scroll_delta_clamped() {
        assert!((wheel_scroll_delta(1.0) - SCROLL_LINES_SCALE).abs() < f32::EPSILON);
        assert!((wheel_scroll_delta(100.0) - SCROLL_MAX_DELTA).abs() < f32::EPSILON);
        assert!((wheel_scroll_delta(-100.0) + SCROLL_MAX_DELTA).abs() < f32::EPSILON);
    }

    #[test]
    fn test_content_width_scales_with_zoom_and_min() {
        let c = TimelineCanvas {
            duration_secs: 10.0,
            zoom: 1.0,
            scroll_x: 0.0,
            ctrl_pressed: false,
        };
        // 10s * 80px/s = 800
        assert!((c.content_width() - 800.0).abs() < f32::EPSILON);

        let tiny = TimelineCanvas {
            duration_secs: 0.0,
            zoom: 1.0,
            scroll_x: 0.0,
            ctrl_pressed: false,
        };
        // 最小兜底 400
        assert!((tiny.content_width() - 400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_visible_seconds_window() {
        let c = TimelineCanvas {
            duration_secs: 30.0,
            zoom: 2.0,
            scroll_x: 320.0, // pps_zoom=160 → 起点 2s
            ctrl_pressed: false,
        };
        // 视口宽 800 → 终点 (320+800)/160 = 7s
        let (s, e) = c.visible_seconds(800.0);
        assert!((s - 2.0).abs() < 0.001, "start = {s}");
        assert!((e - 7.0).abs() < 0.001, "end = {e}");

        // 滚动越界被 clamp 到 0
        let scrolled_back = TimelineCanvas {
            scroll_x: -500.0,
            ..c
        };
        let (s, _) = scrolled_back.visible_seconds(800.0);
        assert!(s.abs() < f32::EPSILON);
    }
}
