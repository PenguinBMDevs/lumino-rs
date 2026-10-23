//! Join 参数测试

use super::*;

#[test]
fn test_valid_and_roundtrip() {
    let params = JoinedParams {
        gaps: vec![1],
        tumours: vec![None, Some(TumourParams::default())],
        splits: vec![2],
        src: vec![1, 2],
    };
    params.validate().expect("参数应合法");
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: JoinedParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        JoinedParams {
            src: Vec::new(),
            tumours: Vec::new(),
            gaps: Vec::new(),
            splits: Vec::new(),
        },
        JoinedParams {
            gaps: vec![2, 1],
            tumours: vec![None],
            splits: Vec::new(),
            src: vec![1],
        },
        JoinedParams {
            gaps: Vec::new(),
            tumours: Vec::new(),
            splits: Vec::new(),
            src: vec![1],
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
