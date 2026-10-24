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

#[test]
fn test_off_axis_seam_anchor_is_projected_onto_axis() {
    // DRAW-043：接缝（中间）锚点原本偏离竖轴 x=100，展开后必须落在轴上（1e-6）
    let mut input = anchors();
    input[2].pos = (100.4, 70.0);
    let out = symmetric(&input, &params(Sym::Mirror)).expect("应可展开");
    let seam = out[2].pos;
    assert!(
        (seam.0 - 100.0).abs() < 1e-6,
        "中间锚点必须落在对称线上，实际 {seam:?}"
    );
    assert!(
        ((out[0].pos.0 + out[4].pos.0) * 0.5 - 100.0).abs() < 1e-6,
        "两端点仍应关于该轴严格对称"
    );
}

#[test]
fn test_reject_reasons_are_distinguishable() {
    // DRAW-043：拒绝必须给出**具体**原因，不能一律 None
    let two = vec![
        ShapeAnchor::new((0.0, 60.0)),
        ShapeAnchor::new((100.0, 70.0)),
    ];
    assert_eq!(
        symmetric_with_reason(&two, &params(Sym::Mirror)).err(),
        Some(SymReject::TooFewAnchors),
        "2 个锚点应报「太少」而不是「偶数」"
    );

    let four: Vec<ShapeAnchor> = (0..4)
        .map(|index| ShapeAnchor::new((index as f32 * 10.0, 60.0)))
        .collect();
    assert_eq!(
        symmetric_with_reason(&four, &params(Sym::Mirror)).err(),
        Some(SymReject::EvenAnchors)
    );

    let mut bad = params(Sym::Mirror);
    bad.axis = SymAxis::Custom {
        origin: (0.0, 0.0),
        dir: (0.0, 0.0),
    };
    assert_eq!(
        symmetric_with_reason(&anchors(), &bad).err(),
        Some(SymReject::InvalidParams),
        "零方向轴在 validate 阶段就被拒（早于 Axis 构造）"
    );

    // validate 只查「非零」，极短但非零的方向能过校验、在 Axis 构造处退化
    let mut tiny = params(Sym::Mirror);
    tiny.axis = SymAxis::Custom {
        origin: (0.0, 0.0),
        dir: (2e-7, 0.0),
    };
    assert_eq!(
        symmetric_with_reason(&anchors(), &tiny).err(),
        Some(SymReject::DegenerateAxis)
    );

    let mut line = ShapeModel::new(9, 1, ShapeKind::Line, 1.0);
    line.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    assert_eq!(
        symmetric_shape_with_reason(&line, &params(Sym::Mirror)).err(),
        Some(SymReject::NotCurve)
    );

    let mut joined = ShapeModel::new(10, 1, ShapeKind::Curve, 1.0);
    joined.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    joined
        .ops
        .push(ShapeOp::Joined(lumino_core::shape::JoinedParams {
            gaps: Vec::new(),
            tumours: vec![None],
            splits: Vec::new(),
            src: vec![1],
        }));
    assert_eq!(
        symmetric_shape_with_reason(&joined, &params(Sym::Mirror)).err(),
        Some(SymReject::JoinedCurve),
        "已连接曲线必须报 JoinedCurve 而不是 NotCurve"
    );
}
