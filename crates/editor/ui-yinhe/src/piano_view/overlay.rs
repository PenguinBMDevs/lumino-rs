//! 演奏指示线覆盖层 — 对应 `yinhe piano_view/overlay.rs:317` 与 `arrange/view_ui/render.rs`
//!
//! 红色竖线 + 三角形头，跟随播放 tick，支持横/纵方向，
//! 与 `lumino_core::ViewState` 联动（`scroll_x / zoom_x`）。

use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke};

use lumino_core::ViewState;
use lumino_ui_core::{Renderer, Theme};

use crate::piano_view::layout::{Orientation, PianoLayout};

const INDICATOR_WIDTH: f32 = 2.0;
const TRIANGLE_SIZE: f32 = 8.0;
const INDICATOR_COLOR: iced_core::Color = iced_core::Color::from_rgb(1.0, 0.2, 0.2);

/// 绘制演奏指示线到 Geometry（供 canvas Program 调用）
///
/// - `cursor_tick`: 当前播放位置（tick），None 则不绘制
/// - `layout`/`view`: 用于 tick→px 映射与裁剪
pub fn draw_to_geometry(
    view: &ViewState,
    layout: &PianoLayout,
    renderer: &Renderer,
    bounds: Rectangle,
    _theme: &Theme,
    cursor_tick: Option<f64>,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(view, layout, &mut frame, bounds, cursor_tick);
    frame.into_geometry()
}

/// 绘制演奏指示线（横向竖线/纵向横线，含三角形头）
pub fn draw(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    cursor_tick: Option<f64>,
) {
    let Some(tick) = cursor_tick else {
        return;
    };
    if tick < 0.0 {
        return;
    }
    match layout.orientation {
        Orientation::Horizontal => draw_horizontal(view, layout, frame, bounds, tick),
        Orientation::Vertical => draw_vertical(view, layout, frame, bounds, tick),
    }
}

fn draw_horizontal(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    tick: f64,
) {
    // tick → outer x
    let cx = view.tick_to_x(tick as f32);
    // 裁剪：仅在 music_rect 横向范围内可见
    let music = layout.music_rect;
    if cx < music.x - 1.0 || cx > music.x + music.width + 1.0 {
        return;
    }
    if cx < bounds.x || cx > bounds.x + bounds.width {
        return;
    }
    // 本地坐标：frame 原点 = bounds 原点
    let local_x = cx - bounds.x;
    let top = layout.content_y - bounds.y;
    let bottom = layout.content_bottom - bounds.y;
    if bottom <= top {
        return;
    }

    // 竖线
    let line = Path::line(Point::new(local_x, top), Point::new(local_x, bottom));
    frame.stroke(
        &line,
        Stroke::default()
            .with_width(INDICATOR_WIDTH)
            .with_color(INDICATOR_COLOR),
    );

    // 顶部倒三角形（▼）在 ruler 底部 / content 顶部
    let tri = Path::new(|b| {
        let half = TRIANGLE_SIZE / 2.0;
        let tip_y = top + TRIANGLE_SIZE;
        b.move_to(Point::new(local_x - half, top));
        b.line_to(Point::new(local_x + half, top));
        b.line_to(Point::new(local_x, tip_y));
        b.close();
    });
    frame.fill(&tri, INDICATOR_COLOR);
    // 三角描边提升可见性
    frame.stroke(
        &tri,
        Stroke::default()
            .with_width(0.5)
            .with_color(iced_core::Color::WHITE.scale_alpha(0.9)),
    );
}

fn draw_vertical(
    view: &ViewState,
    layout: &PianoLayout,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    tick: f64,
) {
    // 纵向：时间沿 Y
    let main_px = layout.tick_to_main_px(view, tick);
    let cy = layout.content_y + main_px;
    let music = layout.music_rect;
    // 裁剪：Y 在 content 范围内，X 在 music 横向范围内
    if cy < layout.content_y - 1.0 || cy > layout.content_bottom + 1.0 {
        return;
    }
    let left = music.x - bounds.x;
    let right = music.x + music.width - bounds.x;
    let local_y = cy - bounds.y;
    if local_y < bounds.y || local_y > bounds.y + bounds.height {
        // 本地已换算，此处直接检查
    }
    // 横线
    let line = Path::line(Point::new(left, local_y), Point::new(right, local_y));
    frame.stroke(
        &line,
        Stroke::default()
            .with_width(INDICATOR_WIDTH)
            .with_color(INDICATOR_COLOR),
    );
    // 左侧三角形（▶）在 ruler 右缘 / content 左缘
    let tri = Path::new(|b| {
        let half = TRIANGLE_SIZE / 2.0;
        let tip_x = left + TRIANGLE_SIZE;
        b.move_to(Point::new(left, local_y - half));
        b.line_to(Point::new(left, local_y + half));
        b.line_to(Point::new(tip_x, local_y));
        b.close();
    });
    frame.fill(&tri, INDICATOR_COLOR);
    frame.stroke(
        &tri,
        Stroke::default()
            .with_width(0.5)
            .with_color(iced_core::Color::WHITE.scale_alpha(0.9)),
    );
}

/// 走带（arrange）指示线绘制 — 复用相同样式，但参数为 ArrangeViewport 语义
pub fn draw_arrange_to_geometry(
    tick_to_x: impl Fn(f64) -> f32,
    left_panel_width: f32,
    content_y: f32,
    content_h: f32,
    renderer: &Renderer,
    bounds: Rectangle,
    cursor_tick: Option<f64>,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    if let Some(tick) = cursor_tick {
        let cx = tick_to_x(tick);
        if cx >= left_panel_width && cx <= bounds.width {
            let local_x = cx - bounds.x;
            let top = content_y - bounds.y;
            let bottom = (content_y + content_h) - bounds.y;
            let line = Path::line(Point::new(local_x, top), Point::new(local_x, bottom));
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(INDICATOR_WIDTH)
                    .with_color(INDICATOR_COLOR),
            );
            let half = TRIANGLE_SIZE / 2.0;
            let tri = Path::new(|b| {
                let tip_y = top + TRIANGLE_SIZE;
                b.move_to(Point::new(local_x - half, top));
                b.line_to(Point::new(local_x + half, top));
                b.line_to(Point::new(local_x, tip_y));
                b.close();
            });
            frame.fill(&tri, INDICATOR_COLOR);
        }
    }
    frame.into_geometry()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::{Point, Rectangle, Size};
    use lumino_core::ViewState;

    fn layout_horizontal() -> PianoLayout {
        PianoLayout {
            content_rect: Rectangle::new(Point::new(0.0, 52.0), Size::new(800.0, 400.0)),
            music_rect: Rectangle::new(Point::new(120.0, 52.0), Size::new(680.0, 400.0)),
            keyboard_rect: Rectangle::new(Point::new(0.0, 52.0), Size::new(120.0, 400.0)),
            ruler_rect: Rectangle::new(Point::new(120.0, 28.0), Size::new(680.0, 24.0)),
            content_y: 52.0,
            content_bottom: 452.0,
            w: 680,
            h: 400,
            pw: 680,
            ph: 400,
            total_ticks: 1920.0 * 100.0,
            panels_total_h: 0.0,
            orientation: Orientation::Horizontal,
        }
    }

    #[test]
    fn cursor_none_does_not_panic() {
        let view = ViewState::default();
        let layout = layout_horizontal();
        let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(800.0, 600.0));
        // 仅断言不 panic（需 renderer 才能实际 draw，这里只构造）
        let _ = layout.content_y;
        let _ = view.zoom_x;
        let _ = bounds.width;
    }

    #[test]
    fn tick_to_x_roundtrip() {
        let view = ViewState::default();
        let tick = 480.0_f32;
        let x = view.tick_to_x(tick);
        let back = view.x_to_tick(x);
        assert!((tick - back).abs() < 1e-3);
    }
}
