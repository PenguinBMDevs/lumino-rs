//! Chop 参数测试

use super::*;

#[test]
fn test_builtins_have_expected_shape() {
    for name in ChopParams::BUILTIN_NAMES {
        let params = ChopParams::builtin(name).expect("内置节奏应存在");
        assert_eq!(params.name, name);
        params
            .validate()
            .unwrap_or_else(|e| panic!("{name} 应合法: {e}"));
        assert!(!params.pieces.is_empty(), "{name} 应有片");
    }
    assert!(ChopParams::builtin("unknown").is_none());
}

#[test]
fn test_default_is_even() {
    let params = ChopParams::default();
    assert_eq!(params.name, "even");
    assert_eq!(params.steps, 1);
    params.validate().expect("默认参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    let params = ChopParams {
        name: "custom".into(),
        steps: 6,
        pieces: vec![
            ChopPiece {
                start: 0.0,
                len: 0.5,
                vel: 100,
            },
            ChopPiece {
                start: 0.5,
                len: 1.5,
                vel: 64,
            },
        ],
        len: 0.5,
        abs: true,
        vel: 75,
        fixed: true,
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: ChopParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        ChopParams {
            steps: 0,
            ..ChopParams::default()
        },
        ChopParams {
            steps: 65,
            ..ChopParams::default()
        },
        ChopParams {
            len: -0.1,
            ..ChopParams::default()
        },
        ChopParams {
            vel: 101,
            ..ChopParams::default()
        },
        ChopParams {
            pieces: vec![ChopPiece {
                start: -1.0,
                len: 1.0,
                vel: 100,
            }],
            ..ChopParams::default()
        },
        ChopParams {
            pieces: vec![ChopPiece {
                start: 0.0,
                len: 1.0,
                vel: 128,
            }],
            ..ChopParams::default()
        },
        ChopParams {
            pieces: (0..257)
                .map(|_| ChopPiece {
                    start: 0.0,
                    len: 0.0,
                    vel: 100,
                })
                .collect(),
            ..ChopParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
