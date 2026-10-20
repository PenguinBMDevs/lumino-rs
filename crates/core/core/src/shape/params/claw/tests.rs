//! Claw 参数测试

use super::*;

#[test]
fn test_default_is_valid() {
    let params = ClawParams::default();
    params.validate().expect("默认 Claw 参数应通过校验");
    assert_eq!(params.mode, ClawMode::Time);
    assert_eq!(params.period, ClawPeriod::Four);
}

#[test]
fn test_period_beats() {
    assert_eq!(ClawPeriod::One.beats(), 1.0);
    assert_eq!(ClawPeriod::Two.beats(), 2.0);
    assert_eq!(ClawPeriod::Four.beats(), 4.0);
    assert_eq!(ClawPeriod::Eight.beats(), 8.0);
    assert_eq!(ClawPeriod::Sixteen.beats(), 16.0);
}

#[test]
fn test_bincode_roundtrip() {
    let params = ClawParams {
        mode: ClawMode::Random,
        period: ClawPeriod::Eight,
        trash: Some(ClawTrash { k: 2, n: 5 }),
        dist: -37,
        stretch: true,
        short: true,
        keep: 7,
        skip: 3,
        pct: 42,
        seed: 12345,
        shorten: true,
        cut: 66,
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: ClawParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        ClawParams {
            trash: Some(ClawTrash { k: 0, n: 0 }),
            ..ClawParams::default()
        },
        ClawParams {
            trash: Some(ClawTrash { k: 4, n: 4 }),
            ..ClawParams::default()
        },
        ClawParams {
            dist: 101,
            ..ClawParams::default()
        },
        ClawParams {
            keep: 0,
            ..ClawParams::default()
        },
        ClawParams {
            skip: 1001,
            ..ClawParams::default()
        },
        ClawParams {
            pct: 101,
            ..ClawParams::default()
        },
        ClawParams {
            seed: 1_000_000_001,
            ..ClawParams::default()
        },
        ClawParams {
            cut: 0,
            ..ClawParams::default()
        },
        ClawParams {
            cut: 100,
            ..ClawParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
