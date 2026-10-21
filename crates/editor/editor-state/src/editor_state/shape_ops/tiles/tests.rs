//! Tiles 算法测试

use lumino_core::shape::{TileBy, TileFlip, TileLayout, TilesParams};

use super::*;

fn area() -> Vec<RowSpan> {
    (0..=4)
        .map(|key| RowSpan {
            key,
            start: 0.0,
            end: 400.0,
        })
        .collect()
}

fn tile() -> Vec<RowSpan> {
    (0..=1)
        .map(|key| RowSpan {
            key,
            start: 0.0,
            end: 50.0,
        })
        .collect()
}

fn starts(spans: &[ColouredSpan]) -> Vec<f32> {
    let mut values = spans.iter().map(|span| span.start).collect::<Vec<_>>();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup();
    values
}

#[test]
fn test_grid_spacing() {
    let params = TilesParams {
        length: 100.0,
        keys: 2.0,
        gap: 10.0,
        gap_keys: 1.0,
        ..TilesParams::default()
    };
    let spans = tiled_spans(&area(), &tile(), &params);
    let xs = starts(&spans);
    assert!(xs.contains(&0.0));
    assert!(xs.contains(&110.0));
    assert!(xs.contains(&220.0));
    assert!(xs.contains(&330.0));
    let keys = spans.iter().map(|s| s.key).collect::<Vec<_>>();
    assert!(
        keys.contains(&0) && keys.contains(&3),
        "行间距应为 keys+gap_keys"
    );
}

#[test]
fn test_brick_offsets_odd_rows() {
    let params = TilesParams {
        length: 100.0,
        keys: 2.0,
        layout: TileLayout::Brick,
        ..TilesParams::default()
    };
    let spans = tiled_spans(&area(), &tile(), &params);
    let xs = starts(&spans);
    assert!(xs.contains(&50.0), "奇数行应错开半块，实际 {xs:?}");
    assert!(xs.contains(&100.0));
}

#[test]
fn test_flip_hv_mirrors_by_row_and_col() {
    let params = TilesParams {
        length: 100.0,
        keys: 2.0,
        flip: TileFlip::Hv,
        ..TilesParams::default()
    };
    let spans = tiled_spans(&area(), &tile(), &params);
    assert!(
        spans
            .iter()
            .any(|s| s.key == 0 && (s.start - 150.0).abs() < 1e-4),
        "第 1 列应左右镜像到 150..200"
    );
    assert!(
        spans
            .iter()
            .any(|s| s.key == 2 && (s.start - 0.0).abs() < 1e-4),
        "第 1 行应上下镜像到 key=2"
    );
    assert!(
        spans
            .iter()
            .any(|s| s.key == 2 && (s.start - 150.0).abs() < 1e-4),
        "行+列同时镜像"
    );
}

#[test]
fn test_whole_drops_cut_tiles() {
    let cut = vec![RowSpan {
        key: 0,
        start: 0.0,
        end: 30.0,
    }];
    let params = TilesParams {
        length: 100.0,
        keys: 2.0,
        whole: true,
        ..TilesParams::default()
    };
    assert!(
        tiled_spans(&cut, &tile(), &params).is_empty(),
        "被切块应整块丢弃"
    );
    let params = TilesParams {
        whole: false,
        ..params
    };
    assert!(
        !tiled_spans(&cut, &tile(), &params).is_empty(),
        "非 whole 应保留裁剪片段"
    );
}

#[test]
fn test_by_checker_sets_channels() {
    let params = TilesParams {
        length: 100.0,
        keys: 2.0,
        by: TileBy::Checker,
        colours: 4,
        ..TilesParams::default()
    };
    let spans = tiled_spans(&area(), &tile(), &params);
    assert!(spans.iter().any(|s| s.colour == 0));
    assert!(spans.iter().any(|s| s.colour != 0));
}
