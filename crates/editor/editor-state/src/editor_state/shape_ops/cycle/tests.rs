//! Colours 通道循环测试

use lumino_core::shape::{CycleBy, CycleParams, RowRestart};

use super::*;

fn note(tick: f32, key: u16) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length: 120.0,
        velocity: 100,
        channel: 0,
    }
}

fn run(notes: &mut [ShapeNote], params: CycleParams, ppq: f64) {
    apply(notes, &params, ppq);
}

#[test]
fn test_step_cycles_channels() {
    let mut notes = vec![
        note(0.0, 60),
        note(120.0, 60),
        note(240.0, 60),
        note(360.0, 60),
    ];
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Step,
            n: 3,
            ..CycleParams::default()
        },
        480.0,
    );
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![0, 1, 2, 0]
    );
}

#[test]
fn test_step_every_groups_steps() {
    let mut notes = (0..4)
        .map(|i| note(i as f32 * 120.0, 60))
        .collect::<Vec<_>>();
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Step,
            n: 2,
            every: 2,
            ..CycleParams::default()
        },
        480.0,
    );
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![0, 0, 1, 1]
    );
}

#[test]
fn test_key_cycles_rows() {
    let mut notes = vec![note(0.0, 60), note(0.0, 61), note(0.0, 62), note(0.0, 63)];
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Key,
            n: 2,
            ..CycleParams::default()
        },
        480.0,
    );
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![0, 1, 0, 1]
    );
}

#[test]
fn test_time_uses_every_time_fraction() {
    let mut notes = vec![note(0.0, 60), note(480.0, 60), note(960.0, 60)];
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Time,
            n: 3,
            every_time: Some([1, 4]), // 一拍 = 480 tick
            ..CycleParams::default()
        },
        480.0,
    );
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn test_rows_first_restarts_each_key() {
    let mut notes = vec![
        note(0.0, 60),
        note(120.0, 60),
        note(0.0, 61),
        note(120.0, 61),
    ];
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Step,
            n: 2,
            rows: Some(RowRestart::First),
            ..CycleParams::default()
        },
        480.0,
    );
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![0, 1, 0, 1]
    );
}

#[test]
fn test_rows_end_counts_backwards() {
    let mut notes = vec![note(0.0, 60), note(120.0, 60), note(240.0, 60)];
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Step,
            n: 3,
            rows: Some(RowRestart::End),
            ..CycleParams::default()
        },
        480.0,
    );
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![2, 1, 0]
    );
}

#[test]
fn test_shift_bends_rows_diagonally() {
    let mut notes = vec![note(0.0, 60), note(0.0, 61), note(0.0, 62), note(0.0, 63)];
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Key,
            n: 3,
            every: 1,
            shift: Some([1, 2]),
            ..CycleParams::default()
        },
        480.0,
    );
    // row 0/1: 0,1；row 2/3 再 +1 → 0,1（mod 3）
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![0, 1, 0, 1]
    );
}

#[test]
fn test_only_channel_changes_and_n16_is_safe() {
    let mut notes = (0..20).map(|i| note(i as f32, 60)).collect::<Vec<_>>();
    let before: Vec<_> = notes.iter().map(|n| (n.tick, n.key, n.length)).collect();
    run(
        &mut notes,
        CycleParams {
            by: CycleBy::Step,
            n: 16,
            ..CycleParams::default()
        },
        480.0,
    );
    assert!(notes.iter().all(|n| n.channel < 16));
    assert_eq!(
        notes
            .iter()
            .map(|n| (n.tick, n.key, n.length))
            .collect::<Vec<_>>(),
        before,
        "除 channel 外不得改任何字段"
    );
}

#[test]
fn test_invalid_n_and_empty_are_safe() {
    let mut notes = vec![note(0.0, 60)];
    assert_eq!(
        apply(
            &mut notes,
            &CycleParams {
                n: 1,
                ..CycleParams::default()
            },
            480.0
        ),
        0
    );
    assert!(apply(&mut [], &CycleParams::default(), 480.0) == 0);
}
