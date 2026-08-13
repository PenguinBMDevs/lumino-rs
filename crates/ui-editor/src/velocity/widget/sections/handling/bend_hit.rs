//! 弯音路径命中测试与坐标转换
//!
//! 与卷帘曲线工具（`interaction/line_tool/hit_test.rs`）同构：
//! 命中优先级 控制柄 > 锚点 > 曲线段；逻辑坐标 (tick, value) ↔ 面板局部
//! 屏幕坐标转换；tick 吸附、value 取整。

use iced_core::Point;
use lumino_gfx::automation::AutomationViewParams;

use crate::interaction::line_tool::geom;
use crate::velocity::widget::bend_path::{BendPathState, HandleSide};

use super::super::super::super::HIT_RADIUS;

/// 控制柄命中半径（像素）
const HANDLE_HIT_RADIUS: f32 = 8.0;
/// 控制柄与锚点重合判定阈值（像素）：重合时柄不参与命中（拖动锚点）
const HANDLE_COINCIDE_THRESHOLD_PX: f32 = 6.0;
/// 曲线段命中阈值（像素）
const LINE_HIT_THRESHOLD: f32 = 8.0;

/// 命中类型（弯音路径）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BendHit {
    Anchor { idx: usize },
    Handle { idx: usize, side: HandleSide },
    Segment { segment: usize },
}

/// 弯音逻辑坐标 → 面板局部屏幕坐标（tick 取整、value 直接映射）
pub(super) fn bend_screen_pos(view: &AutomationViewParams, pos: (f32, f32), max_val: f32) -> Point {
    Point::new(
        view.tick_to_x(pos.0.round() as u32),
        view.value_to_y(pos.1, max_val),
    )
}

/// 命中测试（控制柄 > 锚点 > 曲线段）
pub(super) fn bend_hit_test(
    state: &BendPathState,
    view: &AutomationViewParams,
    cursor_pos: Point,
    max_val: f32,
) -> Option<BendHit> {
    // 1. 控制柄（首锚点 out、尾锚点 in、中间 in+out；柄与锚点重合时不参与）
    for (idx, anchor) in state.anchors.iter().enumerate() {
        let ap = bend_screen_pos(view, anchor.pos, max_val);
        for side in state.visible_handle_sides(idx) {
            let h_abs = match side {
                HandleSide::In => anchor.in_handle_abs(),
                HandleSide::Out => anchor.out_handle_abs(),
            };
            let hp = bend_screen_pos(view, h_abs, max_val);
            if (hp.x - ap.x).hypot(hp.y - ap.y) < HANDLE_COINCIDE_THRESHOLD_PX {
                continue;
            }
            if (cursor_pos.x - hp.x).hypot(cursor_pos.y - hp.y) <= HANDLE_HIT_RADIUS {
                return Some(BendHit::Handle { idx, side });
            }
        }
    }
    // 2. 锚点
    for (idx, anchor) in state.anchors.iter().enumerate() {
        let ap = bend_screen_pos(view, anchor.pos, max_val);
        if (cursor_pos.x - ap.x).hypot(cursor_pos.y - ap.y) <= HIT_RADIUS {
            return Some(BendHit::Anchor { idx });
        }
    }
    // 3. 曲线段（采样折线逼近）
    for (si, pair) in state.anchors.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let pa = bend_screen_pos(view, a.pos, max_val);
        let p1 = bend_screen_pos(view, a.out_handle_abs(), max_val);
        let p2 = bend_screen_pos(view, b.in_handle_abs(), max_val);
        let pb = bend_screen_pos(view, b.pos, max_val);
        if geom::point_curve_distance(cursor_pos, pa, p1, p2, pb) <= LINE_HIT_THRESHOLD {
            return Some(BendHit::Segment { segment: si });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::velocity::widget::bend_path::BendAnchor;

    fn view() -> AutomationViewParams {
        AutomationViewParams {
            // 与面板约定一致：panel_height = canvas 高度、toolbar_height = 0
            panel_height: 150.0,
            pixels_per_tick: 1.0,
            scroll_x: 0.0,
            keyboard_width: 0.0,
            value_zoom: 1.0,
            value_scroll: 0.0,
            panel_offset_x: 0.0,
            panel_offset_y: 0.0,
            toolbar_height: 0.0,
            line_thickness: 2.0,
        }
    }

    /// 两点直线路径：tick 0→960，value 8192（y = 面板中部）
    fn straight_path() -> BendPathState {
        let mut state = BendPathState {
            anchors: vec![
                BendAnchor::new((0.0, 8192.0)),
                BendAnchor::new((960.0, 8192.0)),
            ],
            ..Default::default()
        };
        state.recompute_auto_handles();
        state
    }

    #[test]
    fn test_hit_anchor() {
        let path = straight_path();
        let v = view();
        // 锚点屏幕位置：(0, y) 与 (960, y)
        let a0 = bend_screen_pos(&v, path.anchors[0].pos, 16383.0);
        let hit = bend_hit_test(&path, &v, a0, 16383.0);
        assert_eq!(hit, Some(BendHit::Anchor { idx: 0 }));
    }

    #[test]
    fn test_hit_segment_center() {
        let path = straight_path();
        let v = view();
        // 段中点（屏幕）：曲线中点 x=480, y=8192
        let mid = bend_screen_pos(&v, (480.0, 8192.0), 16383.0);
        let hit = bend_hit_test(&path, &v, mid, 16383.0);
        assert!(matches!(hit, Some(BendHit::Segment { segment: 0 })));
    }

    #[test]
    fn test_hit_blank_returns_none() {
        let path = straight_path();
        let v = view();
        // 远离路径的空白处（x=100 但 y 偏移很大）
        let blank = bend_screen_pos(&v, (100.0, 8192.0), 16383.0);
        let far = Point::new(blank.x, blank.y - 100.0);
        assert!(bend_hit_test(&path, &v, far, 16383.0).is_none());
    }

    #[test]
    fn test_hit_handle_after_bend() {
        // 弯曲后控制柄出现在锚点旁 → 优先命中柄
        let mut path = straight_path();
        path.anchors[0].set_out_handle((320.0, -200.0));
        let v = view();
        let hp = bend_screen_pos(&v, path.anchors[0].out_handle_abs(), 16383.0);
        let hit = bend_hit_test(&path, &v, hp, 16383.0);
        assert!(
            matches!(
                hit,
                Some(BendHit::Handle {
                    idx: 0,
                    side: HandleSide::Out
                })
            ),
            "应命中出向柄，实际 {hit:?}"
        );
    }
}
