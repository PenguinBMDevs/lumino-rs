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

#[test]
fn test_mould_pieces_split_dough_into_inner_and_outer() {
    let walls = vec![note(60, 0.0, 100.0), note(60, 200.0, 100.0)];
    let dough = vec![note(60, 0.0, 300.0)];
    let pieces = mould_pieces(&walls, &dough, &contour(), 480);
    assert_eq!(pieces.len(), 2, "应有外片 + 一个内片");
    assert!(!pieces[0].inner);
    assert!(pieces[1].inner);
    assert_eq!(
        pieces[0]
            .notes
            .iter()
            .map(|n| (n.tick, n.length))
            .collect::<Vec<_>>(),
        vec![(0.0, 100.0), (200.0, 100.0)],
        "外片 = 面团减去空隙"
    );
    assert_eq!(
        pieces[1]
            .notes
            .iter()
            .map(|n| (n.tick, n.length))
            .collect::<Vec<_>>(),
        vec![(100.0, 100.0)],
        "内片 = 面团落在空隙里的部分"
    );
    assert!(!pieces[1].ring.is_empty(), "内片应有闭合阶梯环");
}

#[test]
fn test_mould_pieces_keeps_other_rows_whole() {
    let walls = vec![note(60, 0.0, 100.0), note(60, 200.0, 100.0)];
    // key 61 没有墙：整行归外片；key 60 的面团按空隙裁开
    let dough = vec![note(60, 0.0, 300.0), note(61, 0.0, 300.0)];
    let pieces = mould_pieces(&walls, &dough, &contour(), 480);
    let outer = &pieces[0].notes;
    assert!(
        outer
            .iter()
            .any(|n| n.key == 61 && (n.length - 300.0).abs() < 1e-3),
        "无墙的行应整行保留在外片"
    );
}

#[test]
fn test_mould_pieces_empty_without_gap() {
    let walls = vec![note(60, 0.0, 100.0)];
    let dough = vec![note(60, 0.0, 100.0)];
    assert!(mould_pieces(&walls, &dough, &contour(), 480).is_empty());
}

#[test]
fn test_notes_within_rows_drops_short_fragment() {
    let dough = vec![note(60, 0.0, 100.0)];
    // 只有 5 tick 落在空隙内（< 100 的 25%）
    assert!(notes_within_rows(&dough, &[(60, 95.0, 110.0)]).is_empty());
}

#[test]
fn test_piece_rings_enclose_note_rows() {
    // 音符第 key 行占几何 [key-1, key]：片轮廓必须包住它的所有音符行
    let walls = vec![note(60, 0.0, 100.0), note(60, 200.0, 100.0)];
    let dough = vec![
        note(59, 50.0, 300.0),
        note(60, 50.0, 300.0),
        note(61, 50.0, 300.0),
    ];
    let pieces = mould_pieces(&walls, &dough, &contour(), 480);
    assert!(!pieces.is_empty());
    for piece in &pieces {
        if piece.notes.is_empty() || piece.ring.is_empty() {
            continue;
        }
        let ring_min = piece.ring.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let ring_max = piece
            .ring
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let note_min = piece.notes.iter().map(|n| n.key).min().expect("有音符") as f32;
        let note_max = piece.notes.iter().map(|n| n.key).max().expect("有音符") as f32;
        assert!(
            ring_min <= note_min - 1.0 + 1e-3,
            "轮廓底边 {ring_min} 未包住最下一行音符 key {note_min}"
        );
        assert!(
            ring_max >= note_max - 1e-3,
            "轮廓顶边 {ring_max} 未包住最上一行音符 key {note_max}"
        );
    }
}

#[test]
fn test_band_ring_is_axis_aligned_staircase() {
    let rows = vec![(60, 100.0, 200.0), (61, 120.0, 200.0), (62, 120.0, 180.0)];
    let ring = band_ring(&rows);
    assert!(ring.len() >= 5, "阶梯环至少要有 4 条边");
    for pair in ring.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        assert!(
            (a.0 - b.0).abs() < 1e-4 || (a.1 - b.1).abs() < 1e-4,
            "阶梯环的每条边都必须轴对齐：{a:?} -> {b:?}"
        );
    }
    assert_eq!(ring.first(), ring.last(), "环必须闭合");
    let min_x = ring.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = ring.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let min_y = ring.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = ring.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    // 音符第 key 行占几何 [key-1, key]：行 60..62 → 环 59..62
    assert_eq!((min_x, max_x, min_y, max_y), (100.0, 200.0, 59.0, 62.0));
}
