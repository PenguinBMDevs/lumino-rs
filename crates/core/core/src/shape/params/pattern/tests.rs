//! Pattern 参数测试

use super::*;

#[test]
fn test_default_is_valid() {
    PatternParams::default()
        .validate()
        .expect("默认 Pattern 参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    let params = PatternParams {
        preset: "wave".into(),
        formula: "height * sin(x * 2 * pi)".into(),
        vars: BTreeMap::from([("height".to_string(), 3.5)]),
        along: Some("sin(x * 2 * pi) / 4".into()),
        loops: 2.5,
        each: true,
        k: 0.25,
        mirror: true,
        scale: -1.5,
        sym: Some(Sym::Turn),
        loop_curve: Some(LoopCurve {
            pts: vec![[0.0, 0.0], [0.5, 1.0], [1.0, 0.0]],
            sharp: vec![1],
        }),
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: PatternParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        PatternParams {
            loops: 0.0,
            ..PatternParams::default()
        },
        PatternParams {
            k: 0.0,
            ..PatternParams::default()
        },
        PatternParams {
            scale: 0.0,
            ..PatternParams::default()
        },
        PatternParams {
            formula: "sin(".into(),
            ..PatternParams::default()
        },
        PatternParams {
            along: Some("foo(".into()),
            ..PatternParams::default()
        },
        PatternParams {
            formula: String::new(),
            loop_curve: Some(LoopCurve {
                pts: vec![[0.0, 0.0]],
                sharp: Vec::new(),
            }),
            ..PatternParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
