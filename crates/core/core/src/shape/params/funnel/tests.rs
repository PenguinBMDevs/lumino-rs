//! Funnel 参数测试

use super::*;

fn params() -> FunnelParams {
    FunnelParams {
        lines: vec![[[0.0, 20.0], [100.0, 20.0]]],
        wall: [[100.0, 10.0], [100.0, 30.0]],
        starts: Vec::new(),
        fill: ShapeFill::Spam,
        reverse: false,
    }
}

#[test]
fn test_default_shape_is_valid() {
    params().validate().expect("最简 Funnel 参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    let params = FunnelParams {
        starts: vec![FunnelStart {
            line: 0,
            at: 0.25,
            ends: [Some(vec![[90.0, 12.0]]), None],
        }],
        reverse: true,
        ..params()
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: FunnelParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_unsupported_values() {
    for params in [
        FunnelParams {
            lines: Vec::new(),
            ..params()
        },
        FunnelParams {
            lines: vec![[[0.0, 20.0], [100.0, 20.0]], [[0.0, 30.0], [100.0, 30.0]]],
            ..params()
        },
        FunnelParams {
            fill: ShapeFill::Fill,
            ..params()
        },
        FunnelParams {
            starts: vec![FunnelStart {
                line: 1,
                at: 0.5,
                ends: [None, None],
            }],
            ..params()
        },
        FunnelParams {
            wall: [[100.0, f32::NAN], [100.0, 30.0]],
            ..params()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
