//! Straighten 参数测试

use super::*;

#[test]
fn test_accepts_0_to_100() {
    for level in [0, 1, 50, 100] {
        StraightenParams { level }
            .validate()
            .expect("0..=100 应合法");
    }
}

#[test]
fn test_rejects_over_100() {
    assert!(StraightenParams { level: 101 }.validate().is_err());
}

#[test]
fn test_roundtrip() {
    let params = StraightenParams { level: 73 };
    let bytes = bincode::serialize(&params).expect("序列化失败");
    let back: StraightenParams = bincode::deserialize(&bytes).expect("反序列化失败");
    assert_eq!(back, params);
}
