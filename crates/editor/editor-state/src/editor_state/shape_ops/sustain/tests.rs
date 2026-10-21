//! Sustain 算法测试

use lumino_core::shape::{StopsParams, SustainParams};

use super::*;
use crate::ShapeNote;

const PPQ: f64 = 480.0;

fn note(tick: f32, key: u16, length: f32, velocity: u8) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length,
        velocity,
        channel: 0,
    }
}

fn params(gap_beats: Option<f32>) -> SustainParams {
    SustainParams {
        gap_beats,
        ..SustainParams::default()
    }
}

fn lengths(notes: &[ShapeNote]) -> Vec<f32> {
    notes.iter().map(|note| note.length).collect()
}

#[test]
fn test_auto_gap_extends_group_to_group_end() {
    let notes = vec![
        note(0.0, 60, 100.0, 100),
        note(150.0, 60, 100.0, 100),
        note(300.0, 60, 100.0, 100),
    ];
    let (out, report) = apply(&notes, &SustainParams::default(), PPQ);
    assert_eq!(
        lengths(&out),
        vec![400.0, 250.0, 100.0],
        "auto_gap=75，三条同组，全部延长到组末 400"
    );
    assert_eq!(report.stops, 0);
    assert!(!report.gave_up);
}

#[test]
fn test_manual_gap_can_keep_notes_separate() {
    let notes = vec![
        note(0.0, 60, 100.0, 100),
        note(150.0, 60, 100.0, 100),
        note(300.0, 60, 100.0, 100),
    ];
    let (out, _) = apply(&notes, &params(Some(0.1)), PPQ);
    assert_eq!(
        lengths(&out),
        vec![100.0, 100.0, 100.0],
        "gap=48 tick < 实际 50，三条各自成组"
    );

    let (out, _) = apply(&notes, &params(Some(0.2)), PPQ);
    assert_eq!(
        lengths(&out),
        vec![400.0, 250.0, 100.0],
        "gap=96 时重新合组"
    );
}

#[test]
fn test_most_caps_but_never_shortens() {
    let notes = vec![note(0.0, 60, 100.0, 100), note(100.0, 60, 300.0, 100)];
    let sustain = SustainParams {
        most_beats: Some(0.25), // 120 tick
        ..SustainParams::default()
    };
    let (out, _) = apply(&notes, &sustain, PPQ);
    assert_eq!(
        lengths(&out),
        vec![120.0, 300.0],
        "第一条截到 120；第二条原长 300 不能被缩短"
    );
}

#[test]
fn test_holes_are_not_crossed() {
    let notes = vec![
        note(0.0, 60, 100.0, 100),
        note(50.0, 60, 100.0, 100),
        note(500.0, 60, 100.0, 100),
        note(550.0, 60, 100.0, 100),
    ];
    let (out, _) = apply(&notes, &SustainParams::default(), PPQ);
    assert_eq!(lengths(&out), vec![150.0, 100.0, 150.0, 100.0]);
    assert!(
        out.iter().all(|note| !(150.0..500.0).contains(&note.tick)),
        "空洞 150..500 必须保持为空"
    );
}

#[test]
fn test_stop_splits_without_gap() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let sustain = SustainParams {
        every_beats: Some(0.5),
        ..SustainParams::default()
    };
    let (out, report) = apply(&notes, &sustain, PPQ);
    assert_eq!(report.stops, 1);
    assert_eq!(report.pieces, 2);
    let mut ticks: Vec<f32> = out.iter().map(|note| note.tick).collect();
    ticks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(ticks, vec![0.0, 240.0]);
    assert!(out.iter().all(|note| note.length == 240.0));
}

#[test]
fn test_stop_from_offset() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let sustain = SustainParams {
        every_beats: Some(0.5),
        from_beats: Some(0.25), // 120 tick
        ..SustainParams::default()
    };
    let (out, _) = apply(&notes, &sustain, PPQ);
    let mut ticks: Vec<f32> = out.iter().map(|note| note.tick).collect();
    ticks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(ticks, vec![0.0, 120.0, 360.0]);
}

#[test]
fn test_stop_merges_same_key_at_one_tick() {
    let notes = vec![note(0.0, 60, 300.0, 100), note(100.0, 60, 300.0, 90)];
    let sustain = SustainParams {
        every_beats: Some(0.5),
        ..SustainParams::default()
    };
    let (out, report) = apply(&notes, &sustain, PPQ);
    assert_eq!(report.stops, 1);
    assert_eq!(out.len(), 3, "两段 lead + 一个 stop 处合并音");
    let at_stop: Vec<&ShapeNote> = out.iter().filter(|note| note.tick == 240.0).collect();
    assert_eq!(at_stop.len(), 1, "同 tick 同 key 只能有一条");
    assert_eq!(at_stop[0].length, 160.0);
    assert_eq!(at_stop[0].velocity, 100, "力度取最大");
}

#[test]
fn test_stops_off_keeps_notes_whole() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let sustain = SustainParams {
        stops: Some(StopsParams {
            at: vec![0.5],
            past: false,
            off: true,
        }),
        ..SustainParams::default()
    };
    let (out, report) = apply(&notes, &sustain, PPQ);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].length, 480.0);
    assert_eq!(report.stops, 0);
}

#[test]
fn test_past_stop_at_end_keeps_last_group() {
    let notes = vec![note(0.0, 60, 100.0, 100), note(200.0, 60, 280.0, 100)];
    let sustain = SustainParams {
        stops: Some(StopsParams {
            at: vec![1.0],
            past: true,
            off: false,
        }),
        ..SustainParams::default()
    };
    let (out, report) = apply(&notes, &sustain, PPQ);
    assert_eq!(report.stops, 1);
    assert_eq!(lengths(&out), vec![480.0, 280.0]);
    assert!(
        out.iter().all(|note| note.tick < 480.0),
        "stop 落在末终点时不应额外生成新音符"
    );
}

#[test]
fn test_gives_up_pieces_over_limit() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let sustain = SustainParams {
        every_beats: Some(0.5),
        ..SustainParams::default()
    };
    let (out, report) = apply_with_limits(&notes, &sustain, PPQ, 1, 10);
    assert!(report.gave_up);
    assert_eq!(report.pieces, 2);
    assert_eq!(out.len(), 1, "放弃切 stop 时只保留延长后的音符");
    assert_eq!(out[0].length, 480.0);
}

#[test]
fn test_gives_up_stops_over_limit() {
    let notes = vec![note(0.0, 60, 480.0, 100)];
    let sustain = SustainParams {
        every_beats: Some(0.5),
        ..SustainParams::default()
    };
    let (out, report) = apply_with_limits(&notes, &sustain, PPQ, 100, 0);
    assert!(report.gave_up);
    assert_eq!(report.stops, 0);
    assert_eq!(report.pieces, 1);
    assert_eq!(out.len(), 1);
}

#[test]
fn test_empty_and_invalid_ppq_are_safe() {
    assert!(apply(&[], &SustainParams::default(), PPQ).0.is_empty());
    let notes = vec![note(0.0, 60, 120.0, 100)];
    assert_eq!(apply(&notes, &SustainParams::default(), 0.0).0, notes);
}

#[test]
fn test_most_and_reach_never_shorten() {
    let notes = vec![note(0.0, 60, 100.0, 100)];
    let sustain = SustainParams {
        most_beats: Some(0.05), // 24 tick
        stops: Some(StopsParams {
            at: vec![1.0],
            past: true,
            off: false,
        }),
        ..SustainParams::default()
    };
    let (out, _) = apply(&notes, &sustain, PPQ);
    assert_eq!(out[0].length, 100.0, "most 不能把原长 100 缩短");
}
