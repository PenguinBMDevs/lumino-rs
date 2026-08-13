//! 弯音贝塞尔路径绘制（参考卷帘曲线工具的 line_tool_box 视觉）
//!
//! - √× 确认模式：全量绘制本地路径（贝塞尔段 4px + 锚点圆 + 控制柄方块 +
//!   锚点→柄辅助线）+ √/× 悬浮按钮；
//! - 实时生效模式：仅绘制"绘制中"的 ghost 预览（起点→当前点直线 + 圆点），
//!   已提交曲线由 host 层 gfx `build_lane_instances` 渲染，避免双重绘制。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Frame, Path, Stroke, path};

use crate::velocity::widget::bend_path::{BendInteraction, BendPathState};
use crate::{Renderer, Theme};

use lumino_gfx::automation::AutomationViewParams;

use super::super::sections::handling::bend::{BEND_BUTTON_SIZE, BUTTON_SPACING, BendButtonRects};

/// 曲线段粗细（像素，与卷帘 line_tool_box 一致）
const LINE_WIDTH: f32 = 4.0;
/// 锚点半径（像素）
const ANCHOR_RADIUS: f32 = 5.0;
/// 锚点描边宽度（像素）
const ANCHOR_STROKE_WIDTH: f32 = 2.0;
/// 控制柄方块边长（像素）
const HANDLE_SIZE: f32 = 7.0;
/// 控制柄辅助线宽度（像素）
const HANDLE_LINE_WIDTH: f32 = 1.0;

/// 弯音逻辑坐标 → 面板局部屏幕坐标（tick 取整、value 直接映射）
fn bend_screen_pos(view: &AutomationViewParams, pos: (f32, f32), max_val: f32) -> Point {
    Point::new(
        view.tick_to_x(pos.0.round() as u32),
        view.value_to_y(pos.1, max_val),
    )
}

/// 绘制弯音贝塞尔路径
pub fn draw_bend_path(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    state: &BendPathState,
    view: &AutomationViewParams,
    max_val: f32,
    confirm_mode: bool,
    line_thickness: f32,
) {
    let anchor_color = theme.extended_palette().primary.strong.color;
    let white = Color::WHITE;

    // 实时模式：仅绘制中的 ghost 预览（已提交曲线由 gfx 渲染）
    if !confirm_mode {
        if state.interaction == BendInteraction::Drawing
            && let Some(cur) = state.current
        {
            let start = bend_screen_pos(view, state.draw_start, max_val);
            let end = bend_screen_pos(view, cur, max_val);
            let ghost = Color {
                a: 0.6,
                ..anchor_color
            };
            let mut b = path::Builder::new();
            b.move_to(start);
            b.line_to(end);
            frame.stroke(
                &b.build(),
                Stroke::default()
                    .with_width(line_thickness)
                    .with_color(ghost),
            );
            frame.fill(&Path::circle(start, 4.0), ghost);
            frame.fill(&Path::circle(end, 4.0), ghost);
        }
        return;
    }

    // √× 确认模式：全量绘制路径（曲线段 + 控制柄 + 锚点）
    if state.anchors.len() >= 2 {
        for pair in state.anchors.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let pa = bend_screen_pos(view, a.pos, max_val);
            let p1 = bend_screen_pos(view, a.out_handle_abs(), max_val);
            let p2 = bend_screen_pos(view, b.in_handle_abs(), max_val);
            let pb = bend_screen_pos(view, b.pos, max_val);
            let curve = Path::new(|p| {
                p.move_to(pa);
                p.bezier_curve_to(p1, p2, pb);
            });
            frame.stroke(
                &curve,
                Stroke::default()
                    .with_width(LINE_WIDTH)
                    .with_color(anchor_color),
            );
        }
    }

    // 控制柄（辅助线 + 方块，常驻显示）
    for (idx, anchor) in state.anchors.iter().enumerate() {
        let ap = bend_screen_pos(view, anchor.pos, max_val);
        for side in state.visible_handle_sides(idx) {
            let h_abs = match side {
                crate::velocity::widget::bend_path::HandleSide::In => anchor.in_handle_abs(),
                crate::velocity::widget::bend_path::HandleSide::Out => anchor.out_handle_abs(),
            };
            let hp = bend_screen_pos(view, h_abs, max_val);
            // 锚点 → 控制柄辅助线
            let aux = Path::new(|p| {
                p.move_to(ap);
                p.line_to(hp);
            });
            frame.stroke(
                &aux,
                Stroke::default()
                    .with_width(HANDLE_LINE_WIDTH)
                    .with_color(Color {
                        a: 0.5,
                        ..anchor_color
                    }),
            );
            // 控制柄方块
            let rect = Rectangle::new(
                Point::new(hp.x - HANDLE_SIZE * 0.5, hp.y - HANDLE_SIZE * 0.5),
                Size::new(HANDLE_SIZE, HANDLE_SIZE),
            );
            let path = Path::rectangle(rect.position(), rect.size());
            frame.fill(&path, anchor_color);
            frame.stroke(&path, Stroke::default().with_width(1.0).with_color(white));
        }
    }

    // 锚点：实心圆 + 白色描边
    for anchor in &state.anchors {
        let ap = bend_screen_pos(view, anchor.pos, max_val);
        let path = Path::circle(ap, ANCHOR_RADIUS);
        frame.fill(&path, anchor_color);
        frame.stroke(
            &path,
            Stroke::default()
                .with_width(ANCHOR_STROKE_WIDTH)
                .with_color(white),
        );
    }
}

/// 绘制 √/× 悬浮按钮（√× 确认模式 + 完整路径时显示）
pub fn draw_bend_confirm_buttons(
    frame: &mut Frame<Renderer>,
    _theme: &crate::Theme,
    state: &BendPathState,
    view: &AutomationViewParams,
    max_val: f32,
    bounds: Size,
) {
    let Some(rects) = bend_button_screen_rects(view, state, max_val, bounds) else {
        return;
    };
    draw_simple_button(frame, rects.confirm, "✓", Color::from_rgb8(46, 125, 50));
    draw_simple_button(frame, rects.cancel, "×", Color::from_rgb8(198, 40, 40));
}

/// 绘制单个悬浮按钮（圆角矩形底色 + 白色图标字符）
fn draw_simple_button(frame: &mut Frame<Renderer>, rect: Rectangle, icon: &str, color: Color) {
    let path = Path::rounded_rectangle(
        rect.position(),
        rect.size(),
        iced_core::border::Radius::from(4.0),
    );
    frame.fill(&path, color);
    // 白色图标字符
    frame.fill_text(canvas::Text {
        content: icon.into(),
        position: Point::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
        max_width: rect.width,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(14.0),
        color: Color::WHITE,
        font: iced_core::Font::DEFAULT,
        align_x: iced_core::alignment::Horizontal::Center.into(),
        align_y: iced_core::alignment::Vertical::Center,
        shaping: iced_core::text::Shaping::Basic,
    });
}

/// √× 按钮矩形（面板局部坐标）：完整路径包围盒右侧垂直居中，钳制到画布内
pub fn bend_button_screen_rects(
    view: &AutomationViewParams,
    state: &BendPathState,
    max_val: f32,
    bounds: Size,
) -> Option<BendButtonRects> {
    if !state.is_complete() {
        return None;
    }
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for a in &state.anchors {
        let p = bend_screen_pos(view, a.pos, max_val);
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
        if !a.handles_auto {
            for h in [a.out_handle_abs(), a.in_handle_abs()] {
                let hp = bend_screen_pos(view, h, max_val);
                min_x = min_x.min(hp.x);
                max_x = max_x.max(hp.x);
                min_y = min_y.min(hp.y);
                max_y = max_y.max(hp.y);
            }
        }
    }
    if !(min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite()) {
        return None;
    }
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;
    let group_w = BEND_BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 钳制到画布内
    let x0 = (mid_x + 8.0).clamp(0.0, (bounds.width - group_w).max(0.0));
    let y0 =
        (mid_y - BEND_BUTTON_SIZE * 0.5).clamp(0.0, (bounds.height - BEND_BUTTON_SIZE).max(0.0));
    let confirm = Rectangle::new(
        Point::new(x0, y0),
        Size::new(BEND_BUTTON_SIZE, BEND_BUTTON_SIZE),
    );
    let cancel = Rectangle::new(
        Point::new(x0 + BEND_BUTTON_SIZE + 8.0, y0),
        Size::new(BEND_BUTTON_SIZE, BEND_BUTTON_SIZE),
    );
    Some(BendButtonRects { confirm, cancel })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::velocity::widget::bend_path::BendPathState;

    fn view() -> AutomationViewParams {
        AutomationViewParams {
            panel_height: 150.0,
            pixels_per_tick: 1.0,
            scroll_x: 0.0,
            keyboard_width: 0.0,
            value_zoom: 1.0,
            value_scroll: 0.0,
            panel_offset_x: 0.0,
            panel_offset_y: 0.0,
            toolbar_height: 28.0,
            line_thickness: 2.0,
        }
    }

    fn two_anchor_path() -> BendPathState {
        BendPathState {
            anchors: vec![
                crate::velocity::widget::bend_path::BendAnchor::new((0.0, 8192.0)),
                crate::velocity::widget::bend_path::BendAnchor::new((960.0, 8192.0)),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn test_button_rects_incomplete_path_returns_none() {
        let state = BendPathState {
            anchors: vec![crate::velocity::widget::bend_path::BendAnchor::new((
                0.0, 8192.0,
            ))],
            ..Default::default()
        };
        assert!(
            bend_button_screen_rects(&view(), &state, 16383.0, Size::new(800.0, 150.0)).is_none()
        );
    }

    #[test]
    fn test_button_rects_complete_path_side_by_side() {
        let state = two_anchor_path();
        let rects = bend_button_screen_rects(&view(), &state, 16383.0, Size::new(800.0, 150.0));
        let rects = rects.expect("完整路径应返回按钮");
        // 两个按钮并排、尺寸一致、y 对齐
        assert_eq!(rects.confirm.size(), rects.cancel.size());
        assert_eq!(rects.confirm.y, rects.cancel.y);
        assert_eq!(rects.confirm.width, BEND_BUTTON_SIZE);
        assert!(rects.cancel.x > rects.confirm.x);
    }

    #[test]
    fn test_button_rects_clamped_to_canvas() {
        // 路径靠近画布右边缘 → 按钮钳制在画布内
        let state = BendPathState {
            anchors: vec![
                crate::velocity::widget::bend_path::BendAnchor::new((790.0, 8192.0)),
                crate::velocity::widget::bend_path::BendAnchor::new((795.0, 9000.0)),
            ],
            ..Default::default()
        };
        let rects = bend_button_screen_rects(&view(), &state, 16383.0, Size::new(800.0, 150.0));
        let rects = rects.expect("应返回按钮");
        assert!(rects.confirm.x >= 0.0);
        assert!(rects.cancel.x + BEND_BUTTON_SIZE <= 800.0);
    }
}
