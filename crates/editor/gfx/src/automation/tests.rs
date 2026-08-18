//! 自动化实例生成测试

use crate::automation::{AutomationViewParams, build_lane_instances};
use lumino_note_core::AutomationTarget;
use lumino_note_core::automation::{AutomationEvent, AutomationLane, SegmentShape};

fn make_lane(ticks: &[u32], values: &[u16]) -> AutomationLane {
    AutomationLane {
        target: AutomationTarget::CC { controller: 7 },
        track: 0,
        channel: 0,
        events: ticks
            .iter()
            .zip(values.iter())
            .map(|(&tick, &value)| AutomationEvent::new(tick, value, SegmentShape::Step))
            .collect(),
    }
}

#[test]
fn test_value_to_y_and_back() {
    let view = AutomationViewParams {
        panel_height: 100.0,
        pixels_per_tick: 1.0,
        scroll_x: 0.0,
        keyboard_width: 0.0,
        value_zoom: 1.0,
        value_scroll: 0.0,
        panel_offset_x: 0.0,
        panel_offset_y: 0.0,
        toolbar_height: 28.0,
        line_thickness: 2.0,
    };
    assert!((view.value_to_y(0.0, 127.0) - 100.0).abs() < 1e-3);
    assert!((view.value_to_y(127.0, 127.0) - 28.0).abs() < 1e-3);
    assert!((view.y_to_value(view.value_to_y(64.0, 127.0), 127.0) - 64.0).abs() < 1e-3);
}

#[test]
fn test_build_lane_instances_step() {
    let lane = make_lane(&[0, 100], &[0, 127]);
    let view = AutomationViewParams {
        panel_height: 100.0,
        pixels_per_tick: 1.0,
        scroll_x: 0.0,
        keyboard_width: 0.0,
        value_zoom: 1.0,
        value_scroll: 0.0,
        panel_offset_x: 0.0,
        panel_offset_y: 0.0,
        toolbar_height: 28.0,
        line_thickness: 2.0,
    };
    let mut out = Vec::new();
    build_lane_instances(&mut out, 200.0, &view, &lane, [1.0, 1.0, 1.0], false);
    assert!(!out.is_empty(), "应生成 Step 线段");
}

#[test]
fn test_lane_instances_visible_when_anchors_off_viewport() {
    // 回归：锚点（事件）在视口外、但贝塞尔控制柄把曲线延伸到视口内时，
    // 曲线必须仍然渲染（事件窗口需按柄的 tick 偏移扩展）。
    // 场景：事件在 tick 10000（视口外右侧），入向柄向左拉 9000 tick，
    // 曲线延伸到 tick 1000（视口内）。
    let view = AutomationViewParams {
        panel_height: 100.0,
        pixels_per_tick: 1.0,
        scroll_x: 0.0,
        keyboard_width: 0.0,
        value_zoom: 1.0,
        value_scroll: 0.0,
        panel_offset_x: 0.0,
        panel_offset_y: 0.0,
        toolbar_height: 28.0,
        line_thickness: 2.0,
    };
    let mut evt = AutomationEvent::new(10_000, 8192, SegmentShape::Curve { tension: 0 });
    evt.set_in_handle((-9000.0, -3000.0)); // 入向柄向左延伸 9000 tick
    let mut lane = AutomationLane {
        target: AutomationTarget::PitchBend,
        track: 0,
        channel: 0,
        events: vec![evt],
    };
    lane.recompute_auto_handles();
    // 视口 0..200 tick（事件在 10000，远超视口）
    let mut out = Vec::new();
    build_lane_instances(&mut out, 200.0, &view, &lane, [1.0, 1.0, 1.0], false);
    assert!(
        !out.is_empty(),
        "柄延伸进视口的曲线必须渲染（事件窗口已扩展）"
    );
    // 生成的线段应覆盖视口区域（有 x < 200 的实例）
    assert!(
        out.iter().any(|i| i.position[0] < 200.0),
        "曲线应包含视口内的线段"
    );
}

#[test]
fn test_lane_instances_off_viewport_no_handle_still_hidden() {
    // 无柄延伸时：事件全部在视口外 → 只渲染 chase 水平线（保持前一事件值），
    // 不产生任何曲线段（斜线/竖线）
    let view = AutomationViewParams {
        panel_height: 100.0,
        pixels_per_tick: 1.0,
        scroll_x: 0.0,
        keyboard_width: 0.0,
        value_zoom: 1.0,
        value_scroll: 0.0,
        panel_offset_x: 0.0,
        panel_offset_y: 0.0,
        toolbar_height: 28.0,
        line_thickness: 2.0,
    };
    let lane = make_lane(&[10_000, 10_100], &[8192, 9000]);
    let mut out = Vec::new();
    build_lane_instances(&mut out, 200.0, &view, &lane, [1.0, 1.0, 1.0], false);
    assert!(!out.is_empty(), "chase 水平线应存在（保持前一事件值 8192）");
    // 全部为水平线（高度 = 线粗 2px）；无斜线/竖线（高度 > 2）
    assert!(
        out.iter().all(|i| i.size[1] <= 2.0 + 0.01),
        "视口外事件只应有 chase 水平线: {out:?}"
    );
}
