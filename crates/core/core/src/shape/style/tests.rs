//! 形状样式测试

use super::*;
use crate::shape::AreaSpot;
use crate::shape::{AREA_COLOUR_MAX, COLOUR_SLOT_MAX, STROKE_WIDTH_MAX};

#[test]
fn test_style_default_matches_reference() {
    let style = ShapeStyle::default();
    assert_eq!(style.fill, ShapeFill::Empty);
    assert_eq!(style.gate_beats, 0.0625, "门长默认值应与 Spiderweb 一致");
    assert_eq!(style.stroke_width, 1);
    assert_eq!(style.velocity, 100);
    assert_eq!(style.colour_slot, 0);
    assert_eq!(style.align, SpamAlign::Auto, "v2 新字段默认 align=Auto");
    assert_eq!(style.ends, SpamEnds::Round, "v2 新字段默认 ends=Round");
    assert!(!style.union, "v2 新字段默认 union=false");
    assert!(!style.apart, "v2 新字段默认 apart=false");
    assert!(!style.borders, "v2 新字段默认 borders=false");
    assert!(style.areas.is_empty(), "v2 新字段默认 areas 为空");
    style.validate().expect("默认样式应通过校验");
}

#[test]
fn test_fill_needs_gate() {
    assert!(!ShapeFill::Empty.needs_gate());
    assert!(!ShapeFill::Fill.needs_gate());
    assert!(ShapeFill::Spam.needs_gate());
    assert!(ShapeFill::OutlineSpam.needs_gate());
    assert_eq!(ShapeFill::default(), ShapeFill::Empty);
}

#[test]
fn test_style_validate_rejects_bad_gate() {
    for bad in [0.0_f32, -0.5, f32::NAN, f32::INFINITY] {
        let style = ShapeStyle {
            gate_beats: bad,
            ..ShapeStyle::default()
        };
        assert!(style.validate().is_err(), "门长 {bad} 应被拒绝");
    }
}

#[test]
fn test_style_validate_rejects_bad_stroke_width() {
    for bad in [0_u8, STROKE_WIDTH_MAX + 1] {
        let style = ShapeStyle {
            stroke_width: bad,
            ..ShapeStyle::default()
        };
        let err = style.validate().expect_err("描边宽度越界应被拒绝");
        assert!(format!("{err}").contains("描边宽度越界"), "实际错误：{err}");
    }
}

#[test]
fn test_style_validate_rejects_bad_velocity() {
    let style = ShapeStyle {
        velocity: 128,
        ..ShapeStyle::default()
    };
    let err = style.validate().expect_err("力度越界应被拒绝");
    assert!(format!("{err}").contains("力度越界"), "实际错误：{err}");
}

#[test]
fn test_style_validate_rejects_bad_colour_slot() {
    let style = ShapeStyle {
        colour_slot: COLOUR_SLOT_MAX + 1,
        ..ShapeStyle::default()
    };
    let err = style.validate().expect_err("颜色槽越界应被拒绝");
    assert!(format!("{err}").contains("颜色槽越界"), "实际错误：{err}");
}

#[test]
fn test_style_validate_accepts_boundary_values() {
    let style = ShapeStyle {
        fill: ShapeFill::Spam,
        gate_beats: f32::MIN_POSITIVE,
        stroke_width: STROKE_WIDTH_MAX,
        velocity: 127,
        colour_slot: COLOUR_SLOT_MAX,
        align: SpamAlign::Centred,
        ends: SpamEnds::Stretch,
        union: true,
        apart: true,
        borders: true,
        areas: vec![AreaSpot {
            u: 0.0,
            v: 1.0,
            colour: AREA_COLOUR_MAX,
        }],
    };
    style.validate().expect("边界值应通过校验");
}

#[test]
fn test_style_validate_rejects_bad_area_colour() {
    let style = ShapeStyle {
        areas: vec![AreaSpot {
            u: 0.5,
            v: 0.5,
            colour: AREA_COLOUR_MAX + 1,
        }],
        ..ShapeStyle::default()
    };
    let err = style.validate().expect_err("区域色号越界应被拒绝");
    assert!(
        format!("{err}").contains("区域着色点非法"),
        "实际错误：{err}"
    );
    assert!(format!("{err}").contains("色号越界"), "实际错误：{err}");
}

#[test]
fn test_style_validate_rejects_bad_area_coords() {
    let style = ShapeStyle {
        areas: vec![AreaSpot {
            u: 1.5,
            v: 0.5,
            colour: 1,
        }],
        ..ShapeStyle::default()
    };
    let err = style.validate().expect_err("区域坐标越界应被拒绝");
    assert!(format!("{err}").contains("坐标越界"), "实际错误：{err}");
}
