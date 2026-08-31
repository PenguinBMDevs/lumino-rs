//! 网格背景 — 对应 `yinhe piano_view/bg.rs:488`（横向/纵向转置）
//!
//! 职责（与 `yinhe-bg` 一致区分）：
//! - `crate::piano_view::bg` 画 **横向 key 轴背景**（黑键条纹 + 八度线 / 纵向转置）
//! - 小节/拍竖向网格由 `lumino-gfx::GridRenderer` 的 `infinite_grid.wgsl` 常驻 GPU 绘制，
//!   桩层仅在 iced canvas 矢量层画 key 轴条纹与八度线，保证不自建 wgpu 管线。

use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Geometry, Path};

use lumino_core::ViewState;
use lumino_ui_core::{Renderer, Theme};

use crate::piano_view::layout::{Orientation, PianoLayout};

/// 绘制调式/八度背景到 Geometry
pub fn draw_to_geometry(
    view: &ViewState,
    layout: &PianoLayout,
    renderer: &Renderer,
    bounds: Rectangle,
    theme: &Theme,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(view, layout, &mut frame, bounds, theme);
    frame.into_geometry()
}

/// 绘制背景（分支到横/纵向）
pub fn draw(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    _bounds: Rectangle,
    theme: &Theme,
) {
    match layout.orientation {
        Orientation::Horizontal => draw_horizontal(view, layout, frame, theme),
        Orientation::Vertical => draw_vertical(view, layout, frame, theme),
    }
}

fn stripe_color(theme: &Theme) -> iced_core::Color {
    theme
        .extended_palette()
        .background
        .weak
        .color
        .scale_alpha(0.5)
}

fn octave_color(theme: &Theme) -> iced_core::Color {
    theme
        .extended_palette()
        .background
        .strong
        .color
        .scale_alpha(0.35)
}

fn is_black_key(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

fn draw_horizontal(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    theme: &Theme,
) {
    let mr = layout.music_rect;
    let kh = view.zoom_y;
    if kh < 0.5 || mr.width < 1.0 || mr.height < 1.0 {
        return;
    }
    let stripe = stripe_color(theme);
    let octave = octave_color(theme);

    // 黑键行条纹（按 ViewState 坐标，仅画可见 key 段）
    let key_lo = view.y_to_key(mr.y + mr.height) as u8;
    let key_hi = view.y_to_key(mr.y).min(127) as u8;
    for key in key_lo..=key_hi {
        if !is_black_key(key) {
            continue;
        }
        let y = view.key_to_y(key as u16);
        if y + kh < mr.y || y > mr.y + mr.height {
            continue;
        }
        let rect = Rectangle::new(Point::new(mr.x, y), Size::new(mr.width, kh));
        let clipped = rect.intersection(&mr);
        if let Some(r) = clipped {
            let p = Path::rectangle(r.position(), r.size());
            frame.fill(&p, stripe);
        }
    }

    // 八度线（每个 C 顶部横线，与 yinhe bg::paint_octave_lines 对齐）
    for key in (0u8..128).step_by(12) {
        let y = view.key_to_y(key as u16) + kh; // C 顶部（与 yinhe bottom - key*kh 对齐）
        // ViewState 的 key_to_y 已是屏幕坐标，直接判断可见性
        if y < mr.y || y > mr.y + mr.height {
            continue;
        }
        let line = Rectangle::new(Point::new(mr.x, y), Size::new(mr.width, 1.0));
        let p = Path::rectangle(line.position(), line.size());
        frame.fill(&p, octave);
    }
}

fn draw_vertical(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    theme: &Theme,
) {
    let mr = layout.music_rect;
    let kw = view.zoom_y;
    if kw < 0.5 || mr.width < 1.0 || mr.height < 1.0 {
        return;
    }
    let stripe = stripe_color(theme);
    let octave = octave_color(theme);

    // 纵向：key 沿 X 排布，tick 沿 Y；黑键列条纹
    for key in 0u8..128 {
        if !is_black_key(key) {
            continue;
        }
        let x = mr.x + key as f32 * kw - view.scroll_x;
        if x + kw < mr.x || x > mr.x + mr.width {
            continue;
        }
        let rect = Rectangle::new(Point::new(x, mr.y), Size::new(kw, mr.height));
        if let Some(r) = rect.intersection(&mr) {
            let p = Path::rectangle(r.position(), r.size());
            frame.fill(&p, stripe);
        }
    }

    for key in (0u8..128).step_by(12) {
        let x = mr.x + key as f32 * kw - view.scroll_x;
        if x < mr.x || x > mr.x + mr.width {
            continue;
        }
        let line = Rectangle::new(Point::new(x, mr.y), Size::new(1.0, mr.height));
        let p = Path::rectangle(line.position(), line.size());
        frame.fill(&p, octave);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_black_key_matches_yinhe() {
        assert!(is_black_key(1));
        assert!(!is_black_key(0));
        assert!(is_black_key(6));
        assert!(!is_black_key(7));
    }
}
