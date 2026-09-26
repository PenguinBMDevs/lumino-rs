//! 轨道有序不变式回归：undo/redo MoveOp 回放必须保持「按 start_tick 升序」
//!
//! 背景：`apply_move_ops` 为性能**就地**改 tick（`get_mut`），移动跨过其它音符
//! 后列表失序——`partition_point` / `window_range` 二分查询会漏检音符
//! （渲染可见性、命中检测、框选失效）。

use super::make_data_with_notes;
use crate::EditorData;
use lumino_midi_model::NoteEvent;
use lumino_note_core::history::MoveOp;
use lumino_note_core::note::Note;

/// 轨 1 的 start_tick 序列
fn ticks(data: &EditorData) -> Vec<u32> {
    data.track_notes(1).iter().map(|n| n.start_tick).collect()
}

/// 轨 1 按索引取原始值快照（按值引用）
fn value_of(data: &EditorData, index: usize) -> NoteEvent {
    *data.track_notes(1).get(index).expect("音符应存在")
}

/// 按生产侧同一逻辑计算移动后快照（original + delta，clamp + 长度不变）。
fn shifted_one(orig: &NoteEvent, delta_tick: i32, delta_key: i16, max_key: u16) -> NoteEvent {
    let mut m = *orig;
    let new_tick = (orig.start_tick as i64 + delta_tick as i64).max(0) as u32;
    let new_key = (orig.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u8;
    let len = orig.end_tick.saturating_sub(orig.start_tick).max(1);
    m.start_tick = new_tick;
    m.end_tick = new_tick.saturating_add(len);
    m.key = new_key;
    m
}

#[test]
fn test_apply_move_ops_forward_restores_sorted_order() {
    // 音符 tick 0/10/20；把首个音符移到 30（越过其余两个）→ 必须重排
    let mut data = make_data_with_notes();
    let orig = value_of(&data, 0);
    let ops = vec![MoveOp {
        track_id: 1,
        moved: vec![shifted_one(&orig, 30, 0, 127)],
        originals: vec![orig],
        delta_tick: 30,
        delta_key: 0,
        seq: 0,
    }];
    assert_eq!(data.apply_move_ops(&ops, false, 127), 1);
    assert_eq!(ticks(&data), vec![10, 20, 30], "回放后必须保持升序不变式");
    // 二分查询必须覆盖全部音符（渲染/命中依赖）
    assert_eq!(data.track_notes(1).partition_point(21), 2);
    // 删加语义：索引整体位移，旧区间事件失效 → 清空事件 + 单轨 TrackDelta 重建，
    // 不得触发全量会话兜底（note_delta_dirty=false，主轨结构 dirty=true）。
    assert!(!data.note_delta_dirty, "删加后不得触发全量重建");
    assert!(
        data.note_delta_events.is_empty(),
        "删加后旧索引事件已失效，应清空（按受影响轨 TrackDelta 重建）"
    );
}

#[test]
fn test_apply_move_ops_inverse_restores_sorted_order() {
    let mut data = make_data_with_notes();
    // 前进：0 → 30（失序来源）
    let orig = value_of(&data, 0);
    let fwd = vec![MoveOp {
        track_id: 1,
        moved: vec![shifted_one(&orig, 30, 0, 127)],
        originals: vec![orig],
        delta_tick: 30,
        delta_key: 0,
        seq: 0,
    }];
    assert_eq!(data.apply_move_ops(&fwd, false, 127), 1);
    assert_eq!(ticks(&data), vec![10, 20, 30]);

    // 反向（undo）：同一前向 op + inverse 标志，30 → 0，必须恢复升序 0/10/20
    assert_eq!(data.apply_move_ops(&fwd, true, 127), 1);
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
    let orig: NoteEvent = *data.track_notes(2).get(0).expect("音符应存在");
    let ops = vec![MoveOp {
        track_id: 2,
        moved: vec![shifted_one(&orig, 100, 0, 127)],
        originals: vec![orig],
        delta_tick: 100,
        delta_key: 0,
        seq: 0,
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
