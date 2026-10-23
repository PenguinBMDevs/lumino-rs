//! Between 参数测试

use super::*;

fn first() -> BetweenParams {
    BetweenParams {
        id: 7,
        role: BetweenRole::First,
        at: Some(0.0),
        sig: None,
        push: None,
        vel: None,
        set: Some(BetweenSet::default()),
    }
}

#[test]
fn test_valid_first_and_step() {
    first().validate().expect("首形状应合法");
    BetweenParams {
        role: BetweenRole::Step,
        at: Some(0.5),
        set: None,
        ..first()
    }
    .validate()
    .expect("Step 应合法");
}

#[test]
fn test_rejects_missing_set_and_bad_at() {
    let mut params = first();
    params.set = None;
    assert!(params.validate().is_err());
    let mut params = first();
    params.role = BetweenRole::Step;
    params.at = Some(1.5);
    assert!(params.validate().is_err());
}

#[test]
fn test_set_limits_and_roundtrip() {
    let set = BetweenSet {
        steps: MOST_STEPS + 1,
        ..BetweenSet::default()
    };
    assert!(set.validate().is_err());
    let params = first();
    let bytes = bincode::serialize(&params).expect("序列化失败");
    let back: BetweenParams = bincode::deserialize(&bytes).expect("反序列化失败");
    assert_eq!(back, params);
}
