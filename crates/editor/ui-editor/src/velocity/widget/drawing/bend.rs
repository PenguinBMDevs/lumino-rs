//! 弯音贝塞尔路径绘制
//!
//! 曲线本体由 host 层 gfx `build_lane_instances` 渲染（实时生效），
//! 本地绘制补充交互视觉（全部基于本地 `bend_path` 状态，不依赖
//! lane 异步同步，保证锚点立即可见）：
//! - 全部锚点小圆（未选中 3px）；
//! - 全部锚点的控制柄（方块 + 锚点→柄辅助线，选中锚点的柄高亮）；
//! - 选中锚点高亮（大圆 + 白色描边）。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Path, Stroke};

use crate::velocity::widget::bend_path::{BendPathState, HandleSide};
use crate::{Renderer, Theme};

use lumino_gfx::automation::AutomationViewParams;

/// 未选中锚点半径（像素，与 gfx 层 `ANCHOR_RADIUS` 一致）
const ANCHOR_RADIUS: f32 = 3.0;
/// 选中锚点半径（像素，比普通锚点大）
const SELECTED_ANCHOR_RADIUS: f32 = 6.0;
/// 选中锚点描边宽度（像素）
const ANCHOR_STROKE_WIDTH: f32 = 2.0;
/// 控制柄方块边长（像素）
const HANDLE_SIZE: f32 = 7.0;
/// 控制柄辅助线宽度（像素）
const HANDLE_LINE_WIDTH: f32 = 1.0;
/// 选中锚点控制柄的辅助线宽度（像素）
const SELECTED_HANDLE_LINE_WIDTH: f32 = 2.0;

/// 弯音逻辑坐标 → 面板局部屏幕坐标（tick 取整、value 直接映射）
fn bend_screen_pos(view: &AutomationViewParams, pos: (f32, f32), max_val: f32) -> Point {
    Point::new(
        view.tick_to_x(pos.0.round() as u32),
        view.value_to_y(pos.1, max_val),
    )
}

/// 绘制弯音路径交互视觉：控制柄 + 选中锚点高亮
pub fn draw_bend_path(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    state: &BendPathState,
    view: &AutomationViewParams,
    max_val: f32,
) {
    if state.anchors.is_empty() {
        return;
    }
    let anchor_color = theme.extended_palette().primary.strong.color;
    let white = Color::WHITE;

    // 全部锚点小圆：未选中 3px（gfx 层基于 lane 异步渲染，本地先画保证
    // 可见且不闪烁）；选中锚点最后画（大圆覆盖小圆）
    for (idx, anchor) in state.anchors.iter().enumerate() {
        if state.selected == Some(idx) {
            continue;
        }
        let ap = bend_screen_pos(view, anchor.pos, max_val);
        let path = Path::circle(ap, ANCHOR_RADIUS);
        frame.fill(&path, anchor_color);
    }

    // 控制柄（辅助线 + 方块；选中锚点的柄高亮加粗）
    for (idx, anchor) in state.anchors.iter().enumerate() {
        let selected = state.selected == Some(idx);
        let ap = bend_screen_pos(view, anchor.pos, max_val);
        for side in state.visible_handle_sides(idx) {
            let h_abs = match side {
                HandleSide::In => anchor.in_handle_abs(),
                HandleSide::Out => anchor.out_handle_abs(),
            };
            let hp = bend_screen_pos(view, h_abs, max_val);
            // 锚点 → 控制柄辅助线
            let handle_connector = Path::new(|p| {
                p.move_to(ap);
                p.line_to(hp);
            });
            let connector_width = if selected {
                SELECTED_HANDLE_LINE_WIDTH
            } else {
                HANDLE_LINE_WIDTH
            };
            frame.stroke(
                &handle_connector,
                Stroke::default().with_width(connector_width).with_color(Color {
                    a: if selected { 0.9 } else { 0.5 },
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

    // 选中锚点高亮（大圆 + 白色描边；未选中锚点由 gfx 渲染小圆）
    if let Some(idx) = state.selected
        && let Some(anchor) = state.anchors.get(idx)
    {
        let ap = bend_screen_pos(view, anchor.pos, max_val);
        let path = Path::circle(ap, SELECTED_ANCHOR_RADIUS);
        frame.fill(&path, anchor_color);
        frame.stroke(
            &path,
            Stroke::default()
                .with_width(ANCHOR_STROKE_WIDTH)
                .with_color(white),
        );
    }
}
