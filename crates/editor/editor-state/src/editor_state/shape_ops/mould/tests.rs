//! Mould 测试

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

fn contour() -> Vec<Vec<Pt>> {
    vec![vec![
        (50.0, 59.0),
        (350.0, 59.0),
        (350.0, 63.0),
        (50.0, 63.0),
        (50.0, 59.0),
    ]]
}

#[test]
fn test_gap_inside_contour_becomes_row() {
    let walls = vec![note(60, 0.0, 100.0), note(60, 200.0, 100.0)];
    let rows = mould_rows(&walls, &contour(), 480);
    assert_eq!(rows, vec![(60, 100.0, 200.0)]);
}

#[test]
fn test_gap_outside_contour_is_ignored() {
    let walls = vec![note(70, 0.0, 100.0), note(70, 200.0, 100.0)];
    assert!(mould_rows(&walls, &contour(), 480).is_empty());
}

#[test]
fn test_short_fragment_is_dropped() {
    let walls = vec![note(60, 0.0, 100.0), note(60, 105.0, 100.0)];
    assert!(
        mould_rows(&walls, &contour(), 480).is_empty(),
        "5 tick 小于 100 的 25%"
    );
}
