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

/// 两片相邻且颜色不同：`borders` 应把交界格标成 `AREA_BORDER`
fn two_colour_regions() -> AreaContext {
    let mut paths = vec![square(0.0, 100.0)];
    paths.push(vec![(50.0, 0.0), (50.0, 100.0)]);
    AreaContext::build(
        &paths,
        box_100(),
        &[
            AreaSpot {
                u: 0.25,
                v: 0.5,
                colour: 4,
            },
            AreaSpot {
                u: 0.75,
                v: 0.5,
                colour: 5,
            },
        ],
        false,
        false,
    )
    .expect("两片都应着色")
}

#[test]
fn test_borders_marks_colour_boundary() {
    let ctx = two_colour_regions();
    let plain = row_pieces_coloured(&[row(50, 0.0, 100.0)], &ctx.map, ctx.box_);
    assert!(
        plain.iter().all(|(_, colour)| *colour != AREA_BORDER),
        "borders=false 不得标交界"
    );

    let bordered =
        row_pieces_coloured_with_borders(&[row(50, 0.0, 100.0)], &ctx.map, ctx.box_, true);
    assert!(
        bordered.iter().any(|(_, colour)| *colour == AREA_BORDER),
        "颜色交界处应出现 AREA_BORDER：{bordered:?}"
    );
    // 交界带必须落在中线附近（网格误差 <= 1 格 ≈ 0.1 tick）
    let border_span: f32 = bordered
        .iter()
        .filter(|(_, colour)| *colour == AREA_BORDER)
        .map(|(span, _)| span.end - span.start)
        .sum();
    assert!(border_span > 0.0, "交界带不能为空");
    let first_border = bordered
        .iter()
        .find(|(_, colour)| *colour == AREA_BORDER)
        .map(|(span, _)| span.start)
        .expect("应有交界段");
    assert!(
        (first_border - 50.0).abs() <= 0.2,
        "交界应从 u=0.5 附近开始，实际 {first_border}"
    );
}

#[test]
fn test_borders_keeps_empty_region_silent() {
    // 一半着色、一半空：空区域不参与交界判定，不能被描边复活
    let mut paths = vec![square(0.0, 100.0)];
    paths.push(vec![(50.0, 0.0), (50.0, 100.0)]);
    let ctx = AreaContext::build(
        &paths,
        box_100(),
        &[
            AreaSpot {
                u: 0.25,
                v: 0.5,
                colour: 4,
            },
            AreaSpot {
                u: 0.75,
                v: 0.5,
                colour: 0,
            },
        ],
        false,
        false,
    )
    .expect("两片应建立上下文");
    let bordered =
        row_pieces_coloured_with_borders(&[row(50, 0.0, 100.0)], &ctx.map, ctx.box_, true);
    assert!(
        bordered.iter().all(|(_, colour)| *colour != AREA_BORDER),
        "空区域侧不得产生交界描边：{bordered:?}"
    );
}
