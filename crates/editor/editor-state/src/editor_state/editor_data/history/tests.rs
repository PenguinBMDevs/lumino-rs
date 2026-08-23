//! EditorData 历史记录与 MoveOp 单元测试
//!
//! 覆盖：
//! - `apply_move_ops` 正向/反向应用
//! - key clamp、跨轨操作（document 轨道构造时固定）
//! - `move_ops_from_drag_state` 连续区间拆分、delta 饱和
//! - 基于 MoveOp 的 undo/redo 往返

use super::*;
use crate::DragState;
use bit_vec::BitVec;
use lumino_midi_model::NoteEvent;
use lumino_note_core::history::CreateOp;
use lumino_note_core::note::Note;

fn make_data_with_notes() -> EditorData {
    EditorData::with_f32_notes(
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
        ],
    )
}

#[test]
fn test_apply_move_ops_forward() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 2,
        delta_tick: 5,
        delta_key: -2,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    let modified = data.apply_move_ops(&ops, false, 127);
    assert_eq!(modified, 2);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        5.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        58
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        15.0
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        20.0,
        "未在范围内的音符不变"
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );

    // document 同步更新（唯一权威源）
    let track = data.track_notes(1);
    assert_eq!(track[0].start_tick as f32, 5.0);
    assert_eq!(track[1].start_tick as f32, 15.0);
}

#[test]
fn test_apply_move_ops_inverse() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 3,
        delta_tick: 10,
        delta_key: 5,
        seq: 0,
        original_ticks: vec![0.0, 10.0, 20.0],
        original_keys: vec![60, 62, 64],
    }];
    // 先 forward
    data.apply_move_ops(&ops, false, 127);
    // 再 inverse 应还原
    let modified = data.apply_move_ops(&ops, true, 127);
    assert_eq!(modified, 3);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        0.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        10.0
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        62
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        20.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );
}

#[test]
fn test_apply_move_ops_clamps_key() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        range_start: 0,
        range_end: 1,
        delta_tick: 0,
        delta_key: -100,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops, false, 20);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        0,
        "key 应 clamp 到 0"
    );

    let ops2 = vec![MoveOp {
        track_id: 1,
        range_start: 1,
        range_end: 2,
        delta_tick: 0,
        delta_key: 100,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops2, false, 20);
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        20,
        "key 应 clamp 到 max_key"
    );
}

#[test]
fn test_apply_move_ops_creates_missing_track_notes() {
    // 语义替代（2026-08）：apply_move_ops 不再自动创建缺失音轨（document 轨道
    // 构造时固定）。原测试意图「操作指定轨数据」改为：构造含 track 2 的 document，
    // 验证 apply_move_ops 可作用于非当前轨（track_id=2）。
    let mut data = EditorData::with_f32_notes(2, &[Note::new(0.0, 60, 1.0)]);
    let ops = vec![MoveOp {
        track_id: 2,
        range_start: 0,
        range_end: 1,
        delta_tick: 3,
        delta_key: 1,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    data.apply_move_ops(&ops, false, 127);
    let track = data.track_notes(2);
    assert_eq!(track[0].start_tick as f32, 3.0);
    assert_eq!(track[0].key as u16, 61);
}

#[test]
fn test_move_ops_from_drag_state_splits_ranges() {
    let data = EditorData::with_f32_notes(
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
            Note::new(30.0, 66, 1.0),
        ],
    );

    let mut bv = BitVec::from_elem(4, false);
    bv.set(0, true);
    bv.set(1, true);
    bv.set(3, true);
    let mut drag_state = DragState::new(bv, 0, 60);
    drag_state.set_delta(5, -2);

    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops.len(), 2, "应拆分为两个连续段");
    assert_eq!(ops[0].range_start, 0);
    assert_eq!(ops[0].range_end, 2);
    assert_eq!(ops[0].delta_tick, 5);
    assert_eq!(ops[0].delta_key, -2);
    assert_eq!(ops[0].seq, 0);

    assert_eq!(ops[1].range_start, 3);
    assert_eq!(ops[1].range_end, 4);
    assert_eq!(ops[1].delta_tick, 5);
    assert_eq!(ops[1].delta_key, -2);
    assert_eq!(ops[1].seq, 1);
}

#[test]
fn test_move_ops_from_drag_state_saturates_delta_tick() {
    let data = EditorData::with_f32_notes(1, &[Note::new(0.0, 60, 1.0)]);

    let mut drag_state = DragState::from_single(0, data.current_track_note_count(), 0, 60);
    drag_state.set_delta(i64::MAX, 0);

    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops[0].delta_tick, i32::MAX, "delta_tick 应饱和到 i32::MAX");

    drag_state.set_delta(i64::MIN, 0);
    let ops = data.move_ops_from_drag_state(&drag_state);
    assert_eq!(ops[0].delta_tick, i32::MIN, "delta_tick 应饱和到 i32::MIN");
}

#[test]
fn test_undo_redo_with_move_op_entry() {
    let mut data = make_data_with_notes();
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2);
        ds
    });
    data.apply_move_ops(&ops, false, 127);
    data.push_move_op(ops);

    // undo 应还原
    assert!(data.undo());
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        0.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        20.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );

    // redo 应再次应用
    assert!(data.redo());
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        5.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        58
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        25.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        62
    );
}

/// Bug 2 回归：A 端对 MoveOp 执行 undo/redo 后，必须把「反向/正向移动」记入
/// `pending_collab_move_sync`，供 ui-editor 层广播给协作对端。若缺失，B 端在 A 撤销后
/// 本地音符坐标与 A 端失同步（用户日志中的「匹配 0/1」「B 无法正确响应」）。
#[test]
fn test_undo_redo_populates_collab_move_sync() {
    let mut data = make_data_with_notes();
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2); // delta_tick=5, delta_key=-2
        ds
    });
    data.apply_move_ops(&ops, false, 127);
    data.push_move_op(ops);

    // 移动/提交阶段不应累积协作同步记录
    assert!(
        data.take_pending_collab_move_sync().is_empty(),
        "提交阶段不应填充待广播队列"
    );

    // ── undo（inverse=true）──
    assert!(data.undo());
    let mut pending = data.take_pending_collab_move_sync();
    assert_eq!(pending.len(), 2, "被移动的两个音符应各有一条同步记录");
    pending.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    // 音符0：original_tick=0, original_key=60, delta_tick=5, delta_key=-2
    //  ref = original + delta，offset = -delta
    assert_eq!(pending[0], (5.0, 58, -5.0, 2, 1));
    // 音符2：original_tick=20, original_key=64
    assert_eq!(pending[1], (25.0, 62, -5.0, 2, 1));

    // ── redo（inverse=false）──
    assert!(data.redo());
    let mut pending2 = data.take_pending_collab_move_sync();
    assert_eq!(pending2.len(), 2);
    pending2.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    // ref = original，offset = +delta
    assert_eq!(pending2[0], (0.0, 60, 5.0, -2, 1));
    assert_eq!(pending2[1], (20.0, 64, 5.0, -2, 1));
}

/// Bug 回归：A 端撤销「创建音符」时本地音符消失，但此前不广播删除事件，
/// 导致 B 端残留该音符。本测试验证 undo/redo 创建会正确填充
/// `pending_collab_create_sync`（undo→`LocalNoteDeleted`，redo→`LocalNoteAdded`）。
#[test]
fn test_undo_redo_create_populates_collab_create_sync() {
    let mut data = EditorData::with_f32_notes(0, &[Note::new(0.0, 60, 1.0)]);
    // 模拟「创建」一个位于 (100, 72) 的新音符
    let op = CreateOp {
        track_id: 0,
        note: NoteEvent::new(100, 101, 72, 100, 0),
    };
    // 正向应用（创建）
    data.apply_create_ops(&[op.clone()], false);
    assert_eq!(data.current_track_note_count(), 2, "创建后应有 2 个音符");
    data.push_note_create(vec![op]);

    assert!(
        data.take_pending_collab_create_sync().is_empty(),
        "创建提交阶段不应填充队列"
    );

    // ── undo：本地删除，应广播 LocalNoteDeleted（is_added=false）──
    assert!(data.undo());
    assert_eq!(
        data.current_track_note_count(),
        1,
        "撤销创建后本地只剩原音符"
    );
    let pending = data.take_pending_collab_create_sync();
    assert_eq!(pending.len(), 1);
    let (tick, key, _len, _vel, _ch, track, is_added) = pending[0];
    assert_eq!(tick, 100.0);
    assert_eq!(key, 72);
    assert_eq!(track, 0);
    assert!(!is_added, "undo 创建应为删除（is_added=false）");

    // ── redo：本地重新插入，应广播 LocalNoteAdded（is_added=true）──
    assert!(data.redo());
    assert_eq!(data.current_track_note_count(), 2);
    let pending2 = data.take_pending_collab_create_sync();
    assert_eq!(pending2.len(), 1);
    let (tick2, key2, _len2, _vel2, _ch2, _track2, is_added2) = pending2[0];
    assert_eq!(tick2, 100.0);
    assert_eq!(key2, 72);
    assert!(is_added2, "redo 创建应为添加（is_added=true）");
}
