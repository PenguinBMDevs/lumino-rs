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
fn test_box_handles_are_strictly_eight_plus_optional_center() {
    for model in [line(), curve()] {
        let eight = box_handles(&model, zoom(), false).expect("应有箱体手柄");
        assert_eq!(eight.handles.len(), 8, "严格 8 个箱体锚点");
        assert!(
            eight.handles.iter().all(|(kind, _)| !matches!(
                kind,
                HandleKind::Move | HandleKind::Path | HandleKind::AnchorHandle(_)
            )),
            "箱体锚点不得混入中心/Path 点：{:?}",
            eight.handles
        );

        let nine = box_handles(&model, zoom(), true).expect("应有箱体手柄");
        assert_eq!(nine.handles.len(), 9, "8 + 1 中心");
        assert!(matches!(nine.handles[8].0, HandleKind::Move));
    }
}

#[test]
fn test_hit_test_box_ignores_path_points() {
    let curve = curve();
    // 曲线中间锚点 (60, 64) 在完整手柄集里可命中路径点（优先于同位置的边锚点）
    assert!(matches!(
        hit_test(&curve, (60.0, 64.0), zoom(), 4.0),
        Some(HandleKind::AnchorHandle(_)) | Some(HandleKind::Path)
    ));
    // 箱体命中只认 4 角 + 4 边 + 可选中心：远离锚点的位置不命中
    assert_eq!(
        hit_test_box(&curve, (100.0, 62.0), zoom(), 4.0, false),
        None
    );
    assert_eq!(
        hit_test_box(&curve, (0.0, 64.0), zoom(), 4.0, false),
        Some(HandleKind::ResizeCorner(Corner::Nw))
    );
    assert_eq!(
        hit_test_box(&curve, (60.0, 60.0), zoom(), 4.0, true),
        Some(HandleKind::Side(Side::Bottom))
    );
    assert_eq!(
        hit_test_box(&curve, (60.0, 62.0), zoom(), 4.0, true),
        Some(HandleKind::Move)
    );
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
fn test_turn_with_ppq_scale_keeps_geometry_sane() {
    let mut shape = line();
    apply_drag(
        &mut shape,
        HandleKind::Turn,
        (120.0, 60.0),
        (70.0, 121.0),
        Mods::default(),
        1920.0,
    );
    let (x0, y0, x1, y1) = shape.geometry.bounds().expect("应有 bounds");
    assert!(x0.abs() < 1.0e7 && x1.abs() < 1.0e7, "tick 不应爆炸");
    assert!(
        (-256.0..=512.0).contains(&y0) && (-256.0..=512.0).contains(&y1),
        "key 不应被 k² 放大"
    );
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
