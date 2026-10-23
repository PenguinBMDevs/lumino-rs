//! Merge 参数测试

use super::*;

#[test]
fn test_roundtrip() {
    let params = MergeParams {
        stay: 7,
        from_left: Some(true),
        rest_slides: true,
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: MergeParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}
