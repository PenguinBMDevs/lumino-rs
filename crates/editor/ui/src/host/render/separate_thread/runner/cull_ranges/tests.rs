//! 可见区间 cull 计划单元测试

use super::{compute_cull_plan, compute_cull_plan_inner, should_use_ranges};
use lumino_midi_loader::{MidiDocument, NoteEvent};

fn note(start: u32, len: u32) -> NoteEvent {
    NoteEvent::new(start, start + len, 60, 100, 0)
}

fn doc_with(notes: &[NoteEvent]) -> MidiDocument {
    let mut doc = MidiDocument::empty_with_tracks(1, 480);
    for n in notes {
        assert!(doc.insert_note(0, *n), "测试音符插入失败");
    }
    doc
}

/// 相机映射到 tick 窗口：scroll/zoom/viewport → [left, right]
fn plan_for(
    doc: &MidiDocument,
    scroll_x: f32,
    zoom_x: f32,
    viewport_w: f32,
) -> (Vec<(u32, u32)>, usize) {
    // 小样本单测绕过 dispatch 规模门槛，专注区间几何正确性
    let plan = compute_cull_plan_inner(Some(doc), scroll_x, zoom_x, viewport_w, 0.0, 0.0, false);
    (
        plan.ranges.iter().map(|r| (r.start, r.count)).collect(),
        plan.total_instances,
    )
}

#[test]
fn test_no_document_or_empty_returns_empty_plan() {
    let plan = compute_cull_plan(None, 0.0, 1.0, 100.0, 0.0, 0.0);
    assert!(plan.ranges.is_empty());
    assert_eq!(plan.total_instances, 0);

    let doc = MidiDocument::empty_with_tracks(2, 480);
    let plan = compute_cull_plan(Some(&doc), 0.0, 1.0, 100.0, 0.0, 0.0);
    assert!(plan.ranges.is_empty());
    assert_eq!(plan.total_instances, 0);
}

#[test]
fn test_full_view_merges_to_single_range() {
    let doc = doc_with(&[note(0, 100), note(1000, 100), note(2000, 100)]);
    let (ranges, total) = plan_for(&doc, 0.0, 1.0, 10_000.0);
    assert_eq!(total, 3);
    assert_eq!(
        ranges,
        vec![(0, 3)],
        "全景应合并为单区间（少量间隙合并，减少 dispatch）"
    );
}

#[test]
fn test_middle_window_only_includes_candidates() {
    let doc = doc_with(&[note(0, 100), note(1000, 100), note(2000, 100)]);
    // tick 窗口 [900, 1100]：只有中间音符的 start 落在窗口内；
    // 前一个音符 end=100 < 900，不可能可见。
    let (ranges, total) = plan_for(&doc, 900.0, 1.0, 200.0);
    assert_eq!(total, 3);
    assert_eq!(ranges, vec![(1, 1)]);
}

#[test]
fn test_long_note_starting_before_window_is_kept() {
    // 0 号音符长 5000 tick，窗口 [900,1100] 内仍可见；
    // max_len=5000 使 first_tick = 0，不能把它裁掉。
    let doc = doc_with(&[note(0, 5000), note(1000, 10)]);
    let (ranges, total) = plan_for(&doc, 900.0, 1.0, 200.0);
    assert_eq!(total, 2);
    assert_eq!(ranges, vec![(0, 2)], "长音符必须包含在候选区间内");
}

#[test]
fn test_dirty_max_len_falls_back_to_full_track() {
    let mut doc = doc_with(&[note(0, 10), note(5000, 10)]);
    // 可变引用置脏：max_len 未知 → 整轨候选（宁可多扫，不丢可见音符）
    let _ = doc.track_notes_mut(0).expect("音轨可变引用");
    let (ranges, total) = plan_for(&doc, 4900.0, 1.0, 200.0);
    assert_eq!(total, 2);
    assert_eq!(ranges, vec![(0, 2)]);
}

#[test]
fn test_track_offsets_accumulate_across_tracks() {
    let mut doc = MidiDocument::empty_with_tracks(2, 480);
    doc.insert_note(0, note(0, 10));
    doc.insert_note(0, note(100, 10));
    doc.insert_note(1, note(0, 10));
    doc.insert_note(1, note(3000, 10));

    let plan = compute_cull_plan_inner(Some(&doc), 2900.0, 1.0, 200.0, 0.0, 0.0, false);
    assert_eq!(plan.total_instances, 4);
    // 轨 0 无候选；轨 1 偏移 = 2，第二个音符（start=3000）索引 = 3
    assert_eq!(
        plan.ranges
            .iter()
            .map(|r| (r.start, r.count))
            .collect::<Vec<_>>(),
        vec![(3, 1)]
    );
}

#[test]
fn test_should_use_ranges_avoids_small_n_dispatch_overhead() {
    // 2M 音符 + 43 区间：43×10 万 = 430 万 > 200 万 → 回退（实测负优化）
    assert!(!should_use_ranges(2_000_000, 43));
    // 8M 音符 + 43 区间：430 万 < 800 万 → 启用（实测 1.7x）
    assert!(should_use_ranges(8_000_000, 43));
    // 单区间总是值得（全景合并）
    assert!(should_use_ranges(2_000_000, 1));
    // 空区间/超上限一律回退
    assert!(!should_use_ranges(8_000_000, 0));
    assert!(!should_use_ranges(200_000_000, 1025));
    // 4M + 43：430 万 > 400 万 → 仍回退，等 8M 档再启用
    assert!(!should_use_ranges(4_000_000, 43));
}

#[test]
fn test_degenerate_zoom_returns_empty_plan() {
    let doc = doc_with(&[note(0, 10)]);
    for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let plan = compute_cull_plan(Some(&doc), 0.0, zoom, 100.0, 0.0, 0.0);
        assert!(plan.ranges.is_empty(), "zoom={zoom} 必须回退整段 cull");
        assert_eq!(plan.total_instances, 1);
    }
}
