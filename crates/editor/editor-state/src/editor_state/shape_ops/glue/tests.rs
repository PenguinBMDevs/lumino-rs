//! Glue 算法测试

use lumino_core::shape::ShapeRect;

use super::*;

fn note(track: usize, tick: f32, key: u16, length: f32) -> ShapeNote {
    ShapeNote {
        track,
        tick,
        key,
        length,
        velocity: 100,
        channel: 0,
    }
}

fn shape_box() -> ShapeRect {
    ShapeRect::normalized(0.0, 59.5, 100.0, 72.5)
}

#[test]
fn test_all_merges_ten_adjacent_notes_into_one() {
    let notes: Vec<ShapeNote> = (0..10)
        .map(|i| note(0, i as f32 * 10.0, 60, 10.0))
        .collect();
    let out = apply(&notes, &GlueParams::All, None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].tick, 0.0);
    assert_eq!(out[0].length, 100.0, "总覆盖应保持不变");
}

#[test]
fn test_gap_of_one_tick_breaks() {
    let notes = vec![note(0, 0.0, 60, 10.0), note(0, 11.0, 60, 10.0)];
    let out = apply(&notes, &GlueParams::All, None);
    assert_eq!(out.len(), 2, "gap = 1 tick 必须断开");
}

#[test]
fn test_sub_tick_gap_merges() {
    let notes = vec![note(0, 0.0, 60, 10.0), note(0, 10.5, 60, 10.0)];
    let out = apply(&notes, &GlueParams::All, None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].length, 20.5);
}

#[test]
fn test_overlap_is_contained() {
    let notes = vec![note(0, 0.0, 60, 10.0), note(0, 2.0, 60, 3.0)];
    let out = apply(&notes, &GlueParams::All, None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].tick, 0.0);
    assert_eq!(out[0].length, 10.0, "重叠包含不得缩短覆盖");
}

#[test]
fn test_different_track_and_key_do_not_merge() {
    let notes = vec![
        note(0, 0.0, 60, 10.0),
        note(1, 0.0, 60, 10.0),
        note(0, 0.0, 61, 10.0),
    ];
    let out = apply(&notes, &GlueParams::All, None);
    assert_eq!(out.len(), 3);
}

#[test]
fn test_boxes_filter_by_midpoint() {
    let notes = vec![
        note(0, 0.0, 60, 2.0),  // mid 1 → u = 0.01
        note(0, 2.0, 60, 2.0),  // mid 3 → u = 0.03
        note(0, 20.0, 60, 2.0), // mid 21 → u = 0.21
    ];
    let params = GlueParams::Boxes(vec![[0.0, 0.0, 0.1, 1.0]]);
    let out = apply(&notes, &params, Some(shape_box()));
    assert_eq!(out.len(), 2, "框内两条合并，框外一条保留");
    assert_eq!(out[0].length, 4.0);
    assert_eq!(out[1].tick, 20.0);
}

#[test]
fn test_boxes_filter_by_key_row() {
    let notes = vec![note(0, 0.0, 60, 2.0), note(0, 0.0, 70, 2.0)];
    let params = GlueParams::Boxes(vec![[0.0, 0.0, 1.0, 0.5]]);
    let out = apply(&notes, &params, Some(shape_box()));
    assert_eq!(out.len(), 2, "不同 key 行不粘");
    assert!(out.iter().any(|n| n.key == 60));
    assert!(out.iter().any(|n| n.key == 70));
}

#[test]
fn test_boxes_reversed_corners_are_normalized() {
    let notes = vec![note(0, 0.0, 60, 2.0), note(0, 2.0, 60, 2.0)];
    let params = GlueParams::Boxes(vec![[0.1, 1.0, 0.0, 0.0]]);
    let out = apply(&notes, &params, Some(shape_box()));
    assert_eq!(out.len(), 1);
}

#[test]
fn test_empty_boxes_and_missing_box_do_nothing() {
    let notes = vec![note(0, 0.0, 60, 10.0), note(0, 10.0, 60, 10.0)];
    assert_eq!(
        apply(&notes, &GlueParams::Boxes(Vec::new()), Some(shape_box())),
        notes
    );
    assert_eq!(
        apply(&notes, &GlueParams::Boxes(vec![[0.0, 0.0, 1.0, 1.0]]), None),
        notes
    );
}

#[test]
fn test_output_is_deterministic() {
    let notes: Vec<ShapeNote> = (0..20)
        .map(|i| note(0, (i % 5) as f32 * 10.0, 60 + (i % 3) as u16, 10.0))
        .collect();
    let first = apply(&notes, &GlueParams::All, None);
    let second = apply(&notes, &GlueParams::All, None);
    assert_eq!(first, second);
}

#[test]
fn test_single_note_unchanged() {
    let notes = vec![note(0, 5.0, 60, 10.0)];
    assert_eq!(apply(&notes, &GlueParams::All, None), notes);
}
