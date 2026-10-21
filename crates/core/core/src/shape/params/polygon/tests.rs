//! Polygon 参数测试

use super::*;

#[test]
fn test_defaults_are_valid() {
    let params = PolygonParams::default();
    params.validate().expect("默认 Polygon 参数应合法");
    assert_eq!(params.points, 5);
    assert_eq!(params.style, PolyStyle::Polygon);
}

#[test]
fn test_bincode_roundtrip() {
    let params = PolygonParams {
        points: 7,
        style: PolyStyle::Star,
        inner: 42.5,
        skip: 3,
        shape: Some(PatternRef {
            formula: "sin(x*2*pi)".into(),
            vars: BTreeMap::from([("height".to_string(), 2.0)]),
            loops: 3.0,
            k: 0.25,
            mirror: true,
            scale: 1.5,
        }),
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: PolygonParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        PolygonParams {
            points: 2,
            ..PolygonParams::default()
        },
        PolygonParams {
            points: 201,
            ..PolygonParams::default()
        },
        PolygonParams {
            inner: f32::NAN,
            ..PolygonParams::default()
        },
        PolygonParams {
            skip: 0,
            ..PolygonParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
