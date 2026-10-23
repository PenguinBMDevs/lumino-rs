//! Merge 测试

use lumino_core::shape::MergeParams;

use super::*;

fn note(key: u16, tick: f32, length: f32) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length,
        velocity: 100,
        channel: 0,
    }
}

fn apply(notes: &[ShapeNote], outcome: &MergeOutcome) -> Vec<ShapeNote> {
    notes
        .iter()
        .map(|note| {
            let shift = outcome.by_key.get(&note.key).copied().unwrap_or(0) + outcome.shift_all;
            ShapeNote {
                tick: note.tick + shift as f32,
                ..*note
            }
        })
        .collect()
}

#[test]
fn test_from_left_meets_stay_start() {
    let stay = vec![note(60, 100.0, 100.0)];
    let slide = vec![note(60, 300.0, 50.0)];
    let outcome = gate_merge(
        &stay,
        &slide,
        &MergeParams {
            stay: 1,
            from_left: Some(true),
            rest_slides: false,
        },
    )
    .expect("应接上");
    let shifted = apply(&slide, &outcome);
    assert!((shifted[0].tick + shifted[0].length - 100.0).abs() < 1e-3);
    assert_eq!(shifted[0].length, 50.0, "长度不变");
    assert_eq!(outcome.met_keys, vec![60]);
}

#[test]
fn test_from_right_meets_stay_end() {
    let stay = vec![note(60, 100.0, 100.0)];
    let slide = vec![note(60, 300.0, 50.0)];
    let outcome = gate_merge(
        &stay,
        &slide,
        &MergeParams {
            stay: 1,
            from_left: Some(false),
            rest_slides: false,
        },
    )
    .expect("应接上");
    let shifted = apply(&slide, &outcome);
    assert!((shifted[0].tick - 200.0).abs() < 1e-3);
}

#[test]
fn test_leftover_slides_with_rest() {
    let stay = vec![note(60, 100.0, 100.0)];
    let slide = vec![note(60, 300.0, 50.0), note(61, 500.0, 50.0)];
    let outcome = gate_merge(
        &stay,
        &slide,
        &MergeParams {
            stay: 1,
            from_left: Some(true),
            rest_slides: true,
        },
    )
    .expect("应接上");
    assert_eq!(outcome.by_key.get(&61), outcome.by_key.get(&60));
    let outcome = gate_merge(
        &stay,
        &slide,
        &MergeParams {
            stay: 1,
            from_left: Some(true),
            rest_slides: false,
        },
    )
    .expect("应接上");
    assert_eq!(outcome.by_key.get(&61), Some(&0));
}

#[test]
fn test_negative_tick_shifts_all() {
    let stay = vec![note(60, 0.0, 10.0)];
    let slide = vec![note(60, 500.0, 50.0)];
    let outcome = gate_merge(
        &stay,
        &slide,
        &MergeParams {
            stay: 1,
            from_left: Some(true),
            rest_slides: false,
        },
    )
    .expect("应接上");
    let shifted = apply(&slide, &outcome);
    assert!(shifted[0].tick >= 0.0, "整体后移后不应为负");
}

#[test]
fn test_no_common_key_returns_none() {
    let stay = vec![note(60, 0.0, 10.0)];
    let slide = vec![note(61, 100.0, 10.0)];
    assert!(
        gate_merge(
            &stay,
            &slide,
            &MergeParams {
                stay: 1,
                from_left: None,
                rest_slides: true,
            }
        )
        .is_none()
    );
}

#[test]
fn test_comes_from_left() {
    let stay = vec![note(60, 100.0, 100.0)];
    let left = vec![note(60, 0.0, 50.0)];
    let right = vec![note(60, 300.0, 50.0)];
    assert!(comes_from_left(&stay, &left));
    assert!(!comes_from_left(&stay, &right));
}
