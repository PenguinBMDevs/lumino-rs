//! 任意方向切割测试（DRAW-025）

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

#[test]
fn test_vertical_line_matches_legacy_vertical_cut() {
    let points = vec![(0.0, 60.0), (200.0, 60.0)];
    let (positive, negative) =
        cut_open_line(&points, (100.0, 0.0), (100.0, 1.0)).expect("竖线应能切开开放折线");
    assert_eq!(positive, vec![(0.0, 60.0), (100.0, 60.0)]);
    assert_eq!(negative, vec![(100.0, 60.0), (200.0, 60.0)]);
}

#[test]
fn test_diagonal_cut_splits_at_intersection() {
    let points = vec![(0.0, 100.0), (300.0, 100.0)];
    // y = x，与 y=100 的交点为 (100, 100)
    let (positive, negative) =
        cut_open_line(&points, (0.0, 0.0), (200.0, 200.0)).expect("斜线应能切开");
    assert_eq!(positive, vec![(0.0, 100.0), (100.0, 100.0)]);
    assert_eq!(negative, vec![(100.0, 100.0), (300.0, 100.0)]);
}

#[test]
fn test_drag_direction_does_not_change_order() {
    let points = vec![(0.0, 100.0), (300.0, 100.0)];
    let forward = cut_open_line(&points, (0.0, 0.0), (200.0, 200.0)).expect("正向");
    let backward = cut_open_line(&points, (200.0, 200.0), (0.0, 0.0)).expect("反向");
    assert_eq!(forward, backward, "拖拽方向不应影响两半的顺序");
}

#[test]
fn test_notes_split_at_row_crossing() {
    let notes = vec![note(60, 0.0, 200.0)];
    // y = x 在 key 60 的行中心（60.5）处 x = 60.5
    let (positive, negative) = split_notes_line(&notes, (0.0, 0.0), (200.0, 200.0));
    assert_eq!(positive.len(), 1, "正侧应有一段");
    assert_eq!(negative.len(), 1, "负侧应有一段");
    assert!((positive[0].length - 60.5).abs() < 1e-3);
    assert!((negative[0].tick - 60.5).abs() < 1e-3);
    assert!((positive[0].length + negative[0].length - 200.0).abs() < 1e-3);
}

#[test]
fn test_notes_on_opposite_rows_go_to_opposite_sides() {
    let notes = vec![note(70, 0.0, 100.0), note(50, 0.0, 100.0)];
    // 水平线 y=60：整行归属，不切分
    let (positive, negative) = split_notes_line(&notes, (0.0, 60.0), (200.0, 60.0));
    assert_eq!(positive.len(), 1);
    assert_eq!(positive[0].key, 70, "上方行应落在正侧");
    assert_eq!(negative.len(), 1);
    assert_eq!(negative[0].key, 50, "下方行应落在负侧");
    assert!((positive[0].length - 100.0).abs() < 1e-3, "水平切不分音符");
}

#[test]
fn test_vertical_notes_split_matches_legacy() {
    let notes = vec![
        note(60, 0.0, 100.0),
        note(61, 50.0, 100.0),
        note(62, 200.0, 100.0),
    ];
    let (positive, negative) = split_notes_line(&notes, (100.0, 0.0), (100.0, 1.0));
    assert_eq!(positive.len(), 2, "左侧：整段 + 跨切线的左半");
    assert_eq!(negative.len(), 2, "右侧：跨切线的右半 + 整段");
    assert_eq!(positive[1].tick, 50.0);
    assert!((positive[1].length - 50.0).abs() < 1e-3);
    assert!((negative[0].tick - 100.0).abs() < 1e-3);
}

#[test]
fn test_extend_line_covers_bounds() {
    let (a, b) =
        extend_line((50.0, 0.0), (50.0, 1.0), (0.0, 0.0, 100.0, 60.0)).expect("竖线应能延长");
    assert!(a.1 <= 0.0 && b.1 >= 60.0, "延长后应覆盖外接框：{a:?} {b:?}");
    assert!((a.0 - 50.0).abs() < 1e-3 && (b.0 - 50.0).abs() < 1e-3);
}

#[test]
fn test_extend_line_rejects_zero_length() {
    assert!(extend_line((10.0, 10.0), (10.0, 10.0), (0.0, 0.0, 100.0, 60.0)).is_none());
}
