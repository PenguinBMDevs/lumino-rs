//! Chop 算法测试

use lumino_core::shape::ChopParams;

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
    }
}

fn preset(name: &str) -> ChopParams {
    ChopParams::builtin(name).expect("内置节奏应存在")
}

#[test]
fn test_even_is_identity_for_one_beat() {
    let (out, report) = apply(&[note(0.0, 60, 480.0, 100)], &preset("even"), PPQ);
    assert_eq!(report.pieces, 1);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].tick, 0.0);
    assert_eq!(out[0].length, 480.0);
    assert_eq!(out[0].velocity, 100);
}

#[test]
fn test_half_leaves_a_rest() {
    let (out, _) = apply(&[note(0.0, 60, 480.0, 100)], &preset("half"), PPQ);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].length, 240.0);
}

#[test]
fn test_swing_boundaries_and_velocity() {
    let (out, _) = apply(&[note(0.0, 60, 960.0, 100)], &preset("swing"), PPQ);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].tick, 0.0);
    assert!((out[0].length - 640.0).abs() < 1e-3);
    assert_eq!(out[0].velocity, 100);
    assert!((out[1].tick - 640.0).abs() < 1e-3);
    assert!((out[1].length - 320.0).abs() < 1e-3);
    assert_eq!(out[1].velocity, 75);
}

#[test]
fn test_offbeat_starts_at_second_beat() {
    let (out, _) = apply(&[note(0.0, 60, 960.0, 100)], &preset("offbeat"), PPQ);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].tick, 480.0);
    assert_eq!(out[0].length, 480.0);
}

#[test]
fn test_gallop_three_pieces() {
    let (out, _) = apply(&[note(0.0, 60, 960.0, 100)], &preset("gallop"), PPQ);
    assert_eq!(out.len(), 3);
    assert_eq!(out[2].velocity, 80);
}

#[test]
fn test_triplets_three_equal_pieces() {
    let (out, _) = apply(&[note(0.0, 60, 480.0, 100)], &preset("triplets"), PPQ);
    assert_eq!(out.len(), 3);
    for piece in &out {
        assert!((piece.length - 160.0).abs() < 1e-3);
    }
    assert_eq!(out[1].velocity, 80);
}

#[test]
fn test_accent_four_pieces() {
    let (out, _) = apply(&[note(0.0, 60, 1920.0, 100)], &preset("accent"), PPQ);
    assert_eq!(out.len(), 4);
    assert_eq!(out[0].velocity, 100);
    assert_eq!(out[1].velocity, 65);
}

#[test]
fn test_stutter_five_pieces() {
    let (out, _) = apply(&[note(0.0, 60, 1920.0, 100)], &preset("stutter"), PPQ);
    assert_eq!(out.len(), 5);
    assert_eq!(out[4].length, 960.0);
}

#[test]
fn test_abs_aligns_to_tick_zero() {
    let params = ChopParams {
        abs: true,
        ..preset("even")
    };
    let (out, _) = apply(&[note(240.0, 60, 480.0, 100)], &params, PPQ);
    assert_eq!(out.len(), 2, "首片被切短，第二片落在绝对网格");
    assert_eq!(out[0].tick, 240.0);
    assert_eq!(out[0].length, 240.0);
    assert_eq!(out[1].tick, 480.0);
    assert_eq!(out[1].length, 240.0);
}

#[test]
fn test_piece_crossing_note_end_is_truncated() {
    let (out, _) = apply(&[note(0.0, 60, 600.0, 100)], &preset("gallop"), PPQ);
    assert_eq!(out.len(), 2);
    assert_eq!(out[1].tick, 480.0);
    assert_eq!(out[1].length, 120.0);
}

#[test]
fn test_fixed_mixing_and_vel_zero() {
    let fixed = ChopParams {
        fixed: true,
        vel: 50,
        ..preset("accent")
    };
    let (out, _) = apply(&[note(0.0, 60, 1920.0, 100)], &fixed, PPQ);
    assert_eq!(out[1].velocity, 83, "(100 + 65) / 2");

    let keep = ChopParams {
        vel: 0,
        ..preset("accent")
    };
    let (out, _) = apply(&[note(0.0, 60, 1920.0, 100)], &keep, PPQ);
    assert!(out.iter().all(|piece| piece.velocity == 100));
}

#[test]
fn test_contiguous_same_key_notes_form_one_run() {
    let notes = vec![note(0.0, 60, 240.0, 100), note(240.0, 60, 240.0, 100)];
    let (out, _) = apply(&notes, &preset("half"), PPQ);
    assert_eq!(out.len(), 1, "两条相接音符先合并成 480 run，再切成 240");
    assert_eq!(out[0].tick, 0.0);
    assert_eq!(out[0].length, 240.0);
}

#[test]
fn test_different_keys_are_separate_runs() {
    let notes = vec![note(0.0, 60, 480.0, 100), note(0.0, 64, 480.0, 100)];
    let (out, _) = apply(&notes, &preset("half"), PPQ);
    assert_eq!(out.len(), 2);
}

#[test]
fn test_empty_pieces_remove_notes() {
    let params = ChopParams {
        pieces: Vec::new(),
        ..preset("even")
    };
    let (out, report) = apply(&[note(0.0, 60, 480.0, 100)], &params, PPQ);
    assert!(out.is_empty());
    assert_eq!(report.pieces, 0);
}

#[test]
fn test_over_limit_gives_up_and_returns_original() {
    let params = ChopParams {
        len: 0.0,
        pieces: vec![lumino_core::shape::ChopPiece {
            start: 0.0,
            len: 1.0,
            vel: 100,
        }],
        ..preset("even")
    };
    let notes = vec![note(0.0, 60, 100.0, 100)];
    let (out, report) = apply_with_limit(&notes, &params, PPQ, 10);
    assert!(report.gave_up);
    assert_eq!(out, notes, "放弃时必须返回原 notes");
}

#[test]
fn test_boundaries_do_not_panic() {
    let params = preset("even");
    assert!(apply(&[], &params, PPQ).0.is_empty());
    assert!(apply(&[note(0.0, 60, 0.0, 100)], &params, PPQ).0.is_empty());
}
