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
fn test_corner_drag_past_opposite_edge_does_not_mirror() {
    // Nw 角：固定点在 (x1, y0)；向下拖过底边时比例会变负 → 曾经直接上下镜像
    let mut shape = curve();
    assert!(apply_drag(
        &mut shape,
        HandleKind::ResizeCorner(Corner::Nw),
        (0.0, 64.0),
        (0.0, 58.0),
        Mods::default(),
        1.0
    ));
    let keys: Vec<f32> = shape.geometry.anchors.iter().map(|a| a.pos.1).collect();
    assert!(
        keys[1] >= keys[0] && keys[1] >= keys[2],
        "向下拖过对边不得上下镜像：{keys:?}"
    );

    // Ne 角：固定点在 (x0, y0)；向左拖过左边时同样不得左右镜像
    let mut shape = curve();
    assert!(apply_drag(
        &mut shape,
        HandleKind::ResizeCorner(Corner::Ne),
        (120.0, 64.0),
        (-60.0, 64.0),
        Mods::default(),
        1.0
    ));
    let ticks: Vec<f32> = shape.geometry.anchors.iter().map(|a| a.pos.0).collect();
    assert!(
        ticks[1] >= ticks[0] && ticks[2] >= ticks[1],
        "向左拖过对边不得左右镜像：{ticks:?}"
    );
}

#[test]
fn test_corner_drag_past_edge_with_ctrl_does_not_mirror() {
    let mut shape = curve();
    assert!(apply_drag(
        &mut shape,
        HandleKind::ResizeCorner(Corner::Nw),
        (0.0, 64.0),
        (0.0, 58.0),
        Mods {
            ctrl: true,
            shift: false,
        },
        1.0
    ));
    let keys: Vec<f32> = shape.geometry.anchors.iter().map(|a| a.pos.1).collect();
    assert!(keys[1] >= keys[0], "Ctrl 等比也不得镜像：{keys:?}");
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

fn rect_model(x0: f32, y0: f32, x1: f32, y1: f32, id: u64) -> ShapeModel {
    let mut model = ShapeModel::new(id, 1, ShapeKind::Custom, 1.0);
    model.geometry = ShapeGeometry::from_rect(lumino_core::shape::ShapeRect {
        tick_lo: x0,
        key_lo: y0,
        tick_hi: x1,
        key_hi: y1,
    });
    model
}

fn group() -> Vec<ShapeModel> {
    vec![
        rect_model(0.0, 60.0, 100.0, 64.0, 1),
        rect_model(200.0, 60.0, 300.0, 64.0, 2),
    ]
}

#[test]
fn test_union_model_is_joint_box() {
    let models = group();
    let union = union_model(&models).expect("应有联合 box");
    let (x0, y0, x1, y1) = bounds(&union).expect("联合模型应有 bounds");
    assert_eq!((x0, y0, x1, y1), (0.0, 60.0, 300.0, 64.0));
}

#[test]
fn test_group_move_keeps_relative_layout() {
    let mut models = group();
    assert!(apply_group_drag(
        &mut models,
        HandleKind::Move,
        (0.0, 0.0),
        (50.0, 2.0),
        Mods::default(),
        1.0
    ));
    for model in &models {
        assert_eq!(
            model.geometry.rect.expect("rect").tick_lo,
            50.0 + if model.id == 2 { 200.0 } else { 0.0 }
        );
        assert_eq!(model.geometry.rect.expect("rect").key_lo, 62.0);
    }
}

#[test]
fn test_group_resize_scales_each_member_about_joint_box() {
    let mut models = group();
    // 联合 box = (0,60)-(300,64)；Se 固定 (0,60)，x 拉到 600 ⇒ sx=2，sy=1
    assert!(apply_group_drag(
        &mut models,
        HandleKind::ResizeCorner(Corner::Se),
        (300.0, 60.0),
        (600.0, 60.0),
        Mods::default(),
        1.0
    ));
    let a = models[0].geometry.rect.expect("rect");
    let b = models[1].geometry.rect.expect("rect");
    assert!((a.tick_lo - 0.0).abs() < 1e-3 && (a.tick_hi - 200.0).abs() < 1e-3);
    assert!((b.tick_lo - 400.0).abs() < 1e-3 && (b.tick_hi - 600.0).abs() < 1e-3);
    // y 方向不变
    assert!((a.key_lo - 60.0).abs() < 1e-3 && (a.key_hi - 64.0).abs() < 1e-3);
}

#[test]
fn test_group_turn_keeps_member_distance() {
    let models = group();
    let mut turned = models.clone();
    let center = |model: &ShapeModel| {
        let rect = model.geometry.rect.expect("rect");
        (
            (rect.tick_lo + rect.tick_hi) / 2.0,
            (rect.key_lo + rect.key_hi) / 2.0,
        )
    };
    let before = dist2(center(&models[0]), center(&models[1]));
    assert!(apply_group_drag(
        &mut turned,
        HandleKind::Turn,
        (300.0, 62.0),
        (150.0, 212.0),
        Mods::default(),
        1.0
    ));
    let after = dist2(center(&turned[0]), center(&turned[1]));
    assert!(
        (before - after).abs() < 1e-2,
        "旋转应保持成员中心距离：before={before}, after={after}"
    );
    assert_ne!(turned, models, "90° 旋转必须改变成员参数");
}
