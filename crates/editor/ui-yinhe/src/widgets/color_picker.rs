//! 颜色选择器 — yinhe `widgets/color_picker.rs:200` 的 iced 迁移桩
//!
//! 原 `egui` 实现为按钮弹出 HSV 面板（SV 平面 + 色相条 + RGBA/HSV 数值行）；
//! iced 桩以 `container + column + button + canvas` 重建，SV/色相渐变以 canvas
//! 矢量层绘制，状态由 Host 持有，图标/字体走 `Theme`。

use iced_core::mouse::{self, Cursor};
use iced_core::{Color, Length, Point, Rectangle, Size};
use iced_widget::canvas::{Cache, Frame, Geometry, Path, Program};
use iced_widget::{button, column, container, row, text, text_input};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// HSV 颜色（`egui::ecolor::Hsva` 的 iced 等价）
#[derive(Debug, Clone, Copy)]
pub struct Hsva {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub a: f32,
}

impl Hsva {
    pub fn to_color(self) -> Color {
        // 简化 HSV→RGB（h in 0..1）
        let h = self.h.fract() * 6.0;
        let i = h.floor() as i32;
        let f = h - i as f32;
        let p = self.v * (1.0 - self.s);
        let q = self.v * (1.0 - f * self.s);
        let t = self.v * (1.0 - (1.0 - f) * self.s);
        let (r, g, b) = match i % 6 {
            0 => (self.v, t, p),
            1 => (q, self.v, p),
            2 => (p, self.v, t),
            3 => (p, q, self.v),
            4 => (t, p, self.v),
            _ => (self.v, p, q),
        };
        Color::from_rgba(r, g, b, self.a)
    }

    pub fn from_color(c: Color) -> Self {
        let r = c.r;
        let g = c.g;
        let b = c.b;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / d).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };
        let s = if max == 0.0 { 0.0 } else { d / max };
        Self {
            h,
            s,
            v: max,
            a: c.a,
        }
    }
}

/// SV 平面 Canvas Program
struct SvPanel {
    h: f32,
}

impl Program<lumino_ui_core::Message, Theme, iced_wgpu::Renderer> for SvPanel {
    type State = ();
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced_wgpu::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<iced_wgpu::Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        // 简化：单色填充（实际应为 SV 渐变网格，此处以纯色占位，后续可接入 Mesh 渐变）
        let col = Hsva {
            h: self.h,
            s: 1.0,
            v: 1.0,
            a: 1.0,
        }
        .to_color();
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), col);
        // 对角线提示渐变方向
        frame.stroke(
            &Path::line(Point::ORIGIN, Point::new(bounds.width, bounds.height)),
            iced_widget::canvas::Stroke::default()
                .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.2))
                .with_width(1.0),
        );
        vec![frame.into_geometry()]
    }
}

/// 色相条 Canvas Program
struct HueBar;

impl Program<lumino_ui_core::Message, Theme, iced_wgpu::Renderer> for HueBar {
    type State = ();
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced_wgpu::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<iced_wgpu::Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        // 简化：七段彩虹占位
        let segs = 7;
        for i in 0..segs {
            let h = i as f32 / segs as f32;
            let c = Hsva {
                h,
                s: 1.0,
                v: 1.0,
                a: 1.0,
            }
            .to_color();
            let x = i as f32 / segs as f32 * bounds.width;
            let w = bounds.width / segs as f32;
            frame.fill_rectangle(Point::new(x, 0.0), Size::new(w, bounds.height), c);
        }
        vec![frame.into_geometry()]
    }
}

/// 渲染颜色编辑按钮（点击弹出面板的触发器）
pub fn color_edit_button<'a>(window: &'a Window, color: Color) -> Element<'a> {
    let preview =
        container(iced_widget::Space::new().width(28).height(20)).style(move |_t: &Theme| {
            container::Style {
                background: Some(iced_core::Background::Color(color)),
                border: iced_core::Border {
                    radius: 3.0.into(),
                    width: 1.0,
                    color: iced_core::Color::from_rgb(0.4, 0.4, 0.4),
                },
                ..Default::default()
            }
        });

    button(preview)
        .on_press(lumino_ui_core::message::null())
        .padding(2)
        .style(|_t: &Theme, _| button::Style::default())
        .into()
}

/// 渲染完整调色板面板（供对话框/侧栏嵌入）
pub fn view<'a>(window: &'a Window, hsva: Hsva) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let sv = iced_widget::canvas::Canvas::new(SvPanel { h: hsva.h })
        .width(Length::Fixed(240.0))
        .height(Length::Fixed(150.0));

    let hue = iced_widget::canvas::Canvas::new(HueBar)
        .width(Length::Fixed(240.0))
        .height(Length::Fixed(16.0));

    let rgba_row = row![
        text("R").size(11),
        text_input("0", &format!("{}", (hsva.to_color().r * 255.0) as u8))
            .on_input(|_| lumino_ui_core::message::null())
            .padding(4)
            .width(Length::Fixed(50.0)),
        text("G").size(11),
        text_input("0", &format!("{}", (hsva.to_color().g * 255.0) as u8))
            .on_input(|_| lumino_ui_core::message::null())
            .padding(4)
            .width(Length::Fixed(50.0)),
        text("B").size(11),
        text_input("0", &format!("{}", (hsva.to_color().b * 255.0) as u8))
            .on_input(|_| lumino_ui_core::message::null())
            .padding(4)
            .width(Length::Fixed(50.0)),
    ]
    .spacing(6);

    let hsv_row = row![
        text("H").size(11),
        text(format!("{:.0}°", hsva.h * 360.0)).size(11),
        text("S").size(11),
        text(format!("{:.0}%", hsva.s * 100.0)).size(11),
        text("V").size(11),
        text(format!("{:.0}%", hsva.v * 100.0)).size(11),
    ]
    .spacing(6);

    let content = column![rgba_row, hsv_row, sv, hue].spacing(8).padding(12);

    container(content)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
