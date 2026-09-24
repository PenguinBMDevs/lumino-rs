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
    assert!(data.note_delta_dirty, "重排 → 主轨全量重建");
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
fn test_restore_current_track_sorted_after_inplace_tick_change() {
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
    assert!(data.restore_current_track_sorted(&[2]), "失序应重排");
    assert_eq!(ticks(&data), vec![0, 1, 10]);
    assert!(
        !data.restore_current_track_sorted(&[2]),
        "已有序应返回 false（零重排）"
    );
}

// ── 同类路径泛化：其余「就地改 tick」写路径同样必须恢复升序 ──

#[test]
fn test_flip_horizontal_restores_track_order() {
    // 水平翻转围绕轴镜像 → 时间顺序整体反转（就地改 tick）
    let mut data = make_data(); // [0, 10, 20]，长度 1
    let selected: std::collections::HashSet<usize> = [0usize, 2].into_iter().collect();
    // 轴 10：note0 (0,1) → 19；note2 (20,21) → -1 → 0
    assert_eq!(data.flip_horizontal(&selected, 10.0), 2);
    assert_eq!(ticks(&data), vec![0, 10, 19], "翻转后必须恢复升序不变式");
    assert!(data.note_delta_dirty, "重排 → 主轨全量重建");
}

#[test]
fn test_speed_change_subset_restores_track_order() {
    let mut data = make_data(); // [0, 10, 20]
    let selected: std::collections::HashSet<usize> = [0usize, 2].into_iter().collect();
    // min=0；factor 0.25 → note2: 20 → 5（越过未选中的 10）
    assert_eq!(data.apply_speed_change(&selected, 0.25), 1);
    assert_eq!(ticks(&data), vec![0, 5, 10], "变速越过后必须恢复升序");
    assert!(data.note_delta_dirty, "重排 → 主轨全量重建");
}

#[test]
fn test_batch_edit_tick_restores_track_order() {
    let mut data = make_data(); // [0, 10, 20]
    let selected: std::collections::HashSet<usize> = [2usize].into_iter().collect();
    // tick 表达式 "/4"：20 → 5（越过未选中的 10）
    assert_eq!(data.apply_batch_edit(&selected, "", "", "", "/4", 127), 1);
    assert_eq!(
        ticks(&data),
        vec![0, 5, 10],
        "tick 表达式越过后必须恢复升序"
    );
    assert!(data.note_delta_dirty, "重排 → 主轨全量重建");
}
