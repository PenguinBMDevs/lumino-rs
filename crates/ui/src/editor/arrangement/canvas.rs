//! 工程走带 Canvas — 按 yinhe 风格绘制所有内容
//!
//! 一次性绘制：走带背景、音轨 lane、网格线、音符矩形。
//! 坐标系统为屏幕像素空间，直接用 iced canvas 渲染，不经过 WGPU NoteRenderer。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Geometry, Program, path};

use crate::editor::grid::theme::ThemeExt;
use crate::{Message, Renderer, Theme};

/// 单个音符的屏幕空间矩形数据
#[derive(Debug, Clone)]
pub struct NoteRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: Color,
}

/// 工程走带 Canvas —— 绘制音轨 lane + 网格 + 音符矩形 + 演奏指示线
pub struct ArrangementCanvas {
    /// 音轨数量
    track_count: usize,
    /// 每轨高度（像素）
    track_height: f32,
    /// 水平滚动偏移（像素）
    pub scroll_x: f32,
    /// 垂直滚动偏移（像素）
    pub scroll_y: f32,
    /// 每 tick 的像素数（水平缩放）
    pub pixels_per_tick: f32,
    /// 可见时间范围内预生成的音符矩形
    pub notes: Vec<NoteRect>,
    /// 演奏指示线 X 坐标（None = 不绘制）
    pub playhead_x: Option<f32>,
}

impl ArrangementCanvas {
    /// 创建工程走带 Canvas
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        track_count: usize,
        track_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        pixels_per_tick: f32,
        notes: Vec<NoteRect>,
        playhead_x: Option<f32>,
    ) -> Self {
        Self {
            track_count,
            track_height,
            scroll_x,
            scroll_y,
            pixels_per_tick,
            notes,
            playhead_x,
        }
    }
}

impl Program<Message, Theme, Renderer> for ArrangementCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        _event: &iced_widget::canvas::Event,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Option<iced_widget::canvas::Action<Message>> {
        let offset = Point::new(bounds.x, bounds.y);
        let size = Size::new(bounds.width, bounds.height);

        Some(iced_widget::canvas::Action::publish(
            Message::ArrangementCanvasBoundsChanged { offset, size },
        ))
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
        let is_light = theme.is_light();

        // ── 1. 绘制音轨 lane 背景和分隔线 ──
        for track_idx in 0..self.track_count {
            let y = track_idx as f32 * self.track_height - self.scroll_y;

            // 视锥裁剪
            if y + self.track_height < 0.0 || y > canvas_h {
                continue;
            }

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

            frame.fill_rectangle(
                Point::new(0.0, y),
                Size::new(canvas_w, self.track_height),
                bg_color,
            );

            // 底部分隔线
            let line_color = if is_light {
                Color::from_rgba(0.0, 0.0, 0.0, 0.08)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.06)
            };
            let mut lb = path::Builder::new();
            lb.move_to(Point::new(0.0, y + self.track_height));
            lb.line_to(Point::new(canvas_w, y + self.track_height));
            frame.stroke(
                &lb.build(),
                iced_widget::canvas::Stroke::default()
                    .with_color(line_color)
                    .with_width(1.0),
            );
        }

        // ── 2. 网格线（以小节为单位） ──
        // 工程走带模式下不显示精细网格，只显示小节线
        if self.pixels_per_tick > 0.0 {
            let ppq = 480.0; // 通用 PPQ
            let ticks_per_bar = ppq * 4.0; // 4/4 拍
            let bar_width = ticks_per_bar * self.pixels_per_tick;

            if bar_width > 4.0 {
                // 可见时才画
                let first_bar_x = ((self.scroll_x / bar_width).floor() * bar_width) - self.scroll_x;
                let bar_color = if is_light {
                    Color::from_rgba(0.0, 0.0, 0.0, 0.06)
                } else {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.04)
                };

                let mut bx = first_bar_x;
                while bx < canvas_w {
                    if bx >= 0.0 {
                        let mut bl = path::Builder::new();
                        bl.move_to(Point::new(bx, 0.0));
                        bl.line_to(Point::new(bx, canvas_h));
                        frame.stroke(
                            &bl.build(),
                            iced_widget::canvas::Stroke::default()
                                .with_color(bar_color)
                                .with_width(1.0),
                        );
                    }
                    bx += bar_width;
                }
            }
        }

        // ── 3. 绘制音符矩形 ──
        // 音符坐标已经是屏幕像素空间（在 view_arrangement 中预计算）
        for note in &self.notes {
            frame.fill_rectangle(
                Point::new(note.x, note.y),
                Size::new(note.w, note.h),
                note.color,
            );
        }

        // ── 4. 绘制演奏指示线（复用钢琴卷帘样式） ──
        if let Some(px) = self.playhead_x {
            let indicator_color = iced_core::Color::from_rgb(1.0, 0.2, 0.2);
            // 垂直线
            let line_path = path::Path::line(
                Point::new(px, 0.0),
                Point::new(px, canvas_h),
            );
            frame.stroke(
                &line_path,
                iced_widget::canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(indicator_color),
            );
            // 顶部倒三角形
            let tri_size = 8.0;
            let tri_path = path::Path::new(|builder| {
                builder.move_to(Point::new(px - tri_size / 2.0, 0.0));
                builder.line_to(Point::new(px + tri_size / 2.0, 0.0));
                builder.line_to(Point::new(px, tri_size));
                builder.close();
            });
            frame.fill(&tri_path, indicator_color);
        }

        vec![frame.into_geometry()]
    }
}
