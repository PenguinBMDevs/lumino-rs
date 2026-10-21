//! Symmetry 参数测试

use super::*;

#[test]
fn test_valid_axes() {
    for axis in [
        SymAxis::Vertical { x: 100.0 },
        SymAxis::Horizontal { y: 60.0 },
        SymAxis::Custom {
            origin: (0.0, 60.0),
            dir: (1.0, 1.0),
        },
    ] {
        let params = SymParams {
            mode: Sym::Mirror,
            axis,
            source: 0,
        };
        params.validate().expect("合法轴应通过");
    }
}

#[test]
fn test_roundtrip() {
    let params = SymParams {
        mode: Sym::Turn,
        axis: SymAxis::Custom {
            origin: (10.0, 60.0),
            dir: (0.0, 1.0),
        },
        source: 3,
    };
    let bytes = bincode::serialize(&params).expect("序列化失败");
    let back: SymParams = bincode::deserialize(&bytes).expect("反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_invalid_axes_rejected() {
    for axis in [
        SymAxis::Vertical { x: f32::NAN },
        SymAxis::Horizontal { y: f32::INFINITY },
        SymAxis::Custom {
            origin: (0.0, 60.0),
            dir: (0.0, 0.0),
        },
        SymAxis::Custom {
            origin: (f32::NAN, 60.0),
            dir: (1.0, 0.0),
        },
    ] {
        assert!(axis.validate().is_err(), "非法轴应被拒绝：{axis:?}");
    }
}
