//! Sustain 参数测试

use super::*;

#[test]
fn test_default_is_valid_and_empty() {
    let params = SustainParams::default();
    params.validate().expect("默认 Sustain 参数应合法");
    assert!(params.gap_beats.is_none());
    assert!(params.most_beats.is_none());
    assert!(params.every_beats.is_none());
    assert!(params.from_beats.is_none());
    assert!(params.stops.is_none());
}

#[test]
fn test_bincode_roundtrip() {
    let params = SustainParams {
        gap_beats: Some(0.25),
        most_beats: Some(4.0),
        every_beats: Some(1.0),
        from_beats: Some(0.5),
        stops: Some(StopsParams {
            at: vec![0.0, 0.5, 1.0],
            past: true,
            off: false,
        }),
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: SustainParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_accepts_gap_zero_and_stops_bounds() {
    let params = SustainParams {
        gap_beats: Some(0.0),
        stops: Some(StopsParams {
            at: vec![0.0, 1.0],
            past: true,
            off: true,
        }),
        ..SustainParams::default()
    };
    params.validate().expect("gap=0 与比例 0/1 应合法");
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        SustainParams {
            gap_beats: Some(-0.1),
            ..SustainParams::default()
        },
        SustainParams {
            gap_beats: Some(MAX_BEATS + 0.1),
            ..SustainParams::default()
        },
        SustainParams {
            gap_beats: Some(f32::NAN),
            ..SustainParams::default()
        },
        SustainParams {
            most_beats: Some(0.0),
            ..SustainParams::default()
        },
        SustainParams {
            every_beats: Some(0.0),
            ..SustainParams::default()
        },
        SustainParams {
            from_beats: Some(-1.0),
            ..SustainParams::default()
        },
        SustainParams {
            stops: Some(StopsParams {
                at: vec![1.1],
                ..StopsParams::default()
            }),
            ..SustainParams::default()
        },
        SustainParams {
            stops: Some(StopsParams {
                at: vec![f32::NAN],
                ..StopsParams::default()
            }),
            ..SustainParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }

    let too_many = SustainParams {
        stops: Some(StopsParams {
            at: vec![0.5; MAX_STOPS_AT + 1],
            ..StopsParams::default()
        }),
        ..SustainParams::default()
    };
    assert!(too_many.validate().is_err(), "stops.at 超上限应被拒绝");
}
