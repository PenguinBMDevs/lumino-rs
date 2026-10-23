//! Colours 参数测试

use super::*;

#[test]
fn test_defaults_are_valid() {
    let params = CycleParams::default();
    params.validate().expect("默认 Colours 参数应合法");
    assert_eq!(params.by, CycleBy::Step);
    assert_eq!(params.n, 2);
    assert_eq!(params.every, 1);
    assert_eq!(params.every_time, Some([1, 4]));
}

#[test]
fn test_bincode_roundtrip() {
    let params = CycleParams {
        by: CycleBy::Time,
        n: 7,
        every: 3,
        every_time: Some([3, 8]),
        rows: Some(RowRestart::End),
        shift: Some([2, 5]),
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: CycleParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        CycleParams {
            n: 1,
            ..CycleParams::default()
        },
        CycleParams {
            n: 17,
            ..CycleParams::default()
        },
        CycleParams {
            every: 0,
            ..CycleParams::default()
        },
        CycleParams {
            every_time: Some([0, 4]),
            ..CycleParams::default()
        },
        CycleParams {
            every_time: Some([10_001, 4]),
            ..CycleParams::default()
        },
        CycleParams {
            shift: Some([0, 0]),
            ..CycleParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}

#[test]
fn test_validate_accepts_boundaries() {
    let params = CycleParams {
        n: 16,
        every: 10_000,
        shift: Some([-10_000, 10_000]),
        ..CycleParams::default()
    };
    params.validate().expect("边界值应合法");
}
