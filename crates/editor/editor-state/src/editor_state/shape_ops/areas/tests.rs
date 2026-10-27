//! 形状区域切分测试

use super::*;
use lumino_core::shape::{AREA_UNASSIGNED, AreaSpot, ShapeRect};

fn box_100() -> ShapeRect {
    ShapeRect::normalized(0.0, 0.0, 100.0, 100.0)
}

fn square(lo: f64, hi: f64) -> Vec<(f64, f64)> {
    vec![(lo, lo), (hi, lo), (hi, hi), (lo, hi)]
}

fn row(key: u16, start: f32, end: f32) -> RowSpan {
    RowSpan { key, start, end }
}

#[test]
fn test_build_and_paint_single_region() {
    let ctx = AreaContext::build(
        &[square(0.0, 100.0)],
        box_100(),
        &[AreaSpot {
            u: 0.5,
            v: 0.5,
            colour: 3,
        }],
        false,
        false,
    )
    .expect("内部着色点应建立上下文");
    let pieces = row_pieces_coloured(&[row(50, 0.0, 100.0)], &ctx.map, ctx.box_);
    assert_eq!(pieces.len(), 1, "整行同色应合并为一段");
    assert_eq!(pieces[0].1, 3);
    assert!((pieces[0].0.start - 0.0).abs() < 1e-3);
    assert!((pieces[0].0.end - 100.0).abs() < 1e-3);
}

#[test]
fn test_empty_colour_suppresses_region() {
    let ctx = AreaContext::build(
        &[square(0.0, 100.0)],
        box_100(),
        &[AreaSpot {
            u: 0.5,
            v: 0.5,
            colour: 0,
        }],
        false,
        false,
    )
    .expect("空区域也应建立上下文");
    let pieces = row_pieces_coloured(&[row(50, 0.0, 100.0)], &ctx.map, ctx.box_);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].1, 0);
}

#[test]
fn test_region_boundary_splits_row() {
    let mut paths = vec![square(0.0, 100.0)];
    paths.push(vec![(50.0, 0.0), (50.0, 100.0)]);
    let ctx = AreaContext::build(
        &paths,
        box_100(),
        &[AreaSpot {
            u: 0.25,
            v: 0.5,
            colour: 4,
        }],
        false,
        false,
    )
    .expect("左半区应建立上下文");
    let pieces = row_pieces_coloured(&[row(50, 0.0, 100.0)], &ctx.map, ctx.box_);
    assert_eq!(pieces.len(), 2, "边界应把一行切成两段");
    assert_eq!(pieces[0].1, 4);
    assert!((pieces[0].0.end - 50.0).abs() < 0.2);
    assert_eq!(pieces[1].1, AREA_UNASSIGNED);
    assert!((pieces[1].0.start - 50.0).abs() < 0.2);
}

#[test]
fn test_spot_outside_shape_produces_no_context() {
    let ctx = AreaContext::build(
        &[square(0.0, 100.0)],
        box_100(),
        &[AreaSpot {
            u: 2.0,
            v: 2.0,
            colour: 3,
        }],
        false,
        false,
    );
    assert!(ctx.is_none(), "越界点不应悄悄着色");
}

#[test]
fn test_channel_10_requires_use10() {
    let spots = [AreaSpot {
        u: 0.5,
        v: 0.5,
        colour: 16,
    }];
    assert!(
        AreaContext::build(&[square(0.0, 100.0)], box_100(), &spots, false, false).is_none(),
        "未启用通道 10 时 colour=16 应被跳过"
    );
    let ctx = AreaContext::build(&[square(0.0, 100.0)], box_100(), &spots, false, true)
        .expect("启用通道 10 后应可着色");
    let pieces = row_pieces_coloured(&[row(50, 0.0, 100.0)], &ctx.map, ctx.box_);
    assert_eq!(pieces[0].1, 16);
}

#[test]
fn test_translation_keeps_relative_region() {
    let spot = AreaSpot {
        u: 0.25,
        v: 0.25,
        colour: 6,
    };
    let first = AreaContext::build(
        &[square(0.0, 100.0)],
        ShapeRect::normalized(0.0, 0.0, 100.0, 100.0),
        &[spot],
        false,
        false,
    )
    .expect("原形状应建立上下文");
    let shifted_path = vec![(500.0, 20.0), (600.0, 20.0), (600.0, 120.0), (500.0, 120.0)];
    let shifted = AreaContext::build(
        &[shifted_path],
        ShapeRect::normalized(500.0, 20.0, 600.0, 120.0),
        &[spot],
        false,
        false,
    )
    .expect("平移形状应建立上下文");
    let a = row_pieces_coloured(&[row(20, 0.0, 100.0)], &first.map, first.box_);
    let b = row_pieces_coloured(&[row(70, 500.0, 600.0)], &shifted.map, shifted.box_);
    assert_eq!(a.len(), b.len());
    assert_eq!(a[0].1, b[0].1);
    assert!((b[0].0.start - a[0].0.start - 500.0).abs() < 0.3);
}
