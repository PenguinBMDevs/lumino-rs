//! Mask 参数测试

use super::*;

#[test]
fn test_defaults_match_upstream() {
    let params = MaskParams::default();
    assert_eq!(params.size_keys, 2.0);
    assert_eq!(params.k, 0.0625);
    assert!(!params.invert);
    assert!(params.own);
    assert_eq!(params.show, MaskShow::Inside);
    assert!(params.group.is_none());
    assert_eq!(params.mode, MaskMode::Add);
    assert_eq!(params.colour, 0);
    params.validate().expect("默认 Mask 参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    let params = MaskParams {
        size_keys: 7.5,
        k: 0.125,
        invert: true,
        own: false,
        show: MaskShow::Line,
        group: Some(42),
        mode: MaskMode::Subtract,
        colour: 5,
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: MaskParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        MaskParams {
            size_keys: -0.1,
            ..MaskParams::default()
        },
        MaskParams {
            size_keys: 127.1,
            ..MaskParams::default()
        },
        MaskParams {
            size_keys: f32::NAN,
            ..MaskParams::default()
        },
        MaskParams {
            k: 0.0,
            ..MaskParams::default()
        },
        MaskParams {
            k: f32::INFINITY,
            ..MaskParams::default()
        },
        MaskParams {
            group: Some(0),
            ..MaskParams::default()
        },
        MaskParams {
            colour: MaskParams::GROUP_COLOURS,
            ..MaskParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}

#[test]
fn test_validate_accepts_boundaries() {
    let params = MaskParams {
        size_keys: 127.0,
        k: 10_000.0,
        group: Some(999_999_999),
        colour: MaskParams::GROUP_COLOURS - 1,
        ..MaskParams::default()
    };
    params.validate().expect("边界值应合法");
}
