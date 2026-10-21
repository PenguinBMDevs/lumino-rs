//! CopyAlong 参数测试

use super::*;

#[test]
fn test_default_valid_and_roundtrip() {
    let params = CopyPathParams::default();
    params.validate().expect("默认参数应合法");
    let bytes = bincode::serialize(&params).expect("序列化失败");
    let back: CopyPathParams = bincode::deserialize(&bytes).expect("反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_rejects_bad_limits() {
    for params in [
        CopyPathParams {
            k: 0.0,
            ..CopyPathParams::default()
        },
        CopyPathParams {
            mode: CopyMode::Count { n: 0 },
            ..CopyPathParams::default()
        },
        CopyPathParams {
            size: [100.0, f32::NAN],
            ..CopyPathParams::default()
        },
        CopyPathParams {
            colours: ColourMode::Shift { step: 16 },
            ..CopyPathParams::default()
        },
        CopyPathParams {
            colours: ColourMode::One { colour: 17 },
            ..CopyPathParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
