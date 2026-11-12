//! RoundCorner 参数测试

use super::*;

#[test]
fn test_validate_rejects_negative_or_non_finite_radius() {
    let mut params = RoundParams::default();
    assert!(params.validate().is_ok());
    params.set_radius(1, 2.0);
    assert!(params.validate().is_ok());
    params.corners[0].radius = -1.0;
    assert!(params.validate().is_err());
    params.corners[0].radius = f32::NAN;
    assert!(params.validate().is_err());
}

#[test]
fn test_set_radius_overwrites_same_index_and_zero_removes() {
    let mut params = RoundParams::default();
    params.set_radius(2, 1.0);
    params.set_radius(2, 3.0);
    assert_eq!(params.radius_of(2), Some(3.0), "同一下标以最后一条为准");
    assert_eq!(params.corners.len(), 1);
    params.set_radius(2, 0.0);
    assert_eq!(params.radius_of(2), None);
    assert!(params.is_empty());
}

#[test]
fn test_radius_of_missing_index_is_none() {
    let params = RoundParams::default();
    assert_eq!(params.radius_of(7), None);
}
