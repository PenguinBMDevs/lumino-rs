//! 轨道有序不变式回归：就地改 tick 的写路径必须保持「按 start_tick 升序」
//!
//! 背景：拖动流式应用 / 量化等路径为性能**就地**改 tick（`get_mut`），
//! 移动跨过其它音符后列表失序——`partition_point` / `window_range` 二分
//! 查询会漏检音符（渲染可见性、命中检测、框选失效）。

use crate::DragState;
use crate::EditorData;
use crate::EditorTransform;
use bit_vec::BitVec;
use lumino_note_core::note::Note;

fn make_data() -> EditorData {
    EditorData::with_f32_notes(
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
        ],
    )
}

fn ticks(data: &EditorData) -> Vec<u32> {
    data.track_notes(1).iter().map(|n| n.start_tick).collect()
}

#[test]
fn test_streaming_drag_reorder_restores_sorted_order() {
    let mut data = make_data();
    let mut bv = BitVec::from_elem(3, false);
    bv.set(0, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(30, 0); // tick 0 → 30（越过 10/20）

    assert_eq!(data.apply_drag_state_streaming(&ds, 127), 1);
    assert_eq!(
        ticks(&data),
        vec![10, 20, 30],
        "流式拖动后必须保持升序不变式"
    );
    // 重排走受影响闭区间增量更新（替代全量重建）：不置 dirty，事件为区间更新
    assert!(
        !data.note_delta_dirty,
        "重排必须增量更新（受影响区间），不得触发全量重建"
    );
    assert_eq!(
        data.note_delta_events.len(),
        1,
        "重排应发出 1 条 UpdateRange 事件"
    );
    assert_eq!(
        data.track_notes(1).partition_point(21),
        2,
        "二分查询必须覆盖全部音符（渲染/命中依赖）"
    );
}

#[test]
fn test_streaming_drag_no_reorder_keeps_range_events() {
    let mut data = make_data();
    let mut bv = BitVec::from_elem(3, false);
    bv.set(1, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(0, 5); // 仅改 key，tick 不变

    assert_eq!(data.apply_drag_state_streaming(&ds, 127), 1);
    assert_eq!(ticks(&data), vec![0, 10, 20], "tick 未变，顺序不变");
    assert!(!data.note_delta_dirty, "无重排应走区间事件（非全量重建）");
    assert!(
        !data.note_delta_events.is_empty(),
        "应记录 UpdateRange 事件"
    );
}

#[test]
fn test_restore_current_track_sorted_incremental_after_inplace_tick_change() {
    // 模拟量化子集：只改第 3 个音符（tick 20 → 1，越过 0/10）
    let mut data = make_data();
    {
        let track = data
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(1))
            .expect("轨道应存在");
        track.get_mut(2).expect("音符应存在").start_tick = 1;
    }
    assert!(
        data.restore_current_track_sorted_incremental(&[2]),
        "失序应重排"
    );
    assert_eq!(ticks(&data), vec![0, 1, 10]);
    // 重排 → 受影响闭区间增量更新（不触发全量重建）
    assert!(!data.note_delta_dirty, "增量更新，不置全量 dirty");
    assert_eq!(data.note_delta_events.len(), 1, "应发出 1 条区间更新事件");
    assert!(
        !data.restore_current_track_sorted_incremental(&[2]),
        "已有序应返回 false（零重排）"
    );
}

// ── 同类路径泛化：其余「就地改 tick」写路径同样必须恢复升序 ──

/// 重排后不触发全量重建 + 事件负载为受影响区间（而非整轨）的通用断言
fn assert_incremental_reorder(data: &EditorData, expected_ticks: Vec<u32>, ctx: &str) {
    assert_eq!(ticks(data), expected_ticks, "{ctx}: 必须恢复升序");
    assert!(
        !data.note_delta_dirty,
        "{ctx}: 重排必须走增量区间，不得全量重建"
    );
    assert_eq!(
        data.note_delta_events.len(),
        1,
        "{ctx}: 应发出 1 条区间更新事件"
    );
}

#[test]
fn test_flip_horizontal_restores_track_order() {
    // 水平翻转围绕轴镜像 → 时间顺序整体反转（就地改 tick）
    let mut data = make_data(); // [0, 10, 20]，长度 1
    let selected: crate::SelectionSet = [0usize, 2].into_iter().collect();
    // 轴 10：note0 (0,1) → 19；note2 (20,21) → -1 → 0
    assert_eq!(data.flip_horizontal(&selected, 10.0), 2);
    assert_incremental_reorder(&data, vec![0, 10, 19], "水平翻转");
}

#[test]
fn test_speed_change_subset_restores_track_order() {
    let mut data = make_data(); // [0, 10, 20]
    let selected: crate::SelectionSet = [0usize, 2].into_iter().collect();
    // min=0；factor 0.25 → note2: 20 → 5（越过未选中的 10）
    assert_eq!(data.apply_speed_change(&selected, 0.25), 1);
    assert_incremental_reorder(&data, vec![0, 5, 10], "子集变速");
}

#[test]
fn test_batch_edit_tick_restores_track_order() {
    let mut data = make_data(); // [0, 10, 20]
    let selected: crate::SelectionSet = [2usize].into_iter().collect();
    // tick 表达式 "/4"：20 → 5（越过未选中的 10）
    assert_eq!(data.apply_batch_edit(&selected, "", "", "", "/4", 127), 1);
    assert_incremental_reorder(&data, vec![0, 5, 10], "批量编辑 tick 表达式");
}

// ── 增量更新契约（用户验收：重排不得全量重建） ──

/// 区间事件负载音符总数
fn range_payload_len(data: &EditorData) -> usize {
    data.note_delta_events
        .iter()
        .map(|e| match e {
            crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange { notes, .. } => {
                notes.len()
            }
            _ => 0,
        })
        .sum()
}

#[test]
fn test_reorder_payload_is_span_sized_not_whole_track() {
    // 500 音符工程：拖动 index 0 越过 3 个邻居
    let notes: Vec<Note> = (0..500)
        .map(|i| Note::new(i as f32 * 10.0, 60, 1.0))
        .collect();
    let mut data = EditorData::with_f32_notes(1, &notes);
    let mut bv = BitVec::from_elem(500, false);
    bv.set(0, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(35, 0); // 0 → 35（越过 10/20/30）

    assert_eq!(data.apply_drag_state_streaming(&ds, 127), 1);
    assert!(!data.note_delta_dirty, "重排不得触发全量重建");
    let payload = range_payload_len(&data);
    assert!(
        payload <= 8,
        "区间事件负载必须为 O(span)（实际 {payload}），而非整轨 500 —— 全量重建不可接受"
    );
}

#[test]
fn test_reorder_in_large_track_payload_stays_bounded() {
    // 5000 音符：拖动末位音符到最前（跨越整轨的极端场景）
    let notes: Vec<Note> = (0..5000)
        .map(|i| Note::new(i as f32 * 10.0, 60, 1.0))
        .collect();
    let mut data = EditorData::with_f32_notes(1, &notes);
    let mut bv = BitVec::from_elem(5000, false);
    bv.set(4999, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(-49990, 0); // 49990 → 0（跨整轨）

    assert_eq!(data.apply_drag_state_streaming(&ds, 127), 1);
    assert!(!data.note_delta_dirty, "跨整轨重排也不得全量重建");
    let payload = range_payload_len(&data);
    // 跨整轨是有界的最坏情况（区间 = 整轨），但仍是**当前轨**区间而非全工程重传
    assert!(payload <= 5000, "负载不得超过当前轨音符数");
    assert_eq!(
        data.track_notes(1).partition_point(49990),
        5000,
        "二分查询必须覆盖全部音符（渲染/命中依赖）"
    );
}

#[test]
fn test_reorder_scattered_selection_payload_stays_sparse() {
    // 2000 音符；选中每隔 100 个（20 个分散音符），整体 +15 tick 越过各自邻居
    // → 区间集合必须稀疏（不得退化为横跨整轨的单凸包）
    let notes: Vec<Note> = (0..2000)
        .map(|i| Note::new(i as f32 * 10.0, 60, 1.0))
        .collect();
    let mut data = EditorData::with_f32_notes(1, &notes);
    let mut bv = BitVec::from_elem(2000, false);
    let mut selected_count = 0usize;
    for i in (0..2000).step_by(100) {
        bv.set(i, true);
        selected_count += 1;
    }
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(15, 0);

    assert_eq!(
        data.apply_drag_state_streaming(&ds, 127),
        selected_count,
        "全部选中音符应被移动"
    );
    assert!(!data.note_delta_dirty, "不得全量重建");
    let payload = range_payload_len(&data);
    assert!(
        payload * 4 < 2000,
        "分散选区负载必须远小于整轨（实际 {payload}）——稀疏区间不得退化为凸包"
    );
    assert!(
        data.note_delta_events.len() >= 5,
        "分散改动应产生多个区间事件（实际 {}）",
        data.note_delta_events.len()
    );
}
