//! Nested 参数测试

use super::*;

#[test]
fn test_default_is_valid() {
    NestedParams::default()
        .validate()
        .expect("默认 Nested 参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    let params = NestedParams {
        way: NestedWay::Blend,
        steps: 7,
        last: 0.25,
        spacing: -35.0,
        turn: 12.5,
        colours: 3,
        skip: true,
        at: Some([0.25, 0.75]),
        mix: 0.8,
        of: Some(DrawingRef { id: 3 }),
        angle: 45.0,
        morph: true,
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: NestedParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        NestedParams {
            steps: 0,
            ..NestedParams::default()
        },
        NestedParams {
            steps: 257,
            ..NestedParams::default()
        },
        NestedParams {
            last: 1.1,
            ..NestedParams::default()
        },
        NestedParams {
            colours: 17,
            ..NestedParams::default()
        },
        NestedParams {
            at: Some([1.2, 0.5]),
            ..NestedParams::default()
        },
        NestedParams {
            mix: f32::NAN,
            ..NestedParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
