//! Strum 算法测试

use lumino_core::shape::StrumParams;

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

fn chord(keys: &[u16], velocity: u8) -> Vec<ShapeNote> {
    keys.iter()
        .map(|&key| note(0.0, key, 480.0, velocity))
        .collect()
}

fn find(notes: &[ShapeNote], key: u16) -> &ShapeNote {
    notes
        .iter()
        .find(|note| note.key == key)
        .expect("测试音符应存在")
}

#[test]
fn test_basic_even_strum() {
    let params = StrumParams {
        time: 1.0 / 3.0,
        ..StrumParams::default()
    };
    let out = apply(&chord(&[60, 61, 62, 63], 100), &params, PPQ);
    assert!((find(&out, 60).tick - 0.0).abs() < 1e-3);
    assert!((find(&out, 61).tick - 160.0).abs() < 1e-3);
    assert!((find(&out, 62).tick - 320.0).abs() < 1e-3);
    assert!((find(&out, 63).tick - 480.0).abs() < 1e-3);
}

#[test]
fn test_tension_doubles_and_halves() {
    let double = StrumParams {
        time: 1.0,
        tension: 100.0,
        ..StrumParams::default()
    };
    let out = apply(&chord(&[60, 61, 62, 63], 100), &double, PPQ);
    assert_eq!(
        out.iter().map(|n| n.tick).collect::<Vec<_>>(),
        vec![0.0, 480.0, 1440.0, 3360.0]
    );

    let half = StrumParams {
        time: 1.0,
        tension: -100.0,
        ..StrumParams::default()
    };
    let out = apply(&chord(&[60, 61, 62, 63], 100), &half, PPQ);
    assert_eq!(
        out.iter().map(|n| n.tick).collect::<Vec<_>>(),
        vec![0.0, 480.0, 720.0, 840.0]
    );
}

#[test]
fn test_preserve_only_moves_start() {
    let params = StrumParams {
        time: 1.0,
        preserve: true,
        ..StrumParams::default()
    };
    let notes = vec![note(0.0, 60, 1000.0, 100), note(0.0, 64, 1000.0, 100)];
    let out = apply(&notes, &params, PPQ);
    assert_eq!(find(&out, 60).length, 1000.0);
    assert_eq!(find(&out, 64).tick, 480.0);
    assert_eq!(find(&out, 64).length, 520.0, "preserve 只改起点");
}

#[test]
fn test_ahead_puts_last_note_on_original_tick() {
    let params = StrumParams {
        time: 1.0,
        ahead: true,
        ..StrumParams::default()
    };
    let notes: Vec<ShapeNote> = [60, 61, 62, 63]
        .into_iter()
        .map(|key| note(10_000.0, key, 480.0, 100))
        .collect();
    let out = apply(&notes, &params, PPQ);
    assert_eq!(find(&out, 63).tick, 10_000.0);
    assert!(out.iter().all(|note| note.tick >= 0.0));
    assert_eq!(find(&out, 60).tick, 10_000.0 - 1440.0);
}

#[test]
fn test_ahead_never_goes_negative() {
    let params = StrumParams {
        time: 1.0,
        ahead: true,
        ..StrumParams::default()
    };
    let out = apply(&chord(&[60, 61, 62, 63], 100), &params, PPQ);
    assert!(out.iter().all(|note| note.tick >= 0.0));
}

#[test]
fn test_end_sweep_shortens_earlier_ends() {
    let params = StrumParams {
        end: true,
        end_time: 0.1,
        ..StrumParams::default()
    };
    let out = apply(&chord(&[60, 61, 62, 63], 100), &params, PPQ);
    assert_eq!(find(&out, 60).length, 336.0);
    assert_eq!(find(&out, 61).length, 384.0);
    assert_eq!(find(&out, 62).length, 432.0);
    assert_eq!(find(&out, 63).length, 480.0);
}

#[test]
fn test_velocity_sweep() {
    let params = StrumParams {
        vel: -10,
        ..StrumParams::default()
    };
    let out = apply(&chord(&[60, 61, 62, 63], 100), &params, PPQ);
    assert_eq!(find(&out, 60).velocity, 100);
    assert_eq!(find(&out, 61).velocity, 90);
    assert_eq!(find(&out, 62).velocity, 80);
    assert_eq!(find(&out, 63).velocity, 70);
}

#[test]
fn test_alternate_reverses_second_real_chord_only() {
    let params = StrumParams {
        time: 1.0,
        alternate: true,
        ..StrumParams::default()
    };
    let notes = vec![
        note(0.0, 60, 480.0, 100),
        note(0.0, 64, 480.0, 100),
        note(240.0, 60, 480.0, 100), // 单音：不参与 alternate 计数
        note(480.0, 60, 480.0, 100),
        note(480.0, 64, 480.0, 100),
    ];
    let out = apply(&notes, &params, PPQ);
    assert_eq!(find(&out, 60).tick, 0.0, "第一个和弦正序");
    assert_eq!(find_tick(&out, 64, 0.0), 480.0);
    assert_eq!(find_tick(&out, 60, 240.0), 240.0, "单音不动");
    assert_eq!(find_tick(&out, 64, 480.0), 480.0, "第二个和弦反向：高音先");
    assert_eq!(find_tick(&out, 60, 480.0), 960.0);
}

fn find_tick(notes: &[ShapeNote], key: u16, original_tick: f32) -> f32 {
    notes
        .iter()
        .find(|note| note.key == key && note.tick >= original_tick - 1e-3)
        .map(|note| note.tick)
        .unwrap_or(original_tick)
}

#[test]
fn test_single_note_and_time_zero_unchanged() {
    let single = vec![note(0.0, 60, 100.0, 100)];
    let params = StrumParams {
        time: 1.0,
        alternate: true,
        ..StrumParams::default()
    };
    assert_eq!(apply(&single, &params, PPQ), single);
    assert_eq!(
        apply(&chord(&[60, 64], 100), &StrumParams::default(), PPQ).len(),
        2
    );
}

#[test]
fn test_chop_splits_long_note_and_joins_later_chord() {
    let params = StrumParams {
        time: 1.0 / 3.0,
        chop: true,
        ..StrumParams::default()
    };
    let notes = vec![note(0.0, 60, 1000.0, 100), note(500.0, 64, 100.0, 100)];
    let out = apply(&notes, &params, PPQ);
    assert_eq!(out.len(), 3, "长音在 500 处被切开");
    let pieces: Vec<&ShapeNote> = out.iter().filter(|n| n.key == 60).collect();
    assert_eq!(pieces.len(), 2);
    assert_eq!(pieces[0].tick, 0.0);
    assert_eq!(pieces[1].tick, 500.0);
    assert!((find(&out, 64).tick - 660.0).abs() < 1e-3);
}

#[test]
fn test_cap_clamps_extreme_offsets() {
    let params = StrumParams {
        time: 4.0,
        tension: 100.0,
        ..StrumParams::default()
    };
    let out = apply(&chord(&(60..70).collect::<Vec<_>>(), 100), &params, PPQ);
    let max = out.iter().map(|n| n.tick).fold(0.0_f32, f32::max);
    assert!(
        max <= 64.0 * 480.0 + 1e-3,
        "位移必须被 CAP=64 拍封顶，实际 {max}"
    );
}

#[test]
fn test_negative_time_reverses_order() {
    let params = StrumParams {
        time: -1.0,
        ..StrumParams::default()
    };
    let out = apply(&chord(&[60, 64], 100), &params, PPQ);
    assert_eq!(find(&out, 64).tick, 0.0, "高音先");
    assert_eq!(find(&out, 60).tick, 480.0);
}
