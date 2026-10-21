//! 形状对象与修饰链测试

use super::*;
use crate::shape::{
    ChopParams, ClawParams, CycleBy, CycleParams, FunnelParams, GateRangeParams, GlueParams,
    JoinedParams, LiveShapeParams, MaskMode, MaskParams, MaskShow, MergeParams, NestedParams,
    OpStage, PatternParams, PolyStyle, PolygonParams, RangeDir, RowRestart, ShapeFill,
    ShapeGeometry, ShapeKind, ShapeRect, ShapeStyle, StopsParams, Stroke, StrokeKind, StrumParams,
    SustainParams, Sym, SymAxis, SymParams, TilesParams, TumourParams,
};

fn sample_shape() -> ShapeModel {
    let mut shape = ShapeModel::new(7, 2, ShapeKind::Curve, 0.25);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (120.0, 64.0), (480.0, 72.0)]);
    shape.geometry.closed = false;
    shape.style = ShapeStyle {
        fill: ShapeFill::Spam,
        gate_beats: 0.125,
        stroke_width: 3,
        velocity: 96,
        colour_slot: 4,
        ..ShapeStyle::default()
    };
    shape.push_op(ShapeOp::Noop);
    shape.push_op(ShapeOp::Noop);
    shape.name = "wave".into();
    shape
}

#[test]
fn test_shape_model_bincode_roundtrip() {
    let shape = sample_shape();
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape);
    assert_eq!(back.ops.len(), 2, "修饰链长度应保真");
    assert_eq!(back.k, 0.25, "视图比例 k 应保真");
}

#[test]
fn test_shape_model_json_roundtrip() {
    let shape = sample_shape();
    let text = serde_json::to_string(&shape).expect("json 序列化应成功");
    let back: ShapeModel = serde_json::from_str(&text).expect("json 反序列化应成功");
    assert_eq!(back, shape);
}

#[test]
fn test_new_shape_has_empty_ops_and_default_style() {
    let shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25);
    assert!(shape.ops.is_empty());
    assert_eq!(shape.style, ShapeStyle::default());
    assert_eq!(shape.geometry, ShapeGeometry::default());
    assert!(!shape.hidden);
    assert!(shape.name.is_empty());
}

#[test]
fn test_builder_helpers_set_geometry_and_style() {
    let style = ShapeStyle {
        fill: ShapeFill::Fill,
        ..ShapeStyle::default()
    };
    let geometry = ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0)]);
    let shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25)
        .with_geometry(geometry.clone())
        .with_style(style.clone());
    assert_eq!(shape.geometry, geometry);
    assert_eq!(shape.style, style);
}

#[test]
fn test_validate_accepts_minimal_valid_shape() {
    sample_shape().validate().expect("合法形状应通过校验");
}

#[test]
fn test_validate_accepts_rect_only_shape() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Custom, 0.25);
    shape.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 480.0, 72.0));
    shape.validate().expect("纯框形状应通过校验");
}

#[test]
fn test_validate_rejects_empty_geometry() {
    let shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25);
    let err = shape.validate().expect_err("空几何应被拒绝");
    assert!(format!("{err}").contains("几何为空"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_k() {
    for bad in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
        let mut shape = sample_shape();
        shape.k = bad;
        let err = shape.validate().expect_err("非法视图比例应被拒绝");
        assert!(
            format!("{err}").contains("视图比例 k 非法"),
            "实际错误：{err}"
        );
    }
}

#[test]
fn test_validate_rejects_non_finite_anchor() {
    let mut shape = sample_shape();
    shape.geometry.anchors[1].pos = (f32::NAN, 64.0);
    assert!(shape.validate().is_err(), "NaN 锚点应被拒绝");
}

#[test]
fn test_validate_rejects_out_of_range_key() {
    let mut shape = sample_shape();
    shape.geometry.anchors[0].pos = (0.0, 300.0);
    let err = shape.validate().expect_err("音高越界应被拒绝");
    assert!(format!("{err}").contains("音高越界"), "实际错误：{err}");

    let mut shape = sample_shape();
    shape.geometry.anchors[0].pos = (0.0, -1.0);
    assert!(shape.validate().is_err(), "负音高应被拒绝");
}

#[test]
fn test_validate_rejects_negative_tick() {
    let mut shape = sample_shape();
    shape.geometry.anchors[2].pos = (-1.0, 72.0);
    let err = shape.validate().expect_err("负 tick 应被拒绝");
    assert!(format!("{err}").contains("tick 为负"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_unnormalized_rect() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Custom, 0.25);
    shape.geometry.rect = Some(ShapeRect {
        tick_lo: 480.0,
        key_lo: 72.0,
        tick_hi: 0.0,
        key_hi: 60.0,
    });
    let err = shape.validate().expect_err("未规范化外接框应被拒绝");
    assert!(format!("{err}").contains("未规范化"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_rect_with_out_of_range_key() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Custom, 0.25);
    shape.geometry.rect = Some(ShapeRect {
        tick_lo: 0.0,
        key_lo: 0.0,
        tick_hi: 480.0,
        key_hi: 300.0,
    });
    let err = shape.validate().expect_err("外接框音高越界应被拒绝");
    assert!(
        format!("{err}").contains("外接框音高越界"),
        "实际错误：{err}"
    );
}

#[test]
fn test_validate_rejects_bad_style() {
    let mut shape = sample_shape();
    shape.style.gate_beats = 0.0;
    assert!(shape.validate().is_err(), "零门长应被拒绝");
}

#[test]
fn test_op_chain_order_is_preserved() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0)]);
    shape.push_op(ShapeOp::Noop);
    shape.push_op(ShapeOp::Noop);
    shape.push_op(ShapeOp::Noop);
    assert_eq!(shape.ops.len(), 3);
    assert_eq!(ShapeOp::default(), ShapeOp::Noop);
}

#[test]
fn test_op_stage_classification() {
    assert_eq!(ShapeOp::Noop.stage(), OpStage::Geometry);
    assert_eq!(
        ShapeOp::Tumours(TumourParams::default()).stage(),
        OpStage::Geometry,
        "Tumours 是生成音符之前的 Geometry 阶段算子"
    );
    assert_eq!(
        ShapeOp::Glue(GlueParams::All).stage(),
        OpStage::Notes,
        "Glue 是生成音符之后的 Notes 阶段算子"
    );
    assert_eq!(
        ShapeOp::Claw(ClawParams::default()).stage(),
        OpStage::Notes,
        "Claw 是生成音符之后的 Notes 阶段算子"
    );
    assert_eq!(
        ShapeOp::Strum(StrumParams::default()).stage(),
        OpStage::Notes,
        "Strum 是生成音符之后的 Notes 阶段算子"
    );
    assert_eq!(
        ShapeOp::Chop(ChopParams::default()).stage(),
        OpStage::Notes,
        "Chop 是生成音符之后的 Notes 阶段算子"
    );
    assert_eq!(
        ShapeOp::GateFlip.stage(),
        OpStage::Notes,
        "GateFlip 是生成音符之后的 Notes 阶段算子"
    );
    assert_eq!(
        ShapeOp::Sustain(SustainParams::default()).stage(),
        OpStage::Notes,
        "Sustain 是生成音符之后的 Notes 阶段算子"
    );
    assert_eq!(
        ShapeOp::Mask(MaskParams::default()).stage(),
        OpStage::Notes,
        "Mask 是生成音符之后的 Notes 阶段标记"
    );
    assert_eq!(
        ShapeOp::Cycle(CycleParams::default()).stage(),
        OpStage::Notes,
        "Cycle 是生成音符之后的 Notes 阶段算子"
    );
    assert_eq!(
        ShapeOp::GateRange(GateRangeParams::default()).stage(),
        OpStage::Notes,
        "GateRange 是生成音符之后的 Notes 阶段标记"
    );
    assert_eq!(
        ShapeOp::Polygon(PolygonParams::default()).stage(),
        OpStage::Geometry,
        "Polygon 是几何来源，必须先于音符生成"
    );
    assert_eq!(
        ShapeOp::Pattern(PatternParams::default()).stage(),
        OpStage::Geometry,
        "Pattern 改路径，必须先于音符生成"
    );
    assert_eq!(
        ShapeOp::Funnel(funnel_params()).stage(),
        OpStage::Geometry,
        "Funnel 是斜 box 几何来源，必须先于音符生成"
    );
    assert_eq!(
        ShapeOp::Nested(NestedParams::default()).stage(),
        OpStage::Notes,
        "Nested 是内部填充，生成后消费"
    );
    assert_eq!(
        ShapeOp::Tiles(TilesParams::default()).stage(),
        OpStage::Notes,
        "Tiles 是内部填充，生成后消费"
    );
    assert_eq!(
        ShapeOp::Live(live_params()).stage(),
        OpStage::Geometry,
        "Live 是几何折叠，必须先于音符生成"
    );
    assert_eq!(
        ShapeOp::Joined(joined_params()).stage(),
        OpStage::Geometry,
        "Joined 是几何串联，必须先于音符生成"
    );
    assert_eq!(
        ShapeOp::Merge(MergeParams::default()).stage(),
        OpStage::Notes,
        "Merge 在音符生成后逐行滑移"
    );
    assert_eq!(
        ShapeOp::Symmetry(symmetry_params()).stage(),
        OpStage::Geometry,
        "Symmetry 必须在音符生成前展开"
    );
}

fn symmetry_params() -> SymParams {
    SymParams {
        mode: Sym::Mirror,
        axis: SymAxis::Vertical { x: 100.0 },
        source: 0,
    }
}

fn joined_params() -> JoinedParams {
    JoinedParams {
        gaps: Vec::new(),
        tumours: vec![None],
        splits: Vec::new(),
        src: vec![1],
    }
}

fn live_params() -> LiveShapeParams {
    let mut source = sample_shape();
    source.ops.clear();
    LiveShapeParams {
        strokes: vec![Stroke {
            kind: StrokeKind::Curve,
            anchors: source.geometry.anchors.clone(),
            rect: source.geometry.rect,
        }],
        src: vec![u32::try_from(source.id).unwrap_or(0)],
        from: vec![source],
    }
}

fn funnel_params() -> FunnelParams {
    FunnelParams {
        lines: vec![[[0.0, 20.0], [100.0, 20.0]]],
        wall: [[100.0, 10.0], [100.0, 30.0]],
        starts: Vec::new(),
        fill: ShapeFill::Spam,
        reverse: false,
    }
}

#[test]
fn test_validate_rejects_bad_tumour_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Tumours(TumourParams {
        dist: 0.0,
        ..TumourParams::default()
    }));
    let err = shape.validate().expect_err("dist=0 的 Tumours 应被拒绝");
    assert!(format!("{err}").contains("Tumours"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_glue_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Glue(GlueParams::Boxes(vec![[0.0, 0.0, 1.5, 1.0]])));
    let err = shape.validate().expect_err("越界 Glue 框应被拒绝");
    assert!(format!("{err}").contains("Glue"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_claw_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Claw(ClawParams {
        keep: 0,
        ..ClawParams::default()
    }));
    let err = shape.validate().expect_err("keep=0 的 Claw 应被拒绝");
    assert!(format!("{err}").contains("Claw"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_strum_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Strum(StrumParams {
        time: 5.0,
        ..StrumParams::default()
    }));
    let err = shape.validate().expect_err("time 越界的 Strum 应被拒绝");
    assert!(format!("{err}").contains("Strum"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_chop_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Chop(ChopParams {
        steps: 0,
        ..ChopParams::default()
    }));
    let err = shape.validate().expect_err("steps=0 的 Chop 应被拒绝");
    assert!(format!("{err}").contains("Chop"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_sustain_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Sustain(SustainParams {
        most_beats: Some(0.0),
        ..SustainParams::default()
    }));
    let err = shape.validate().expect_err("most=0 的 Sustain 应被拒绝");
    assert!(format!("{err}").contains("Sustain"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_mask_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Mask(MaskParams {
        size_keys: -1.0,
        ..MaskParams::default()
    }));
    let err = shape.validate().expect_err("size_keys=-1 的 Mask 应被拒绝");
    assert!(format!("{err}").contains("Mask"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_cycle_params() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Cycle(CycleParams {
        n: 1,
        ..CycleParams::default()
    }));
    let err = shape.validate().expect_err("n=1 的 Cycle 应被拒绝");
    assert!(format!("{err}").contains("Cycle"), "实际错误：{err}");
}

#[test]
fn test_shape_model_roundtrip_with_merge() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Merge(MergeParams::default()));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Merge 的形状应逐字段往返");
}

#[test]
fn test_validate_and_roundtrip_symmetry() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Symmetry(symmetry_params()));
    shape.validate().expect("合法 Symmetry 应通过");
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Symmetry 的形状应逐字段往返");

    let mut bad = sample_shape();
    bad.push_op(ShapeOp::Symmetry(SymParams {
        axis: SymAxis::Custom {
            origin: (0.0, 0.0),
            dir: (0.0, 0.0),
        },
        ..symmetry_params()
    }));
    assert!(bad.validate().is_err());
}

#[test]
fn test_validate_rejects_bad_joined() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Joined(JoinedParams {
        src: Vec::new(),
        tumours: Vec::new(),
        gaps: Vec::new(),
        splits: Vec::new(),
    }));
    assert!(shape.validate().is_err());
}

#[test]
fn test_shape_model_roundtrip_with_joined() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Joined(joined_params()));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Joined 的形状应逐字段往返");
}

#[test]
fn test_validate_rejects_bad_live() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Live(LiveShapeParams {
        strokes: Vec::new(),
        src: Vec::new(),
        from: Vec::new(),
    }));
    assert!(shape.validate().is_err());
}

#[test]
fn test_shape_model_roundtrip_with_live() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Live(live_params()));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Live 的形状应逐字段往返");
}

#[test]
fn test_validate_rejects_bad_nested_and_tiles() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Nested(NestedParams {
        steps: 0,
        ..NestedParams::default()
    }));
    assert!(shape.validate().is_err());

    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Tiles(TilesParams {
        length: 0.0,
        ..TilesParams::default()
    }));
    assert!(shape.validate().is_err());
}

#[test]
fn test_shape_model_roundtrip_with_nested_and_tiles() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Nested(NestedParams::default()));
    shape.push_op(ShapeOp::Tiles(TilesParams::default()));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Nested/Tiles 的形状应逐字段往返");
}

#[test]
fn test_validate_rejects_bad_funnel() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Funnel(FunnelParams {
        fill: ShapeFill::Fill,
        ..funnel_params()
    }));
    let err = shape.validate().expect_err("Fill 的 Funnel 应被拒绝");
    assert!(format!("{err}").contains("Funnel"), "实际错误：{err}");
}

#[test]
fn test_shape_model_roundtrip_with_funnel() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Funnel(funnel_params()));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Funnel 的形状应逐字段往返");
}

#[test]
fn test_validate_rejects_bad_pattern() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Pattern(PatternParams {
        loops: 0.0,
        ..PatternParams::default()
    }));
    let err = shape.validate().expect_err("loops=0 的 Pattern 应被拒绝");
    assert!(format!("{err}").contains("Pattern"), "实际错误：{err}");
}

#[test]
fn test_shape_model_roundtrip_with_pattern() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Pattern(PatternParams::default()));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Pattern 的形状应逐字段往返");
}

#[test]
fn test_validate_rejects_bad_polygon() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Polygon(PolygonParams {
        points: 2,
        ..PolygonParams::default()
    }));
    let err = shape.validate().expect_err("points=2 的 Polygon 应被拒绝");
    assert!(format!("{err}").contains("Polygon"), "实际错误：{err}");
}

#[test]
fn test_shape_model_roundtrip_with_polygon() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Polygon(PolygonParams {
        points: 7,
        style: PolyStyle::Star,
        inner: 42.5,
        skip: 3,
        shape: None,
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Polygon 的形状应逐字段往返");
}

#[test]
fn test_validate_rejects_bad_gate_range() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::GateRange(GateRangeParams {
        to: 0.0,
        ..GateRangeParams::default()
    }));
    let err = shape.validate().expect_err("to=0 的 GateRange 应被拒绝");
    assert!(format!("{err}").contains("GateRange"), "实际错误：{err}");
}

#[test]
fn test_shape_model_roundtrip_with_gate_range() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::GateRange(GateRangeParams {
        to: 1.5,
        graph: vec![[0.0, 0.0], [0.5, 0.25], [1.0, 1.0]],
        dir: RangeDir::KeysDown,
        fit: true,
        rows: true,
        join: true,
        halves: true,
        follow_curve: true,
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 GateRange 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_cycle() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Cycle(CycleParams {
        by: CycleBy::Time,
        n: 7,
        every: 3,
        every_time: Some([3, 8]),
        rows: Some(RowRestart::End),
        shift: Some([2, 5]),
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Cycle 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_mask() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Mask(MaskParams {
        size_keys: 7.0,
        k: 0.125,
        invert: true,
        own: false,
        show: MaskShow::Line,
        group: Some(42),
        mode: MaskMode::Intersect,
        colour: 5,
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Mask 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_sustain() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Sustain(SustainParams {
        gap_beats: Some(0.25),
        most_beats: Some(2.0),
        every_beats: Some(1.0),
        from_beats: Some(0.5),
        stops: Some(StopsParams {
            at: vec![0.5],
            past: true,
            off: false,
        }),
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Sustain 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_gate_flip() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::GateFlip);
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 GateFlip 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_chop() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Chop(ChopParams {
        name: "triplets".into(),
        steps: 1,
        ..ChopParams::builtin("triplets").expect("内置节奏")
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Chop 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_strum() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Strum(StrumParams {
        time: 0.25,
        alternate: true,
        ..StrumParams::default()
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Strum 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_claw() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Claw(ClawParams {
        keep: 3,
        skip: 2,
        ..ClawParams::default()
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Claw 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_glue() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Glue(GlueParams::Boxes(vec![[0.0, 0.0, 0.5, 1.0]])));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Glue 的形状应逐字段往返");
}

#[test]
fn test_shape_model_roundtrip_with_tumours() {
    let mut shape = sample_shape();
    shape.push_op(ShapeOp::Tumours(TumourParams {
        size: 6.0,
        side: crate::shape::TumourSide::Random,
        ..TumourParams::default()
    }));
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape, "含 Tumours 的形状应逐字段往返");
}

#[test]
fn test_decode_garbage_returns_error_not_panic() {
    let bytes = [0xFF_u8; 8];
    assert!(
        bincode::deserialize::<ShapeModel>(&bytes).is_err(),
        "垃圾字节应解码失败而不是 panic"
    );
}
