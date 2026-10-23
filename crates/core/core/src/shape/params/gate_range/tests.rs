//! Gate Range 参数测试

use super::*;

#[test]
fn test_defaults_are_valid() {
    let params = GateRangeParams::default();
    params.validate().expect("默认 GateRange 参数应合法");
    assert_eq!(params.dir, RangeDir::Time);
    assert_eq!(params.graph.len(), 2);
}

#[test]
fn test_bincode_roundtrip() {
    let params = GateRangeParams {
        to: 1.5,
        graph: vec![[0.0, 0.0], [0.5, 0.25], [1.0, 1.0]],
        dir: RangeDir::KeysDown,
        fit: true,
        rows: true,
        join: true,
        halves: true,
        follow_curve: true,
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: GateRangeParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        GateRangeParams {
            to: 0.0,
            ..GateRangeParams::default()
        },
        GateRangeParams {
            graph: vec![[0.0, 0.0]],
            ..GateRangeParams::default()
        },
        GateRangeParams {
            graph: vec![[0.1, 0.0], [1.0, 1.0]],
            ..GateRangeParams::default()
        },
        GateRangeParams {
            graph: vec![[0.0, 0.0], [1.2, 1.0]],
            ..GateRangeParams::default()
        },
        GateRangeParams {
            graph: vec![[0.0, 0.0], [0.5, f32::NAN], [1.0, 1.0]],
            ..GateRangeParams::default()
        },
        GateRangeParams {
            graph: vec![[0.0, 0.0], [0.8, 0.5], [0.5, 1.0], [1.0, 1.0]],
            ..GateRangeParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
