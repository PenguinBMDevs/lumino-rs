//! 由 `tests.rs` 拆出的后半段测试，按原顺序保留。

use super::*;

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

/// UI-023：`TextV2` 追加在 `ShapeOp` 末尾，旧变体的 bincode 判别序号不得漂移。
///
/// bincode 1.3 的 `serialize` 用 fixint 小端把枚举判别写成 `u32`：这里直接断言旧变体
/// 的首 4 字节，任何「插到中间」的重排都会让旧工程解出错误语义。
#[test]
fn test_shape_op_text_v2_tail_append_is_backward_compatible() {
    fn tag(op: &ShapeOp) -> u32 {
        let bytes = bincode::serialize(op).expect("ShapeOp 应可序列化");
        u32::from_le_bytes(bytes[..4].try_into().expect("枚举判别应占 4 字节"))
    }

    // 追加前的判别序号：Noop = 0、Text = 24、RoundShape = 27（旧的末尾变体）。
    assert_eq!(tag(&ShapeOp::Noop), 0, "Noop 判别序号不得漂移");
    let text = ShapeOp::Text(TextOp {
        text: "LUMINO".to_string(),
        params: TextParams::default(),
        layout: TextLayoutMode::LegacyBox,
        snap: 480.0,
        merged: false,
        vertical: false,
    });
    assert_eq!(tag(&text), 24, "Text 判别序号不得漂移");
    let round_shape = ShapeOp::RoundShape(RoundShapeParams::default());
    assert_eq!(tag(&round_shape), 27, "RoundShape 判别序号不得漂移");

    // 新变体必须落在末尾（28）；旧变体的字节仍应原样解码。
    let text_v2 = ShapeOp::TextV2(TextOpV2 {
        text: "LUMINO".to_string(),
        params: TextParams::default(),
        layout: TextLayoutMode::Parametric,
        snap: 480.0,
        merged: false,
        vertical: false,
        k: 0.4167,
    });
    assert_eq!(tag(&text_v2), 28, "TextV2 应追加在末尾（第 28 个变体）");

    let bytes = bincode::serialize(&round_shape).expect("RoundShape 应可序列化");
    let back: ShapeOp = bincode::deserialize(&bytes).expect("旧变体字节应可解码");
    assert_eq!(back, round_shape, "尾插后旧变体字节应原样解码");
}
