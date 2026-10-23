//! Tumours 参数测试

use super::*;

#[test]
fn test_default_is_valid() {
    let params = TumourParams::default();
    params.validate().expect("默认 Tumours 参数应通过校验");
    assert!(params.on, "默认应启用");
    assert_eq!(params.shape, TumourShape::Triangle);
    assert_eq!(params.side, TumourSide::Alt);
}

#[test]
fn test_bincode_roundtrip_keeps_all_fields() {
    let params = TumourParams {
        shape: TumourShape::Square,
        size: -6.0,
        length: 0.0,
        dist: 2.5,
        side: TumourSide::Random,
        start: 0.2,
        end: 0.8,
        ease: 0.5,
        rot: 30.0,
        slant: -0.25,
        fit: true,
        seed: 42,
        mirror: true,
        k: 0.5,
        graphs: TumourGraphs {
            size: vec![(0.0, 1.0), (1.0, 0.5)],
            ..TumourGraphs::default()
        },
        ..TumourParams::default()
    };

    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: TumourParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_factor_at_interpolates_and_clamps() {
    let points = [(0.0, 1.0), (0.5, 2.0), (1.0, 0.0)];
    assert_eq!(factor_at(&points, -1.0), 1.0);
    assert_eq!(factor_at(&points, 0.0), 1.0);
    assert!((factor_at(&points, 0.25) - 1.5).abs() < f32::EPSILON);
    assert!((factor_at(&points, 0.75) - 1.0).abs() < f32::EPSILON);
    assert_eq!(factor_at(&points, 2.0), 0.0);
    assert_eq!(factor_at(&[], 0.5), 1.0, "空曲线应恒为 1.0");
}

#[test]
fn test_validate_rejects_bad_scalars() {
    for bad in [
        TumourParams {
            dist: 0.0,
            ..TumourParams::default()
        },
        TumourParams {
            length: -1.0,
            ..TumourParams::default()
        },
        TumourParams {
            ease: -0.1,
            ..TumourParams::default()
        },
        TumourParams {
            start: 1.5,
            ..TumourParams::default()
        },
        TumourParams {
            rot: 181.0,
            ..TumourParams::default()
        },
        TumourParams {
            slant: 1.1,
            ..TumourParams::default()
        },
        TumourParams {
            k: 0.0,
            ..TumourParams::default()
        },
        TumourParams {
            size: f32::NAN,
            ..TumourParams::default()
        },
    ] {
        assert!(bad.validate().is_err(), "非法参数应被拒绝：{bad:?}");
    }
}

#[test]
fn test_validate_rejects_bad_graphs() {
    let unsorted = TumourParams {
        graphs: TumourGraphs {
            size: vec![(0.0, 1.0), (0.8, 1.0), (0.5, 1.0)],
            ..TumourGraphs::default()
        },
        ..TumourParams::default()
    };
    let err = unsorted.validate().expect_err("乱序 graph 应被拒绝");
    assert!(format!("{err}").contains("非降序"), "实际错误：{err}");

    let out_of_range = TumourParams {
        graphs: TumourGraphs {
            size: vec![(1.2, 1.0)],
            ..TumourGraphs::default()
        },
        ..TumourParams::default()
    };
    assert!(out_of_range.validate().is_err(), "u 越界应被拒绝");
}
