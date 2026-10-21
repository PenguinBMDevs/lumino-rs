//! Tiles 参数测试

use super::*;

#[test]
fn test_default_is_valid() {
    TilesParams::default()
        .validate()
        .expect("默认 Tiles 参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    let params = TilesParams {
        of: DrawingRef { id: 2 },
        keys: 8.0,
        length: 240.0,
        gap_keys: 1.0,
        gap: 30.0,
        layout: TileLayout::Brick,
        flip: TileFlip::Hv,
        by: TileBy::Checker,
        colours: 4,
        whole: true,
        at: Some([0.1, 0.2]),
    };
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: TilesParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_validate_rejects_bad_values() {
    for params in [
        TilesParams {
            keys: 0.0,
            ..TilesParams::default()
        },
        TilesParams {
            length: -1.0,
            ..TilesParams::default()
        },
        TilesParams {
            gap: -1.0,
            ..TilesParams::default()
        },
        TilesParams {
            gap_keys: -1.0,
            ..TilesParams::default()
        },
        TilesParams {
            colours: 17,
            ..TilesParams::default()
        },
        TilesParams {
            at: Some([0.5, 2.0]),
            ..TilesParams::default()
        },
    ] {
        assert!(params.validate().is_err(), "非法参数应被拒绝：{params:?}");
    }
}
