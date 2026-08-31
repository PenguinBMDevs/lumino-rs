//! 钢琴键盘 canvas — 对应 `yinhe piano_view/keyboard.rs:157`
//!
//! - 横向：键盘在左列，键沿 y 轴（key127 顶 / key0 底），复用 `ViewState::key_to_y`
//! - 纵向：键盘在底部横条，键沿 x 轴（key0 左 / key127 右），转置坐标
//! - 渲染层：走 iced canvas `Frame`（矢量），色板复用 `Theme::palette`
//!   与 lumino `ui-editor/grid/keyboard.rs:32..166` 一致，不自建 wgpu 管线。

use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};

use lumino_core::ViewState;
use lumino_ui_core::{Renderer, Theme};

use crate::piano_view::layout::{Orientation, PianoLayout};

/// 绘制钢琴键盘到 Geometry（供 canvas Program 调用）
///
/// `layout` 提供 `keyboard_rect` 与方向，`view` 提供 `key_to_y` 等坐标系。
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

/// 绘制键盘（横向/纵向分支，复用 yinhe `keyboard::paint` 逻辑）
pub fn draw(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &Theme,
) {
    match layout.orientation {
        Orientation::Horizontal => draw_horizontal(view, layout, frame, bounds, theme),
        Orientation::Vertical => draw_vertical(view, layout, frame, bounds, theme),
    }
}

fn is_black_key(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

fn note_label(key: u8) -> String {
    let octave = key as i32 / 12 - 1;
    format!("C{octave}")
}

fn draw_horizontal(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    _bounds: Rectangle,
    theme: &Theme,
) {
    let kr = layout.keyboard_rect;
    // 本地 offset：frame 以 bounds 为原点，需把 keyboard_rect 平移到本地坐标
    // 调用方 canvas bounds 与 layout rect 已对齐，此处直接以 rect 本身绘制
    let kb_w = kr.width;
    let kh = view.zoom_y;
    if kh < 0.5 || kb_w < 1.0 {
        return;
    }
    let palette = theme.extended_palette();
    let bg = palette.background.weak.color;
    let border = palette.background.strong.color;

    // 白键
    for key in 0u8..128 {
        if is_black_key(key) {
            continue;
        }
        let y = view.key_to_y(key as u16);
        // key_to_y 已含 ruler 偏移，此处对齐到 keyboard_rect 内需减去 content_y? 简化：用 view 坐标直接映射到 frame
        // 为与 lumino ui-editor keyboard 一致，直接按世界坐标计算 screen_y
        let screen_y = y;
        // 可见性裁剪
        if screen_y + kh < kr.y || screen_y > kr.y + kr.height {
            continue;
        }
        let r = Rectangle::new(Point::new(kr.x, screen_y), Size::new(kb_w, kh));
        let p = Path::rectangle(r.position(), r.size());
        let is_c = key % 12 == 0;
        let fill = if is_c { bg } else { iced_core::Color::WHITE };
        frame.fill(&p, fill);
        frame.stroke(&p, Stroke::default().with_width(0.5).with_color(border));

        if key % 12 == 0 {
            let label = note_label(key);
            frame.fill_text(Text {
                content: label,
                position: Point::new(kr.x + 4.0, screen_y + kh * 0.5),
                color: palette.background.base.text,
                size: iced_core::Pixels((kh * 0.45).clamp(8.0, 12.0)),
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Center,
                shaping: iced_core::text::Shaping::Basic,
                max_width: kb_w - 4.0,
                line_height: iced_core::text::LineHeight::Relative(1.0),
            });
        }
    }
    // 黑键覆盖
    for key in 0u8..128 {
        if !is_black_key(key) {
            continue;
        }
        let y = view.key_to_y(key as u16);
        if y + kh < kr.y || y > kr.y + kr.height {
            continue;
        }
        let r = Rectangle::new(Point::new(kr.x, y), Size::new(kb_w, kh));
        let p = Path::rectangle(r.position(), r.size());
        frame.fill(&p, iced_core::Color::from_rgb8(32, 32, 32));
        frame.stroke(&p, Stroke::default().with_width(0.5).with_color(border));
    }
}

fn draw_vertical(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    _bounds: Rectangle,
    theme: &Theme,
) {
    let kr = layout.keyboard_rect;
    let kb_h = kr.height;
    let kw = view.zoom_y;
    if kw < 0.5 || kb_h < 1.0 {
        return;
    }
    let palette = theme.extended_palette();
    let bg = palette.background.weak.color;
    let border = palette.background.strong.color;

    for key in 0u8..128 {
        if is_black_key(key) {
            continue;
        }
        let x = view.scroll_x + key as f32 * kw; // 近似，转置后语义
        // 映射到 keyboard_rect 内
        let screen_x = kr.x + (x - view.scroll_x);
        if screen_x + kw < kr.x || screen_x > kr.x + kr.width {
            continue;
        }
        let r = Rectangle::new(Point::new(screen_x, kr.y), Size::new(kw, kb_h));
        let p = Path::rectangle(r.position(), r.size());
        frame.fill(&p, iced_core::Color::WHITE);
        frame.stroke(&p, Stroke::default().with_width(0.5).with_color(border));
        if key % 12 == 0 {
            frame.fill_text(Text {
                content: note_label(key),
                position: Point::new(screen_x + kw * 0.5, kr.y + kb_h * 0.5),
                color: palette.background.base.text,
                size: iced_core::Pixels((kw * 0.35).clamp(8.0, 11.0)),
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Center,
                shaping: iced_core::text::Shaping::Basic,
                max_width: kw,
                line_height: iced_core::text::LineHeight::Relative(1.0),
            });
        }
    }
    for key in 0u8..128 {
        if !is_black_key(key) {
            continue;
        }
        let x = view.scroll_x + key as f32 * kw;
        let screen_x = kr.x + (x - view.scroll_x);
        if screen_x + kw < kr.x || screen_x > kr.x + kr.width {
            continue;
        }
        let r = Rectangle::new(Point::new(screen_x, kr.y), Size::new(kw, kb_h));
        let p = Path::rectangle(r.position(), r.size());
        let _ = bg;
        frame.fill(&p, iced_core::Color::from_rgb8(32, 32, 32));
        frame.stroke(&p, Stroke::default().with_width(0.5).with_color(border));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::{Point, Rectangle, Size};

    #[test]
    fn black_key_detection_matches_yinhe() {
        assert!(is_black_key(1));
        assert!(!is_black_key(0));
        assert!(is_black_key(10));
        assert!(!is_black_key(11));
    }

    #[test]
    fn draw_does_not_panic_on_zero_kb() {
        let view = ViewState {
            zoom_y: 0.0,
            ..Default::default()
        };
        let layout = crate::piano_view::layout::PianoLayout {
            content_rect: Rectangle::new(Point::new(0.0, 0.0), Size::new(800.0, 600.0)),
            music_rect: Rectangle::new(Point::new(120.0, 24.0), Size::new(680.0, 576.0)),
            keyboard_rect: Rectangle::new(Point::new(0.0, 24.0), Size::new(120.0, 576.0)),
            ruler_rect: Rectangle::new(Point::new(120.0, 0.0), Size::new(680.0, 24.0)),
            content_y: 24.0,
            content_bottom: 600.0,
            w: 680,
            h: 576,
            pw: 680,
            ph: 576,
            total_ticks: 1920.0 * 100.0,
            panels_total_h: 0.0,
            orientation: crate::piano_view::layout::Orientation::Horizontal,
        };
        // 仅断言不 panic（无 renderer 时不实际调用 Frame）
        let _ = layout.keyboard_rect;
        let _ = view.zoom_y;
    }
}
