//! Strum 参数测试

use super::*;

#[test]
fn test_default_is_valid() {
    let params = StrumParams::default();
    params.validate().expect("默认 Strum 参数应通过校验");
    assert_eq!(params.time, 0.0);
}

#[test]
fn test_bincode_roundtrip() {
    let params = StrumParams {
        time: 1.0 / 3.0,
        tension: 100.0,
        preserve: true,
        ahead: true,
        end: true,
        end_time: -0.5,
        end_tension: -50.0,
        vel: -12,
        vel_tension: 25.0,
        chop: true,
        alternate: true,
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: StrumParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        StrumParams {
            time: 4.1,
            ..StrumParams::default()
        },
        StrumParams {
            end_time: -4.1,
            ..StrumParams::default()
        },
        StrumParams {
            tension: 101.0,
            ..StrumParams::default()
        },
        StrumParams {
            end_tension: -101.0,
            ..StrumParams::default()
        },
        StrumParams {
            vel_tension: f32::NAN,
            ..StrumParams::default()
        },
        StrumParams {
            vel: 128,
            ..StrumParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
