//! `automation` 模块单元测试

use super::*;

fn make_lane(target: AutomationTarget, ticks: &[u32]) -> AutomationLane {
    AutomationLane {
        target,
        track: 0,
        channel: 0,
        events: ticks
            .iter()
            .map(|&t| AutomationEvent::new(t, 64, SegmentShape::Step))
            .collect(),
    }
}

#[test]
fn test_events_in_range() {
    let lane = make_lane(
        AutomationTarget::CC { controller: 7 },
        &[100, 200, 300, 400, 500],
    );
    let slice = lane.events_in_range(150, 450);
    assert_eq!(slice.len(), 3);
    assert_eq!(slice[0].tick, 200);
    assert_eq!(slice[2].tick, 400);
}

#[test]
fn test_events_in_range_empty() {
    let lane = make_lane(AutomationTarget::CC { controller: 7 }, &[100, 200]);
    assert!(lane.events_in_range(300, 400).is_empty());
}

#[test]
fn test_chase_value_found() {
    let lane = AutomationLane {
        target: AutomationTarget::CC { controller: 7 },
        track: 0,
        channel: 0,
        events: vec![
            AutomationEvent::new(100, 80, SegmentShape::Step),
            AutomationEvent::new(200, 100, SegmentShape::Step),
            AutomationEvent::new(300, 60, SegmentShape::Step),
        ],
    };
    assert_eq!(lane.chase_value(250), Some(100));
    assert_eq!(lane.chase_value(300), Some(100));
}

#[test]
fn test_chase_value_none() {
    let lane = make_lane(AutomationTarget::CC { controller: 7 }, &[200, 300]);
    assert_eq!(lane.chase_value(100), None);
}

#[test]
fn test_segment_shape_interpolate_endpoints() {
    let lin0 = SegmentShape::Curve { tension: 0 };
    assert_eq!(lin0.interpolate(0.0), 0.0);
    assert_eq!(lin0.interpolate(1.0), 1.0);
    assert!((lin0.interpolate(0.5) - 0.5).abs() < 1e-6);

    assert_eq!(SegmentShape::Step.interpolate(0.5), 0.0);
}

#[test]
fn test_curve_direction() {
    let ease_in = SegmentShape::Curve { tension: 127 }.interpolate(0.5);
    assert!(ease_in < 0.5);

    let ease_out = SegmentShape::Curve { tension: -127 }.interpolate(0.5);
    assert!(ease_out > 0.5);
}

#[test]
fn test_target_max_and_default_values() {
    assert_eq!(AutomationTarget::CC { controller: 0 }.max_value(), 127);
    assert_eq!(AutomationTarget::CC { controller: 0 }.default_value(), 0);
    assert_eq!(AutomationTarget::CC { controller: 10 }.default_value(), 64);
    assert_eq!(AutomationTarget::PitchBend.max_value(), 16383);
    assert_eq!(
        AutomationTarget::PitchBend.default_value(),
        PITCH_BEND_CENTER as u16
    );
    assert!(AutomationTarget::PitchBend.has_center_line());
    assert!(!AutomationTarget::CC { controller: 7 }.has_center_line());
}

#[test]
fn test_default_shape_per_target() {
    for cc in [64u8, 65, 66, 67, 68] {
        assert_eq!(
            AutomationTarget::CC { controller: cc }.default_shape(),
            SegmentShape::Step,
            "CC {cc} should default to Step"
        );
    }
    for cc in [0u8, 1, 7, 10, 11, 71, 74] {
        assert_eq!(
            AutomationTarget::CC { controller: cc }.default_shape(),
            SegmentShape::Curve { tension: 0 },
            "CC {cc} should default to Curve{{tension:0}}"
        );
    }
}

#[test]
fn test_automation_event_with_default_shape() {
    let evt = AutomationEvent::with_default_shape(100, 64, &AutomationTarget::CC { controller: 7 });
    assert_eq!(evt.shape, SegmentShape::Curve { tension: 0 });

    let evt2 =
        AutomationEvent::with_default_shape(100, 0, &AutomationTarget::CC { controller: 64 });
    assert_eq!(evt2.shape, SegmentShape::Step);
}

#[test]
fn test_set_handle_clamps_tick_offset() {
    // 出向柄：tick 偏移不允许 < 0（越过锚点垂直切线 = 曲线回环）
    let mut a = AutomationEvent::new(0, 8192, SegmentShape::Curve { tension: 0 });
    a.set_out_handle((-500.0, 3000.0));
    assert_eq!(a.out_handle.0, 0.0, "出向柄 tick 偏移被钳制为 0");
    assert_eq!(a.out_handle.1, 3000.0, "value 偏移不受限");

    // 入向柄：tick 偏移不允许 > 0
    let mut b = AutomationEvent::new(960, 8192, SegmentShape::Curve { tension: 0 });
    b.set_in_handle((500.0, -3000.0));
    assert_eq!(b.in_handle.0, 0.0, "入向柄 tick 偏移被钳制为 0");
    assert_eq!(b.in_handle.1, -3000.0);

    // 合法偏移不受影响
    let mut c = AutomationEvent::new(0, 8192, SegmentShape::Curve { tension: 0 });
    c.set_out_handle((320.0, 3000.0));
    assert_eq!(c.out_handle.0, 320.0);
    let mut d = AutomationEvent::new(960, 8192, SegmentShape::Curve { tension: 0 });
    d.set_in_handle((-320.0, -3000.0));
    assert_eq!(d.in_handle.0, -320.0);
}
