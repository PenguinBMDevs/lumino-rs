//! Custom edit 变换测试

use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel};

use super::*;

fn line() -> ShapeModel {
    let mut shape = ShapeModel::new(1, 1, ShapeKind::Line, 1.0);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (120.0, 60.0)]);
    shape
}

fn curve() -> ShapeModel {
    let mut shape = ShapeModel::new(2, 1, ShapeKind::Curve, 1.0);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (60.0, 64.0), (120.0, 60.0)]);
    shape
}

fn zoom() -> Zoom {
    Zoom { x: 1.0, y: 1.0 }
}

#[test]
fn test_hit_test_corner_and_anchor() {
    let line = line();
    assert!(matches!(
        hit_test(&line, (0.0, 60.0), zoom(), 4.0),
        Some(HandleKind::ResizeCorner(_))
    ));
    let curve = curve();
    assert_eq!(
        hit_test(&curve, (60.0, 64.0), zoom(), 4.0),
        Some(HandleKind::AnchorHandle(1))
    );
}

#[test]
fn test_move_translates() {
    let mut shape = line();
    assert!(apply_drag(
        &mut shape,
        HandleKind::Move,
        (0.0, 60.0),
        (10.0, 62.0),
        Mods::default(),
        1.0
    ));
    assert_eq!(shape.geometry.anchors[0].pos, (10.0, 62.0));
    assert_eq!(shape.geometry.anchors[1].pos, (130.0, 62.0));
}

#[test]
fn test_ctrl_corner_keeps_aspect() {
    let mut shape = curve();
    apply_drag(
        &mut shape,
        HandleKind::ResizeCorner(Corner::Se),
        (120.0, 60.0),
        (180.0, 72.0),
        Mods {
            ctrl: true,
            shift: false,
        },
        1.0,
    );
    let (x0, y0, x1, y1) = shape.geometry.bounds().expect("应有 bounds");
    let ratio = (x1 - x0) / (y1 - y0);
    assert!((ratio - 30.0).abs() < 1e-3, "Ctrl 应保持 120:4 比例");
}

#[test]
fn test_side_only_moves_one_edge() {
    let mut shape = line();
    apply_drag(
        &mut shape,
        HandleKind::Side(Side::Left),
        (0.0, 60.0),
        (-60.0, 60.0),
        Mods::default(),
        1.0,
    );
    let (x0, _, x1, _) = shape.geometry.bounds().expect("应有 bounds");
    assert!((x0 + 60.0).abs() < 1e-3, "左边缘应到 -60");
    assert!((x1 - 120.0).abs() < 1e-3, "右边缘应不动");
}

#[test]
fn test_turn_snaps_15_degrees_and_shift_disables() {
    let mut snapped = line();
    apply_drag(
        &mut snapped,
        HandleKind::Turn,
        (120.0, 60.0),
        (70.0, 121.0),
        Mods::default(),
        1.0,
    );
    let mut free = line();
    apply_drag(
        &mut free,
        HandleKind::Turn,
        (120.0, 60.0),
        (70.0, 121.0),
        Mods {
            ctrl: false,
            shift: true,
        },
        1.0,
    );
    assert_ne!(snapped.geometry.anchors, free.geometry.anchors);
}

#[test]
fn test_skew_is_integer_grid() {
    let mut shape = line();
    apply_drag(
        &mut shape,
        HandleKind::Skew,
        (0.0, 60.0),
        (10.4, 60.0),
        Mods::default(),
        1.0,
    );
    let dx = shape.geometry.anchors[0].pos.0;
    assert!((dx - 10.0 * (60.0 - 60.0)).abs() < 1e-6, "中心行不应移动");
}

#[test]
fn test_move_updates_copy_along_group_anchor() {
    let mut shape = line();
    shape.ops.push(lumino_core::shape::ShapeOp::CopyAlong(
        lumino_core::shape::CopyPathParams {
            pts: vec![[240.0, 0.0]],
            grp: Some([0.0, 60.0]),
            ..lumino_core::shape::CopyPathParams::default()
        },
    ));
    apply_drag(
        &mut shape,
        HandleKind::Move,
        (0.0, 60.0),
        (10.0, 64.0),
        Mods::default(),
        1.0,
    );
    let lumino_core::shape::ShapeOp::CopyAlong(params) = &shape.ops[0] else {
        panic!("应为 CopyAlong");
    };
    assert_eq!(params.grp, Some([10.0, 64.0]), "路径起点必须跟随变换");
}

#[test]
fn test_resize_scales_copy_along_path_vectors() {
    let mut shape = line();
    shape.ops.push(lumino_core::shape::ShapeOp::CopyAlong(
        lumino_core::shape::CopyPathParams {
            pts: vec![[100.0, 0.0]],
            grp: Some([0.0, 60.0]),
            ..lumino_core::shape::CopyPathParams::default()
        },
    ));
    apply_drag(
        &mut shape,
        HandleKind::Side(Side::Left),
        (0.0, 60.0),
        (-60.0, 60.0),
        Mods::default(),
        1.0,
    );
    let lumino_core::shape::ShapeOp::CopyAlong(params) = &shape.ops[0] else {
        panic!("应为 CopyAlong");
    };
    assert_eq!(params.grp, Some([-60.0, 60.0]), "组起点跟随缩放");
    assert!(
        (params.pts[0][0] - 150.0).abs() < 1e-3,
        "相对路径向量也必须缩放，否则副本会扭曲到旧位置"
    );
}

#[test]
fn test_anchor_handle_moves_only_target() {
    let mut shape = curve();
    apply_drag(
        &mut shape,
        HandleKind::AnchorHandle(1),
        (60.0, 64.0),
        (70.0, 66.0),
        Mods::default(),
        1.0,
    );
    assert_eq!(shape.geometry.anchors[1].pos, (70.0, 66.0));
    assert_eq!(shape.geometry.anchors[0].pos, (0.0, 60.0));
}
