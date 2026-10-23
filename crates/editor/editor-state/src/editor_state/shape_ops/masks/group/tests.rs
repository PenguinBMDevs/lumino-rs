//! Mask group 组合测试

use super::*;
use crate::shape_ops::masks::area::RowSpan;

fn area(start: f32, end: f32) -> Area {
    Area::new([RowSpan {
        key: 60,
        start,
        end,
    }])
}

fn params(invert: bool, mode: MaskMode) -> MaskParams {
    MaskParams {
        invert,
        mode,
        ..MaskParams::default()
    }
}

fn member(base: (f32, f32), own: Option<(f32, f32)>, invert: bool, mode: MaskMode) -> MaskMember {
    MaskMember {
        params: params(invert, mode),
        base: area(base.0, base.1),
        own: own.map(|(a, b)| area(a, b)).unwrap_or_default(),
    }
}

#[test]
fn test_single_member_subtracts_own_notes() {
    let got = unit_area(&[member(
        (0.0, 100.0),
        Some((20.0, 30.0)),
        false,
        MaskMode::Add,
    )]);
    assert!(!got.invert);
    let spans = got.area.spans_on(60);
    assert_eq!(spans.len(), 2);
    assert_eq!((spans[0].start, spans[0].end), (0.0, 20.0));
    assert_eq!((spans[1].start, spans[1].end), (30.0, 100.0));
    assert_eq!(got.reach.spans_on(60).len(), 1, "reach 不受 own 影响");
}

#[test]
fn test_single_invert_joins_own_notes_into_hole() {
    let got = unit_area(&[member(
        (0.0, 100.0),
        Some((20.0, 30.0)),
        true,
        MaskMode::Add,
    )]);
    assert!(got.invert);
    assert_eq!(got.area.spans_on(60).len(), 1, "hole 应包含 own 音符");
    assert_eq!(got.area.spans_on(60)[0].end, 100.0);
}

#[test]
fn test_add_is_union_and_first_mode_is_ignored() {
    let got = unit_area(&[
        // 首个成员即使写成 Subtract，也应按 Add 处理
        member((0.0, 100.0), None, false, MaskMode::Subtract),
        member((50.0, 150.0), None, false, MaskMode::Add),
    ]);
    assert!(!got.invert);
    let spans = got.area.spans_on(60);
    assert_eq!(spans.len(), 1);
    assert_eq!((spans[0].start, spans[0].end), (0.0, 150.0));
}

#[test]
fn test_subtract_removes_middle() {
    let got = unit_area(&[
        member((0.0, 100.0), None, false, MaskMode::Add),
        member((40.0, 60.0), None, false, MaskMode::Subtract),
    ]);
    assert!(!got.invert);
    let spans = got.area.spans_on(60);
    assert_eq!(spans.len(), 2);
    assert_eq!((spans[0].start, spans[0].end), (0.0, 40.0));
    assert_eq!((spans[1].start, spans[1].end), (60.0, 100.0));
}

#[test]
fn test_intersect_keeps_overlap() {
    let got = unit_area(&[
        member((0.0, 100.0), None, false, MaskMode::Add),
        member((50.0, 150.0), None, false, MaskMode::Intersect),
    ]);
    assert!(!got.invert);
    let spans = got.area.spans_on(60);
    assert_eq!(spans.len(), 1);
    assert_eq!((spans[0].start, spans[0].end), (50.0, 100.0));
}

#[test]
fn test_subtract_inverted_is_intersection() {
    // Subtract = Intersect + 反向 Invert：A - (not B) = A and B
    let got = unit_area(&[
        member((0.0, 100.0), None, false, MaskMode::Add),
        member((40.0, 60.0), None, true, MaskMode::Subtract),
    ]);
    assert!(!got.invert);
    let spans = got.area.spans_on(60);
    assert_eq!(spans.len(), 1);
    assert_eq!((spans[0].start, spans[0].end), (40.0, 60.0));
}

#[test]
fn test_own_notes_are_unioned_before_final_cut() {
    let got = unit_area(&[
        member((0.0, 100.0), Some((20.0, 30.0)), false, MaskMode::Add),
        member((50.0, 150.0), Some((60.0, 70.0)), false, MaskMode::Add),
    ]);
    let spans = got.area.spans_on(60);
    assert_eq!(spans.len(), 3);
    assert_eq!((spans[0].start, spans[0].end), (0.0, 20.0));
    assert_eq!((spans[1].start, spans[1].end), (30.0, 60.0));
    assert_eq!((spans[2].start, spans[2].end), (70.0, 150.0));
    assert_eq!(got.reach.spans_on(60)[0].end, 150.0);
}
