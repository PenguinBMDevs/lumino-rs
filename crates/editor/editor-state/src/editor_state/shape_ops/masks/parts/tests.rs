//! Mask 切分 / 可见片段测试

use super::*;
use crate::shape_ops::masks::area::RowSpan;

fn note(tick: f32, key: u16, length: f32, velocity: u8) -> ShapeNote {
    ShapeNote {
        track: 3,
        tick,
        key,
        length,
        velocity,
        channel: 0,
    }
}

fn area_on(key: u16, spans: &[(f32, f32)]) -> Area {
    Area::new(
        spans
            .iter()
            .map(|&(start, end)| RowSpan { key, start, end }),
    )
}

fn area(spans: &[(f32, f32)]) -> Area {
    area_on(60, spans)
}

fn unit_on(key: u16, spans: &[(f32, f32)], invert: bool) -> UnitArea {
    let area = area_on(key, spans);
    UnitArea {
        area: area.clone(),
        reach: area,
        invert,
    }
}

fn unit(spans: &[(f32, f32)], invert: bool) -> UnitArea {
    unit_on(60, spans, invert)
}

#[test]
fn test_intersect_keeps_all_pieces() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let got = shown_parts(&notes, &unit(&[(100.0, 200.0), (300.0, 400.0)], false));
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].start, got[0].end), (100.0, 200.0));
    assert_eq!((got[1].start, got[1].end), (300.0, 400.0));
}

#[test]
fn test_one_tick_fragment_is_kept() {
    let notes = vec![note(0.0, 60, 100.0, 100)];
    let got = shown_parts(&notes, &unit(&[(0.0, 1.0)], false));
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].start, got[0].end),
        (0.0, 1.0),
        "1 tick 碎片也必须保留"
    );

    let tail = shown_parts(&notes, &unit(&[(99.0, 100.0)], false));
    assert_eq!(tail.len(), 1);
    assert_eq!((tail[0].start, tail[0].end), (99.0, 100.0));
}

#[test]
fn test_invert_returns_complement_pieces() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let got = shown_parts(&notes, &unit(&[(100.0, 300.0)], true));
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].start, got[0].end), (0.0, 100.0));
    assert_eq!((got[1].start, got[1].end), (300.0, 480.0));
}

#[test]
fn test_full_inside_is_one_piece_and_invert_hides_it() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let inside = shown_parts(&notes, &unit(&[(0.0, 480.0)], false));
    assert_eq!(inside.len(), 1);
    assert_eq!((inside[0].start, inside[0].end), (0.0, 480.0));

    let hole = shown_parts(&notes, &unit(&[(0.0, 480.0)], true));
    assert!(hole.is_empty(), "完全落在洞里应无可见片段");
}

#[test]
fn test_covered_and_preview_keep_ghosts() {
    let notes = vec![note(0.0, 60, 480.0, 100), note(0.0, 60, 480.0, 90)];
    let unit = unit(&[(100.0, 200.0)], false);
    assert!(covered(&notes, &unit.reach));
    let view = preview(&notes, &unit).expect("应被遮罩覆盖");
    assert_eq!(view.ghosts.len(), 2, "ghost 保留原音符总数");
    assert_eq!(view.visible.len(), 2, "两条音符各切出一段");

    let far = UnitArea {
        area: area(&[(900.0, 950.0)]),
        reach: area(&[(900.0, 950.0)]),
        invert: false,
    };
    assert!(!covered(&notes, &far.reach));
    assert!(preview(&notes, &far).is_none(), "未覆盖的形状应保持原样");
}

#[test]
fn test_apply_visible_preserves_track_key_velocity() {
    let notes = vec![note(50.0, 64, 100.0, 77)];
    let got = apply_visible(&notes, &unit_on(64, &[(50.0, 100.0)], false));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].tick, 50.0);
    assert_eq!(got[0].length, 50.0);
    assert_eq!(got[0].track, 3);
    assert_eq!(got[0].key, 64);
    assert_eq!(got[0].velocity, 77);
}

#[test]
fn test_zero_length_note_is_ignored() {
    let notes = vec![note(10.0, 60, 0.0, 100)];
    assert!(shown_parts(&notes, &unit(&[(0.0, 100.0)], false)).is_empty());
    assert!(!covered(&notes, &unit(&[(0.0, 100.0)], false).reach));
}
