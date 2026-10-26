//! 吸附规格的 16×16 矢量小图（DRAW-037）
//!
//! 不引入图片资源：全音符/二分/四分/八分/十六分等用符头、符干、符尾组合，
//! 附点画圆点，连音画三根短竖线，Bar 画小节边界。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};

use crate::{Element, Message, Renderer, Theme};

use lumino_core::SnapSpec;

/// 小图尺寸（逻辑像素）
pub const SNAP_ICON_SIZE: f32 = 16.0;

/// 构造一个 16×16 吸附规格小图
pub fn snap_icon(spec: SnapSpec, color: Color) -> Element<'static> {
    Canvas::new(SnapIcon { spec, color })
        .width(iced_core::Length::Fixed(SNAP_ICON_SIZE))
        .height(iced_core::Length::Fixed(SNAP_ICON_SIZE))
        .into()
}

struct SnapIcon {
    spec: SnapSpec,
    color: Color,
}

impl canvas::Program<Message, Theme, Renderer> for SnapIcon {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let color = self.color;
        match self.spec {
            SnapSpec::Bar => {
                frame.fill_rectangle(Point::new(3.0, 3.0), Size::new(1.5, 10.0), color);
                frame.fill_rectangle(Point::new(11.5, 3.0), Size::new(1.5, 10.0), color);
                frame.fill_rectangle(Point::new(3.0, 3.0), Size::new(10.0, 1.2), color);
            }
            SnapSpec::Custom { .. } => {
                // 自定义：虚线方框 + 中心点
                for x in [3.0, 7.0, 11.0] {
                    frame.fill_rectangle(Point::new(x, 3.0), Size::new(2.0, 1.2), color);
                    frame.fill_rectangle(Point::new(x, 11.8), Size::new(2.0, 1.2), color);
                }
                frame.fill_rectangle(Point::new(3.0, 3.0), Size::new(1.2, 10.0), color);
                frame.fill_rectangle(Point::new(11.8, 3.0), Size::new(1.2, 10.0), color);
            }
            SnapSpec::Note { notes, dots, div } => {
                let head = Point::new(6.0, 11.0);
                let head_radius = 2.3;
                let head_path = Path::circle(head, head_radius);
                if notes <= 2 {
                    frame.stroke(
                        &head_path,
                        Stroke::default().with_color(color).with_width(1.2),
                    );
                } else {
                    frame.fill(&head_path, color);
                }
                if notes != 1 {
                    frame.fill_rectangle(Point::new(8.1, 2.5), Size::new(1.2, 8.5), color);
                }
                if notes >= 8 {
                    let flags = match notes {
                        8 => 1,
                        16 => 2,
                        _ => 3,
                    };
                    for index in 0..flags {
                        frame.fill_rectangle(
                            Point::new(9.0, 3.0 + index as f32 * 2.0),
                            Size::new(4.0, 1.1),
                            color,
                        );
                    }
                }
                for index in 0..dots.min(2) {
                    frame.fill_rectangle(
                        Point::new(12.0 + f32::from(index) * 2.0, 10.0),
                        Size::new(1.5, 1.5),
                        color,
                    );
                }
                if div > 1 {
                    // 连音标记：三根短竖线（避免字体依赖）
                    for index in 0..3 {
                        frame.fill_rectangle(
                            Point::new(9.5 + index as f32 * 2.0, 1.5),
                            Size::new(1.0, 4.0),
                            color,
                        );
                    }
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_preset_builds_an_icon() {
        for spec in SnapSpec::presets() {
            let _ = snap_icon(spec, Color::BLACK);
        }
        let _ = snap_icon(SnapSpec::Custom { ticks: 777 }, Color::BLACK);
    }
}
