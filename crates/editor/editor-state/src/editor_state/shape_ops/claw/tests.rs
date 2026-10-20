//! Claw 算法测试

use lumino_core::shape::{ClawMode, ClawParams, ClawPeriod, ClawTrash, GlueParams, ShapeOp};

use super::*;
use crate::shape_ops::{NoteOpContext, apply_note_ops};

const PPQ: f64 = 480.0;

fn note(tick: f32, key: u16, length: f32) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length,
        velocity: 100,
    }
}

fn sequence(step: f32, count: usize) -> Vec<ShapeNote> {
    (0..count)
        .map(|index| note(index as f32 * step, 60, 60.0))
        .collect()
}

fn time_params() -> ClawParams {
    ClawParams {
        mode: ClawMode::Time,
        period: ClawPeriod::One,
        trash: Some(ClawTrash { k: 1, n: 4 }),
        ..ClawParams::default()
    }
}

#[test]
fn test_time_trash_drops_second_quarter_and_closes() {
    let notes = sequence(120.0, 9); // 0,120,...,960
    let out = apply(&notes, &time_params(), PPQ);
    let ticks: Vec<f32> = out.iter().map(|n| n.tick).collect();
    assert_eq!(
        ticks,
        vec![0.0, 120.0, 240.0, 360.0, 480.0, 600.0, 720.0],
        "120/600 被丢弃，后续音符左移 120"
    );
}

#[test]
fn test_time_trash_none_drops_everything() {
    let params = ClawParams {
        trash: None,
        ..time_params()
    };
    assert!(apply(&sequence(120.0, 4), &params, PPQ).is_empty());
}

#[test]
fn test_time_period_longer_than_total_still_works() {
    let params = ClawParams {
        period: ClawPeriod::Sixteen,
        ..time_params()
    };
    let out = apply(&sequence(120.0, 4), &params, PPQ);
    assert_eq!(out.len(), 4, "丢弃片在总长之外，音符应全部保留");
}

#[test]
fn test_stretch_restores_original_span() {
    let params = ClawParams {
        stretch: true,
        ..time_params()
    };
    let notes = sequence(120.0, 9);
    let original_end = notes
        .iter()
        .map(|n| n.tick + n.length)
        .fold(0.0_f32, f32::max);
    let out = apply(&notes, &params, PPQ);
    let new_end = out
        .iter()
        .map(|n| n.tick + n.length)
        .fold(0.0_f32, f32::max);
    assert!(
        (new_end - original_end).abs() <= 1.0,
        "stretch 后总长应回到 {original_end}，实际 {new_end}"
    );
}

#[test]
fn test_dist_bends_time_within_segment() {
    let params = ClawParams {
        trash: Some(ClawTrash { k: 3, n: 4 }),
        dist: 100,
        ..time_params()
    };
    let notes = vec![
        note(0.0, 60, 10.0),
        note(120.0, 60, 10.0),
        note(240.0, 60, 10.0),
    ];
    let out = apply(&notes, &params, PPQ);
    let ticks: Vec<f32> = out.iter().map(|n| n.tick).collect();
    assert!((ticks[0] - 0.0).abs() < 1e-3);
    assert!((ticks[1] - 210.0).abs() < 1e-3, "u=0.25 → 0.4375");
    assert!((ticks[2] - 360.0).abs() < 1e-3, "u=0.5 → 0.75");
}

#[test]
fn test_short_removes_notes_cut_below_threshold() {
    let params = ClawParams {
        short: true,
        ..time_params()
    };
    let notes = vec![note(0.0, 60, 60.0), note(110.0, 62, 30.0)];
    let out = apply(&notes, &params, PPQ);
    assert_eq!(out.len(), 1, "被截到 10 tick 的新短音应被删除");
    assert_eq!(out[0].key, 60);
}

#[test]
fn test_count_notes_keeps_every_other() {
    let params = ClawParams {
        mode: ClawMode::Notes,
        keep: 1,
        skip: 1,
        ..ClawParams::default()
    };
    let out = apply(&sequence(10.0, 10), &params, PPQ);
    assert_eq!(out.len(), 5);
}

#[test]
fn test_count_keys_counts_empty_rows() {
    let params = ClawParams {
        mode: ClawMode::Keys,
        keep: 1,
        skip: 1,
        ..ClawParams::default()
    };
    let notes: Vec<ShapeNote> = (60..70).map(|key| note(0.0, key, 10.0)).collect();
    let out = apply(&notes, &params, PPQ);
    let keys: Vec<u16> = out.iter().map(|n| n.key).collect();
    assert_eq!(keys, vec![60, 62, 64, 66, 68]);
}

#[test]
fn test_count_chords_groups_same_tick() {
    let params = ClawParams {
        mode: ClawMode::Chords,
        keep: 1,
        skip: 1,
        ..ClawParams::default()
    };
    let notes = vec![
        note(0.0, 60, 10.0),
        note(0.0, 64, 10.0),
        note(10.0, 60, 10.0),
        note(10.0, 64, 10.0),
        note(20.0, 60, 10.0),
        note(20.0, 64, 10.0),
    ];
    let out = apply(&notes, &params, PPQ);
    assert_eq!(out.len(), 4, "保留第 0、2 个 chord");
    assert!(out.iter().all(|n| n.tick == 0.0 || n.tick == 20.0));
}

#[test]
fn test_random_is_reproducible_and_bounds() {
    let params = ClawParams {
        mode: ClawMode::Random,
        pct: 50,
        seed: 42,
        ..ClawParams::default()
    };
    let notes = sequence(10.0, 100);
    let first = apply(&notes, &params, PPQ);
    let second = apply(&notes, &params, PPQ);
    assert_eq!(first, second, "同 seed 必须复现");
    assert!(!first.is_empty() && first.len() < notes.len());

    let none = ClawParams {
        pct: 0,
        ..params.clone()
    };
    assert!(apply(&notes, &none, PPQ).is_empty());
    let all = ClawParams { pct: 100, ..params };
    assert_eq!(apply(&notes, &all, PPQ).len(), notes.len());
}

#[test]
fn test_shorten_keeps_dropped_notes_at_cut_percent() {
    let params = ClawParams {
        mode: ClawMode::Notes,
        keep: 1,
        skip: 1,
        shorten: true,
        cut: 50,
        ..ClawParams::default()
    };
    let notes = sequence(10.0, 4);
    let out = apply(&notes, &params, PPQ);
    assert_eq!(out.len(), 4, "shorten 不删除音符");
    assert_eq!(out[1].length, 30.0, "被丢弃音符长度 = 60 × 50%");
    assert_eq!(out[0].length, 60.0, "保留音符长度不变");
}

#[test]
fn test_boundaries_do_not_panic() {
    let params = time_params();
    assert!(apply(&[], &params, PPQ).is_empty());
    assert_eq!(apply(&[note(0.0, 60, 10.0)], &params, PPQ).len(), 1);
}

#[test]
fn test_chain_applies_claw_after_glue() {
    let mut notes = vec![note(0.0, 60, 120.0), note(120.0, 60, 120.0)];
    let ops = vec![
        ShapeOp::Glue(GlueParams::All),
        ShapeOp::Claw(ClawParams {
            mode: ClawMode::Notes,
            keep: 1,
            skip: 0,
            ..ClawParams::default()
        }),
    ];
    let ctx = NoteOpContext {
        glue_box: None,
        ppq: PPQ,
    };
    apply_note_ops(&mut notes, &ops, &ctx);
    assert_eq!(notes.len(), 1, "Glue 先合并，Claw 不再拆开");
    assert_eq!(notes[0].length, 240.0);
}
