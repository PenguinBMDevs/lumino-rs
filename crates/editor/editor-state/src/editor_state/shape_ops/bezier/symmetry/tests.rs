//! 半边对称测试

use super::*;
use lumino_core::shape::{ShapeGeometry, SymAxis};

fn anchors() -> Vec<ShapeAnchor> {
    vec![
        ShapeAnchor::new((0.0, 60.0)),
        ShapeAnchor::new((50.0, 64.0)),
        ShapeAnchor::new((100.0, 70.0)),
    ]
}

fn params(mode: Sym) -> SymParams {
    SymParams {
        mode,
        axis: SymAxis::Vertical { x: 100.0 },
        source: 0,
    }
}

#[test]
fn test_mirror_has_2n_minus_1_and_keeps_half() {
    let input = anchors();
    let out = symmetric(&input, &params(Sym::Mirror)).expect("应可展开");
    assert_eq!(out.len(), 5);
    assert_eq!(out[0].pos, (0.0, 60.0), "起点不动");
    assert_eq!(out[2].pos, (100.0, 70.0), "轴上的接缝锚点不动");
    assert_eq!(out[3].pos, (150.0, 64.0));
    assert_eq!(out[4].pos, (200.0, 60.0));
    assert_eq!(
        out[0].pos.0 + out[4].pos.0,
        200.0,
        "镜像点应关于 x=100 对称"
    );
}

#[test]
fn test_turn_rotates_half_around_pivot() {
    let out = symmetric(&anchors(), &params(Sym::Turn)).expect("应可展开");
    assert_eq!(out.len(), 5);
    assert_eq!(out[3].pos, (150.0, 76.0));
    assert_eq!(out[4].pos, (200.0, 80.0));
}

#[test]
fn test_even_anchors_and_zero_axis_rejected() {
    let even = vec![
        ShapeAnchor::new((0.0, 60.0)),
        ShapeAnchor::new((100.0, 70.0)),
    ];
    assert!(symmetric(&even, &params(Sym::Mirror)).is_none());
    let mut bad = params(Sym::Mirror);
    bad.axis = SymAxis::Custom {
        origin: (0.0, 0.0),
        dir: (0.0, 0.0),
    };
    assert!(symmetric(&anchors(), &bad).is_none());
}

#[test]
fn test_handles_are_reflected_and_seam_flattened() {
    let mut input = anchors();
    input[0].set_out_handle((10.0, 2.0));
    input[1].set_in_handle((-4.0, -1.0));
    input[1].set_out_handle((6.0, 3.0));
    input[2].set_out_handle((3.0, 4.0));
    let out = symmetric(&input, &params(Sym::Mirror)).expect("应可展开");
    // 反向：末锚点的 in 柄来自 a0 的 out 柄，关于竖轴反射后 x 取反
    assert_eq!(out[4].in_handle, (-10.0, 2.0));
    // 接缝锚点的出柄压平到竖直轴方向：只剩 y 分量
    assert_eq!(out[2].out_handle, (0.0, 4.0));
    assert!(!out[2].handles_auto);
    // 后半第一段的入柄也压平
    assert_eq!(out[3].in_handle.0, 0.0);
}

#[test]
fn test_shape_entry_rejects_joined_and_non_curve() {
    let mut model = ShapeModel::new(1, 1, ShapeKind::Curve, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    assert!(symmetric_shape(&model, &params(Sym::Mirror)).is_some());

    model
        .ops
        .push(ShapeOp::Joined(lumino_core::shape::JoinedParams {
            gaps: Vec::new(),
            tumours: vec![None],
            splits: Vec::new(),
            src: vec![1],
        }));
    assert!(symmetric_shape(&model, &params(Sym::Mirror)).is_none());

    let mut line = ShapeModel::new(2, 1, ShapeKind::Line, 1.0);
    line.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    assert!(symmetric_shape(&line, &params(Sym::Mirror)).is_none());
}
