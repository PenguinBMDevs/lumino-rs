//! 速度包络 (Tempo Envelope) 模块
//!
//! 管理工程全局的速度变化曲线，以控制点列表形式存储。
//! 默认在 tick=0 处有一个 120 BPM 控制点。

use crate::{Message, Renderer, Theme};
use iced_core::{Color, Point, Rectangle, alignment, mouse};
use iced_wgpu::Geometry as Geom;
use iced_widget::canvas::{self, Frame, Program, path};

/// 速度控制点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPoint {
    /// tick 位置
    pub tick: f32,
    /// BPM 值 (20-10000)
    pub bpm: f64,
}

/// 速度包络
#[derive(Debug, Clone)]
pub struct TempoEnvelope {
    /// 控制点列表，按 tick 升序
    pub points: Vec<TempoPoint>,
}

impl Default for TempoEnvelope {
    fn default() -> Self {
        Self {
            points: vec![TempoPoint {
                tick: 0.0,
                bpm: 120.0,
            }],
        }
    }
}

impl TempoEnvelope {
    /// 获取指定 tick 位置的 BPM 值（线性插值）
    pub fn get_bpm_at(&self, tick: f32) -> f64 {
        if self.points.is_empty() {
            return 120.0;
        }

        // 如果 tick 在第一个点之前
        if tick <= self.points[0].tick {
            return self.points[0].bpm;
        }

        // 如果 tick 在最后一个点之后
        if tick >= self.points.last().unwrap().tick {
            return self.points.last().unwrap().bpm;
        }

        // 线性插值
        for i in 0..self.points.len() - 1 {
            let a = &self.points[i];
            let b = &self.points[i + 1];
            if tick >= a.tick && tick <= b.tick {
                let t = (tick - a.tick) / (b.tick - a.tick);
                return a.bpm + t as f64 * (b.bpm - a.bpm);
            }
        }

        self.points.last().unwrap().bpm
    }
}

/// 速度包络渲染器
pub struct TempoCanvas<'a> {
    pub envelope: &'a TempoEnvelope,
    pub zoom_x: f32,
    pub scroll_x: f32,
    pub keyboard_width: f32,
    pub panel_height: f32,
}

impl<'a> Program<Message, Theme, Renderer> for TempoCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geom> {
        let mut frame = Frame::new(renderer, bounds.size());

        if self.envelope.points.is_empty() {
            return vec![frame.into_geometry()];
        }

        let width = bounds.width;
        let height = bounds.height;

        // 绘制速度折线
        let line_color = theme.extended_palette().secondary.strong.color;
        let point_color = theme.extended_palette().secondary.base.color;

        // 获取可见范围的点
        let visible_points: Vec<&TempoPoint> = self
            .envelope
            .points
            .iter()
            .filter(|p| {
                let x = p.tick * self.zoom_x - self.scroll_x + self.keyboard_width;
                x >= -50.0 && x <= width + 50.0
            })
            .collect();

        if visible_points.is_empty() {
            return vec![frame.into_geometry()];
        }

        // 计算 BPM 范围
        let min_bpm = visible_points
            .iter()
            .map(|p| p.bpm)
            .fold(f64::INFINITY, |a, b| a.min(b))
            .max(20.0);
        let max_bpm = visible_points
            .iter()
            .map(|p| p.bpm)
            .fold(f64::NEG_INFINITY, |a, b| a.max(b))
            .min(10000.0);
        let bpm_range = (max_bpm - min_bpm).max(1.0);

        // 绘制折线
        let mut line_builder = path::Builder::new();
        let first_pos = self.point_screen_pos(visible_points[0], height, min_bpm, bpm_range);
        line_builder.move_to(first_pos);

        for point in visible_points.iter().skip(1) {
            let pos = self.point_screen_pos(point, height, min_bpm, bpm_range);
            line_builder.line_to(pos);
        }

        frame.stroke(
            &line_builder.build(),
            canvas::Stroke::default()
                .with_color(line_color)
                .with_width(2.0),
        );

        // 绘制控制点
        for point in &visible_points {
            let pos = self.point_screen_pos(point, height, min_bpm, bpm_range);
            frame.fill(&canvas::Path::circle(pos, 3.0), point_color);

            // 显示 BPM 值
            let text = canvas::Text {
                content: format!("{:.0}", point.bpm),
                position: Point::new(pos.x - 10.0, pos.y - 14.0),
                max_width: width,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(9.0),
                color: Color::from_rgba(0.6, 0.6, 0.6, 0.7),
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(text);
        }

        vec![frame.into_geometry()]
    }
}

impl<'a> TempoCanvas<'a> {
    fn point_screen_pos(
        &self,
        point: &TempoPoint,
        height: f32,
        min_bpm: f64,
        bpm_range: f64,
    ) -> Point {
        let x = point.tick * self.zoom_x - self.scroll_x + self.keyboard_width;
        let normalized = ((point.bpm - min_bpm) / bpm_range) as f32;
        let y = height - 10.0 - normalized * (height - 20.0).max(0.0);
        Point::new(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tempo_envelope_default() {
        let env = TempoEnvelope::default();
        assert_eq!(env.points.len(), 1);
        assert_eq!(env.points[0].tick, 0.0);
        assert_eq!(env.points[0].bpm, 120.0);
    }

    #[test]
    fn test_get_bpm_at_default() {
        let env = TempoEnvelope::default();
        assert_eq!(env.get_bpm_at(0.0), 120.0);
        assert_eq!(env.get_bpm_at(100.0), 120.0);
    }

    #[test]
    fn test_get_bpm_interpolation() {
        let env = TempoEnvelope {
            points: vec![
                TempoPoint {
                    tick: 0.0,
                    bpm: 120.0,
                },
                TempoPoint {
                    tick: 480.0,
                    bpm: 140.0,
                },
            ],
        };

        assert_eq!(env.get_bpm_at(0.0), 120.0);
        assert_eq!(env.get_bpm_at(480.0), 140.0);
        assert!((env.get_bpm_at(240.0) - 130.0).abs() < 0.01);
    }

    #[test]
    fn test_get_bpm_before_first() {
        let env = TempoEnvelope {
            points: vec![TempoPoint {
                tick: 100.0,
                bpm: 120.0,
            }],
        };
        assert_eq!(env.get_bpm_at(50.0), 120.0);
    }

    #[test]
    fn test_get_bpm_after_last() {
        let env = TempoEnvelope {
            points: vec![
                TempoPoint {
                    tick: 0.0,
                    bpm: 120.0,
                },
                TempoPoint {
                    tick: 480.0,
                    bpm: 140.0,
                },
            ],
        };
        assert_eq!(env.get_bpm_at(960.0), 140.0);
    }

    #[test]
    fn test_empty_envelope() {
        let env = TempoEnvelope { points: vec![] };
        assert_eq!(env.get_bpm_at(0.0), 120.0);
    }

    #[test]
    fn test_multiple_points() {
        let env = TempoEnvelope {
            points: vec![
                TempoPoint {
                    tick: 0.0,
                    bpm: 120.0,
                },
                TempoPoint {
                    tick: 240.0,
                    bpm: 100.0,
                },
                TempoPoint {
                    tick: 480.0,
                    bpm: 140.0,
                },
            ],
        };

        assert!((env.get_bpm_at(120.0) - 110.0).abs() < 0.01);
        assert!((env.get_bpm_at(360.0) - 120.0).abs() < 0.01);
    }
}
