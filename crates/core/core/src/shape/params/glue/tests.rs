//! Glue 参数测试

use super::*;

#[test]
fn test_default_is_all() {
    assert_eq!(GlueParams::default(), GlueParams::All);
    GlueParams::default().validate().expect("默认参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    for params in [
        GlueParams::All,
        GlueParams::Boxes(vec![[0.0, 0.0, 0.5, 1.0], [0.25, 0.5, 1.0, 1.0]]),
        GlueParams::Boxes(Vec::new()),
    ] {
        let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
        let back: GlueParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
        assert_eq!(back, params);
    }
}

#[test]
fn test_validate_accepts_boundaries_and_empty() {
    for params in [
        GlueParams::Boxes(Vec::new()),
        GlueParams::Boxes(vec![[0.0, 0.0, 1.0, 1.0]]),
        GlueParams::Boxes(vec![[1.0, 1.0, 0.0, 0.0]]),
    ] {
        params.validate().expect("边界/空框应通过校验");
    }
}

#[test]
fn test_validate_rejects_out_of_range_and_non_finite() {
    for corners in [
        [0.0, 0.0, 1.1, 1.0],
        [-0.1, 0.0, 0.5, 1.0],
        [0.0, f32::NAN, 0.5, 1.0],
        [0.0, 0.0, f32::INFINITY, 1.0],
    ] {
        let params = GlueParams::Boxes(vec![corners]);
        assert!(params.validate().is_err(), "非法框应被拒绝：{corners:?}");
    }
}
