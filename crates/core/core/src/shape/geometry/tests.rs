//! 形状几何测试

use super::*;

#[test]
fn test_anchor_new_defaults_to_auto_handles() {
    let anchor = ShapeAnchor::new((100.0, 60.0));
    assert_eq!(anchor.pos, (100.0, 60.0));
    assert_eq!(anchor.in_handle, (0.0, 0.0));
    assert_eq!(anchor.out_handle, (0.0, 0.0));
    assert!(anchor.handles_auto, "新锚点应为自动柄");
}

#[test]
fn test_anchor_handle_absolute_and_finiteness() {
    let mut anchor = ShapeAnchor::new((100.0, 60.0));
    anchor.out_handle = (10.0, 2.0);
    anchor.in_handle = (-10.0, -2.0);
    assert_eq!(anchor.out_handle_abs(), (110.0, 62.0));
    assert_eq!(anchor.in_handle_abs(), (90.0, 58.0));
    assert!(anchor.is_finite());

    anchor.pos = (f32::INFINITY, 60.0);
    assert!(!anchor.is_finite(), "无穷坐标应判定为非有限");
}

#[test]
fn test_anchor_set_handles_marks_custom_and_swaps() {
    let mut anchor = ShapeAnchor::new((100.0, 60.0));
    anchor.set_out_handle((10.0, 2.0));
    anchor.set_in_handle((-10.0, -2.0));
    assert!(!anchor.handles_auto, "设置手柄后应转为自定义");
    assert_eq!(anchor.out_handle_abs(), (110.0, 62.0));
    assert_eq!(anchor.in_handle_abs(), (90.0, 58.0));

    anchor.swap_handles();
    assert_eq!(anchor.out_handle, (-10.0, -2.0));
    assert_eq!(anchor.in_handle, (10.0, 2.0));
    assert!(!anchor.handles_auto, "交换不应改变自定义标记");
}

#[test]
fn test_rect_normalized_swaps_bounds() {
    let rect = ShapeRect::normalized(480.0, 72.0, 0.0, 60.0);
    assert_eq!((rect.tick_lo, rect.key_lo), (0.0, 60.0));
    assert_eq!((rect.tick_hi, rect.key_hi), (480.0, 72.0));
    assert!(rect.is_normalized());
    assert_eq!(rect.size(), (480.0, 12.0));
    assert!(rect.is_finite());
}

#[test]
fn test_rect_not_normalized_detected() {
    let rect = ShapeRect {
        tick_lo: 480.0,
        key_lo: 72.0,
        tick_hi: 0.0,
        key_hi: 60.0,
    };
    assert!(!rect.is_normalized(), "反向框应被判定为未规范化");
}

#[test]
fn test_geometry_from_points_builds_auto_anchors() {
    let geometry = ShapeGeometry::from_points([(0.0, 60.0), (120.0, 64.0)]);
    assert_eq!(geometry.anchors.len(), 2);
    assert!(geometry.anchors.iter().all(|a| a.handles_auto));
    assert_eq!(geometry.rect, None);
    assert!(!geometry.closed, "点列构造默认不闭合");
    assert!(!geometry.is_empty());
}

#[test]
fn test_geometry_from_rect_is_closed_and_empty_of_points() {
    let geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 480.0, 72.0));
    assert!(geometry.anchors.is_empty());
    assert!(geometry.closed, "纯框几何默认闭合");
    assert!(!geometry.is_empty());
}

#[test]
fn test_default_geometry_is_empty_without_bounds() {
    let geometry = ShapeGeometry::default();
    assert!(geometry.is_empty());
    assert_eq!(geometry.bounds(), None);
}

#[test]
fn test_geometry_bounds_covers_anchors_and_rect() {
    let mut geometry = ShapeGeometry::from_points([(10.0, 60.0), (50.0, 70.0)]);
    assert_eq!(geometry.bounds(), Some((10.0, 60.0, 50.0, 70.0)));

    geometry.rect = Some(ShapeRect::normalized(0.0, 55.0, 30.0, 80.0));
    assert_eq!(
        geometry.bounds(),
        Some((0.0, 55.0, 50.0, 80.0)),
        "包围盒应同时覆盖点列与外接框"
    );
}

#[test]
fn test_geometry_translate_moves_anchors_and_rect() {
    let mut geometry = ShapeGeometry::from_points([(10.0, 60.0)]);
    geometry.rect = Some(ShapeRect::normalized(0.0, 55.0, 30.0, 80.0));
    geometry.translate(5.0, -2.0);
    assert_eq!(geometry.anchors[0].pos, (15.0, 58.0));
    let rect = geometry.rect.expect("外接框应存在");
    assert_eq!((rect.tick_lo, rect.key_lo), (5.0, 53.0));
    assert_eq!((rect.tick_hi, rect.key_hi), (35.0, 78.0));
}
