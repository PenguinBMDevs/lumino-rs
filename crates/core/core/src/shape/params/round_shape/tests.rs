//! RoundShape 参数测试

use super::*;

#[test]
fn test_default_is_empty_and_valid() {
    let params = RoundShapeParams::default();
    assert!(params.is_empty());
    params.validate().expect("默认参数应合法");
    assert_eq!(params.shape_of(3), None);
}

#[test]
fn test_set_shape_clamps_and_zero_removes() {
    let mut params = RoundShapeParams::default();
    params.set_shape(2, 5.0);
    assert_eq!(params.shape_of(2), Some(MAX_SHAPE), "超上限应夹取");
    params.set_shape(2, -5.0);
    assert_eq!(params.shape_of(2), Some(MIN_SHAPE), "超下限应夹取");
    params.set_shape(2, 0.4);
    assert_eq!(params.shapes.len(), 1, "同一下标只保留一条");
    assert_eq!(params.shape_of(2), Some(0.4));
    params.set_shape(2, 0.0);
    assert_eq!(params.shape_of(2), None, "0 = 正圆弧，应移除登记");
    assert!(params.is_empty());
}

#[test]
fn test_set_shape_ignores_non_finite() {
    let mut params = RoundShapeParams::default();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        params.set_shape(1, value);
        assert_eq!(params.shape_of(1), None, "非有限值按移除处理：{value}");
    }
}

#[test]
fn test_validate_rejects_out_of_range_or_non_finite() {
    for value in [f32::NAN, f32::INFINITY, MIN_SHAPE - 0.1, MAX_SHAPE + 0.1] {
        let params = RoundShapeParams {
            shapes: vec![RoundShape {
                index: 0,
                shape: value,
            }],
        };
        assert!(params.validate().is_err(), "非法形状应被拒绝：{value}");
    }
}

#[test]
fn test_clamp_shape_is_idempotent_for_legal_values() {
    for value in [MIN_SHAPE, -0.25, 0.0, 0.5, MAX_SHAPE] {
        assert!((RoundShapeParams::clamp_shape(value) - value).abs() < 1e-6);
    }
}
