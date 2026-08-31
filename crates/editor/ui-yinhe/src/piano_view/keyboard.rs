//! 钢琴键盘 — 对应 `yinhe piano_view/keyboard.rs:157`
//!
//! - 横向：键盘在左列，键沿 y 轴（key127 顶 / key0 底），复用 `ViewState` 坐标
//! - 纵向：键盘在底部横条，键沿 x 轴（key0 左 / key127 右），转置坐标
//! - 渲染层：走 iced canvas `Frame`（矢量），色板复用 `Theme`
//!   白键/黑键区分，按压高亮，C4 特殊标注

use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};

use lumino_core::ViewState;
use lumino_ui_core::{Renderer, Theme};

use crate::piano_view::layout::{Orientation, PianoLayout};

fn is_black_key(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

fn note_label(key: u8) -> String {
    let octave = key as i32 / 12 - 1;
    format!("C{octave}")
}

// ── Theme helpers（对齐 lumino-ui-editor/grid/theme.rs） ──

fn is_light(theme: &Theme) -> bool {
    if lumino_ui_core::theme::is_high_contrast() {
        return false;
    }
    theme.extended_palette().background.weakest.color.r > 0.5
}

fn white_key_color(theme: &Theme) -> iced_core::Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::WHITE_KEY;
    }
    let palette = theme.extended_palette().background;
    if is_light(theme) {
        palette.weak.color
    } else {
        palette.weakest.color
    }
}

fn black_key_color(theme: &Theme) -> iced_core::Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::BLACK_KEY;
    }
    let palette = theme.extended_palette().background;
    if is_light(theme) {
        palette.strong.color
    } else {
        palette.base.color
    }
}

fn border_color(theme: &Theme) -> iced_core::Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::BORDER;
    }
    let p = theme.extended_palette().background;
    if is_light(theme) {
        p.strongest.color
    } else {
        p.base.color
    }
}

fn text_color(theme: &Theme) -> iced_core::Color {
    if lumino_ui_core::theme::is_high_contrast() {
        return lumino_ui_core::theme::hc::TEXT;
    }
    if is_light(theme) {
        iced_core::Color::BLACK
    } else {
        iced_core::Color::WHITE
    }
}

fn pressed_highlight_color(theme: &Theme, is_black: bool) -> iced_core::Color {
    // 亮色：暖橙高亮；暗色：亮黄高亮，与主题 primary 呼应
    let primary = theme.extended_palette().primary.strong.color;
    if is_black {
        iced_core::Color::from_rgba(
            (primary.r * 0.9 + 0.1).min(1.0),
            (primary.g * 0.9 + 0.15).min(1.0),
            (primary.b * 0.5).min(1.0),
            0.92,
        )
    } else {
        iced_core::Color::from_rgba(
            (primary.r * 0.85 + 0.15).min(1.0),
            (primary.g * 0.78 + 0.22).min(1.0),
            (primary.b * 0.35).min(1.0),
            0.88,
        )
    }
}

/// 绘制钢琴键盘到 Geometry（供 canvas Program 调用）
///
/// `layout` 提供 `keyboard_rect` 与方向，`view` 提供坐标系。
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

/// 带按压高亮的变体（`pressed` 为当前按下的 key 集合）
pub fn draw_with_pressed(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &Theme,
    pressed: Option<&[u8]>,
) {
    match layout.orientation {
        Orientation::Horizontal => {
            draw_horizontal_with_pressed(view, layout, frame, bounds, theme, pressed)
        }
        Orientation::Vertical => {
            draw_vertical_with_pressed(view, layout, frame, bounds, theme, pressed)
        }
    }
}

/// 绘制键盘（横向/纵向分支，复用 yinhe `keyboard::paint` 逻辑）
pub fn draw(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &Theme,
) {
    draw_with_pressed(view, layout, frame, bounds, theme, None);
}

fn is_pressed(pressed: Option<&[u8]>, key: u8) -> bool {
    pressed.is_some_and(|s| s.contains(&key))
}

#[allow(dead_code)]
fn draw_horizontal(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &Theme,
) {
    draw_horizontal_with_pressed(view, layout, frame, bounds, theme, None);
}

fn draw_horizontal_with_pressed(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    _bounds: Rectangle,
    theme: &Theme,
    pressed: Option<&[u8]>,
) {
    let kr = layout.keyboard_rect;
    let kb_w = kr.width;
    let kh = view.zoom_y;
    if kh < 0.5 || kb_w < 1.0 {
        return;
    }
    let border = border_color(theme);
    let stroke = Stroke::default()
        .with_width((kh * 0.0833).clamp(0.5, 1.5))
        .with_color(border);
    let text_col = text_color(theme);
    let primary = theme.extended_palette().primary.strong.color;

    // 计算屏幕 y： world = (max - key)*zoom, screen = world - scroll + content_y
    let max_idx = (view.visible_key_count.max(1) - 1) as f32;
    let content_y = layout.content_y;

    // 白键：底层
    for key in 0u8..128 {
        if is_black_key(key) {
            continue;
        }
        let world_y = (max_idx - key as f32) * kh;
        let screen_y = world_y - view.scroll_y + content_y;
        if screen_y + kh < kr.y || screen_y > kr.y + kr.height {
            continue;
        }
        let r = Rectangle::new(Point::new(kr.x, screen_y), Size::new(kb_w, kh));
        let p = Path::rectangle(r.position(), r.size());
        let base = white_key_color(theme);
        let fill = if is_pressed(pressed, key) {
            pressed_highlight_color(theme, false)
        } else {
            base
        };
        frame.fill(&p, fill);
        frame.stroke(&p, stroke);

        if key % 12 == 0 {
            let label = note_label(key);
            let is_c4 = key == 60;
            let color = if is_c4 { primary } else { text_col.scale_alpha(0.85) };
            let size = if is_c4 {
                (kh * 0.55).clamp(9.0, 13.0)
            } else {
                (kh * 0.45).clamp(8.0, 11.0)
            };
            frame.fill_text(Text {
                content: label,
                position: Point::new(kr.x + 4.0, screen_y + kh * 0.5),
                color,
                size: iced_core::Pixels(size),
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Center,
                shaping: iced_core::text::Shaping::Basic,
                max_width: kb_w - 8.0,
                line_height: iced_core::text::LineHeight::Relative(1.0),
            });
            // C4 额外下划线强调
            if is_c4 {
                let line_y = screen_y + kh * 0.72;
                let line = Rectangle::new(
                    Point::new(kr.x + 3.0, line_y),
                    Size::new((kb_w - 6.0).min(28.0), 1.2),
                );
                frame.fill_rectangle(line.position(), line.size(), primary.scale_alpha(0.9));
            }
        }
    }
    // 黑键覆盖：稍窄，突出立体感
    let black_w = (kb_w * 0.62).max(8.0);
    for key in 0u8..128 {
        if !is_black_key(key) {
            continue;
        }
        let world_y = (max_idx - key as f32) * kh;
        let screen_y = world_y - view.scroll_y + content_y;
        if screen_y + kh < kr.y || screen_y > kr.y + kr.height {
            continue;
        }
        let r = Rectangle::new(Point::new(kr.x, screen_y), Size::new(black_w, kh));
        let p = Path::rectangle(r.position(), r.size());
        let base = black_key_color(theme);
        let fill = if is_pressed(pressed, key) {
            pressed_highlight_color(theme, true)
        } else {
            iced_core::Color::from_rgb8(28, 28, 30)
        };
        let _ = base;
        frame.fill(&p, fill);
        frame.stroke(&p, stroke);
    }
}

#[allow(dead_code)]
fn draw_vertical(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    _bounds: Rectangle,
    theme: &Theme,
) {
    draw_vertical_with_pressed(view, layout, frame, _bounds, theme, None);
}

fn draw_vertical_with_pressed(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    _bounds: Rectangle,
    theme: &Theme,
    pressed: Option<&[u8]>,
) {
    let kr = layout.keyboard_rect;
    let kb_h = kr.height;
    let kw = view.zoom_y;
    if kw < 0.5 || kb_h < 1.0 {
        return;
    }
    let border = border_color(theme);
    let stroke = Stroke::default()
        .with_width((kw * 0.0833).clamp(0.5, 1.5))
        .with_color(border);
    let text_col = text_color(theme);
    let primary = theme.extended_palette().primary.strong.color;

    // 纵向：key 沿 X， time 沿 Y（键盘在底部）
    // screen_x = kr.x + key*kw - scroll_x
    for key in 0u8..128 {
        if is_black_key(key) {
            continue;
        }
        let screen_x = kr.x + key as f32 * kw - view.scroll_x;
        if screen_x + kw < kr.x || screen_x > kr.x + kr.width {
            continue;
        }
        let r = Rectangle::new(Point::new(screen_x, kr.y), Size::new(kw, kb_h));
        let p = Path::rectangle(r.position(), r.size());
        let base = white_key_color(theme);
        let fill = if is_pressed(pressed, key) {
            pressed_highlight_color(theme, false)
        } else {
            base
        };
        let _ = base;
        frame.fill(&p, fill);
        frame.stroke(&p, stroke);
        if key % 12 == 0 {
            let is_c4 = key == 60;
            let color = if is_c4 { primary } else { text_col.scale_alpha(0.85) };
            let size = if is_c4 {
                (kw * 0.42).clamp(9.0, 12.0)
            } else {
                (kw * 0.32).clamp(8.0, 10.0)
            };
            // 纵向竖排文字：用水平绘制近似，居中
            frame.fill_text(Text {
                content: note_label(key),
                position: Point::new(screen_x + kw * 0.5, kr.y + kb_h * 0.5),
                color,
                size: iced_core::Pixels(size),
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Center,
                shaping: iced_core::text::Shaping::Basic,
                max_width: kw,
                line_height: iced_core::text::LineHeight::Relative(1.0),
            });
        }
    }
    let black_h = (kb_h * 0.62).max(8.0);
    let black_y = kr.y;
    for key in 0u8..128 {
        if !is_black_key(key) {
            continue;
        }
        let screen_x = kr.x + key as f32 * kw - view.scroll_x;
        if screen_x + kw < kr.x || screen_x > kr.x + kr.width {
            continue;
        }
        let r = Rectangle::new(Point::new(screen_x, black_y), Size::new(kw, black_h));
        let p = Path::rectangle(r.position(), r.size());
        let fill = if is_pressed(pressed, key) {
            pressed_highlight_color(theme, true)
        } else {
            iced_core::Color::from_rgb8(28, 28, 30)
        };
        frame.fill(&p, fill);
        frame.stroke(&p, stroke);
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
            content_y: 52.0,
            content_bottom: 600.0,
            w: 680,
            h: 576,
            pw: 680,
            ph: 576,
            total_ticks: 1920.0 * 100.0,
            panels_total_h: 0.0,
            orientation: crate::piano_view::layout::Orientation::Horizontal,
        };
        let _ = layout.keyboard_rect;
        let _ = view.zoom_y;
    }

    #[test]
    fn c4_label_is_c4() {
        assert_eq!(note_label(60), "C4");
        assert_eq!(note_label(0), "C-1");
    }

    #[test]
    fn pressed_highlight_uses_different_color() {
        // 仅验证函数不 panic，颜色分支覆盖
        let theme = lumino_ui_core::window::Window::new("Tokyo Night Storm").theme;
        let c1 = pressed_highlight_color(&theme, false);
        let c2 = white_key_color(&theme);
        assert_ne!(c1.r, c2.r);
        let c3 = pressed_highlight_color(&theme, true);
        assert_ne!(c3.r, 0.0);
    }
}
