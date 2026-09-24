//! 轨道有序不变式回归：undo/redo MoveOp 回放必须保持「按 start_tick 升序」
//!
//! 背景：`apply_move_ops` 为性能**就地**改 tick（`get_mut`），移动跨过其它音符
//! 后列表失序——`partition_point` / `window_range` 二分查询会漏检音符
//! （渲染可见性、命中检测、框选失效）。

use super::make_data_with_notes;
use crate::EditorData;
use lumino_note_core::history::MoveOp;
use lumino_note_core::note::Note;

/// 轨 1 的 start_tick 序列
fn ticks(data: &EditorData) -> Vec<u32> {
    data.track_notes(1).iter().map(|n| n.start_tick).collect()
}

fn id_of(data: &EditorData, index: usize) -> u64 {
    data.track_notes(1).get(index).expect("音符应存在").id
}

#[test]
fn test_apply_move_ops_forward_restores_sorted_order() {
    // 音符 tick 0/10/20；把首个音符移到 30（越过其余两个）→ 必须重排
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        ids: vec![id_of(&data, 0)],
        delta_tick: 30,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![0.0],
        original_keys: vec![60],
    }];
    assert_eq!(data.apply_move_ops(&ops, false, 127), 1);
    assert_eq!(ticks(&data), vec![10, 20, 30], "回放后必须保持升序不变式");
    // 二分查询必须覆盖全部音符（渲染/命中依赖）
    assert_eq!(data.track_notes(1).partition_point(21), 2);
    assert!(
        data.note_delta_dirty,
        "重排轨的区间事件按旧索引失效 → 必须走主轨全量重建"
    );
}

#[test]
fn test_apply_move_ops_inverse_restores_sorted_order() {
    let mut data = make_data_with_notes();
    let id0 = id_of(&data, 0);
    // 前进：0 → 30（失序来源）
    let fwd = vec![MoveOp {
        track_id: 1,
        ids: vec![id0],
        delta_tick: 30,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![0.0],
        original_keys: vec![60],
    }];
    assert_eq!(data.apply_move_ops(&fwd, false, 127), 1);
    assert_eq!(ticks(&data), vec![10, 20, 30]);

    // 反向（undo）：30 → 0，必须恢复升序 0/10/20
    let inv = vec![MoveOp {
        track_id: 1,
        ids: vec![id0],
        delta_tick: -30,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![0.0],
        original_keys: vec![60],
    }];
    assert_eq!(data.apply_move_ops(&inv, true, 127), 1);
    assert_eq!(ticks(&data), vec![0, 10, 20], "undo 回放后必须恢复升序");
}

#[test]
fn test_apply_move_ops_non_current_track_emits_no_main_events() {
    // 非当前轨的 undo/redo 不得推入主轨段内事件（事件队列无 track 维度，
    // 会被误应用到当前轨段 → 错误音符）
    let mut data = make_data_with_notes(); // 当前轨 = 1
    data.ensure_track(2);
    data.insert_note(2, Note::new(0.0, 60, 1.0));
    data.insert_note(2, Note::new(10.0, 62, 1.0));
    let ops = vec![MoveOp {
        track_id: 2,
        ids: vec![data.track_notes(2).get(0).expect("音符应存在").id],
        delta_tick: 100,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![0.0],
        original_keys: vec![60],
    }];
    data.note_delta_events.clear();
    assert_eq!(data.apply_move_ops(&ops, false, 127), 1);
    assert!(
        data.note_delta_events.is_empty(),
        "非当前轨修改不得产生主轨段内事件"
    );
    assert_eq!(
        data.track_notes(2)
            .iter()
            .map(|n| n.start_tick)
            .collect::<Vec<_>>(),
        vec![10, 100],
        "非当前轨仍必须保持升序"
    );
}
