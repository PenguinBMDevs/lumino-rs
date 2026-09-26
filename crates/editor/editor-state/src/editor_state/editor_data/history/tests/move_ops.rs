use super::*;
use crate::DragState;
use crate::EditorData;
use bit_vec::BitVec;
use lumino_midi_model::NoteEvent;
use lumino_note_core::history::MoveOp;

/// 取指定音轨按索引的原始值快照（测试构造 MoveOp 用，按值引用）
fn originals_of(data: &EditorData, track: usize, indices: &[usize]) -> Vec<NoteEvent> {
    let track_notes = data.track_notes(track);
    indices
        .iter()
        .filter_map(|&i| track_notes.get(i).copied())
        .collect()
}

/// originals 的 tick 序列（f32 视图，便于断言）
fn ticks_of(originals: &[NoteEvent]) -> Vec<f32> {
    originals.iter().map(|n| n.start_tick as f32).collect()
}

/// originals 的 key 序列
fn keys_of(originals: &[NoteEvent]) -> Vec<u16> {
    originals.iter().map(|n| n.key as u16).collect()
}

#[test]
fn test_apply_move_ops_forward() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        originals: originals_of(&data, 1, &[0, 1]),
        delta_tick: 5,
        delta_key: -2,
        seq: 0,
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
        originals: originals_of(&data, 1, &[0, 1, 2]),
        delta_tick: 10,
        delta_key: 5,
        seq: 0,
    }];
    // 先 forward
    data.apply_move_ops(&ops, false, 127);
    // 再 inverse 应还原（同一前向 op + inverse 标志）
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
    let all = originals_of(&data, 1, &[0, 1]);
    let ops = vec![MoveOp {
        track_id: 1,
        originals: vec![all[0]],
        delta_tick: 0,
        delta_key: -100,
        seq: 0,
    }];
    data.apply_move_ops(&ops, false, 20);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        0,
        "key 应 clamp 到 0"
    );

    let ops2 = vec![MoveOp {
        track_id: 1,
        originals: vec![all[1]],
        delta_tick: 0,
        delta_key: 100,
        seq: 0,
    }];
    data.apply_move_ops(&ops2, false, 20);
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        20,
        "key 应 clamp 到 max_key"
    );
}

#[test]
fn test_apply_move_ops_empty_originals_is_noop() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        originals: vec![],
        delta_tick: 5,
        delta_key: 0,
        seq: 0,
    }];
    assert_eq!(
        data.apply_move_ops(&ops, false, 127),
        0,
        "空 originals 列表不得修改任何音符"
    );
    assert_eq!(data.track_notes(1)[0].start_tick, 0);
}

#[test]
fn test_apply_move_ops_creates_missing_track_notes() {
    // 语义替代（2026-08）：apply_move_ops 不再自动创建缺失音轨（document 轨道
    // 构造时固定）。原测试意图「操作指定轨数据」改为：构造含 track 2 的 document，
    // 验证 apply_move_ops 可作用于非当前轨（track_id=2）。
    let mut data = EditorData::with_f32_notes(2, &[Note::new(0.0, 60, 1.0)]);
    let ops = vec![MoveOp {
        track_id: 2,
        originals: originals_of(&data, 2, &[0]),
        delta_tick: 3,
        delta_key: 1,
        seq: 0,
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
    assert_eq!(
        ticks_of(&ops[0].originals),
        vec![0.0, 10.0],
        "段 1 捕获原始值快照"
    );
    assert_eq!(keys_of(&ops[0].originals), vec![60, 62]);
    assert_eq!(ops[0].delta_tick, 5);
    assert_eq!(ops[0].delta_key, -2);
    assert_eq!(ops[0].seq, 0);

    assert_eq!(
        ticks_of(&ops[1].originals),
        vec![30.0],
        "段 2 捕获原始值快照"
    );
    assert_eq!(keys_of(&ops[1].originals), vec![66]);
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

/// 回归：移动提交后协作远端在轨道头部插入音符（索引漂移），
/// undo/redo 必须按值恢复/前进被移动音符，且不得破坏远端与无关音符。
///
/// 旧实现按索引区间应用：漂移后会把索引 2 处的「无关音符」改到原位置
/// （数据损坏），并在 redo 时继续错位。
#[test]
fn test_undo_move_op_survives_index_drift() {
    let mut data = make_data_with_notes();
    // 按值捕获被移动音符（tick=20 的完整快照，不依赖索引身份）
    let moved_val: NoteEvent = *data.track_notes(1).get(2).expect("第 3 个音符应存在");

    // 选中索引 2（tick=20）向右移动 +100
    let ops = data.move_ops_from_drag_state(&{
        let mut bv = BitVec::from_elem(3, false);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(100, 0);
        ds
    });
    assert_eq!(data.apply_move_ops(&ops, false, 127), 1);
    data.push_move_op(ops);

    // 模拟协作远端在索引 0 插入新音符（tick=5）→ 索引漂移
    assert!(data.insert_note(1, Note::new(5.0, 70, 1.0)));
    // 远端音符按值捕获（窗口定位用，不做全片扫描断言）
    let remote_val: NoteEvent =
        super::super::super::accessors::note_to_event(Note::new(5.0, 70, 1.0));

    // undo：按值精确恢复（旧实现按索引 2 会损坏 tick=10 的音符）
    assert!(data.undo());
    let track = data.track_notes(1);
    let restored_idx = track.position_of(&moved_val).expect("被移动音符应仍存在");
    assert_eq!(
        track.get(restored_idx).expect("恢复索引应有效").start_tick,
        20,
        "undo 必须按值恢复被移动音符"
    );
    let remote_idx = track.position_of(&remote_val).expect("远端音符应仍存在");
    assert_eq!(
        track.get(remote_idx).expect("远端索引应有效").start_tick,
        5,
        "远端音符不得被破坏"
    );
    assert!(
        track
            .position_of(&super::super::super::accessors::note_to_event(Note::new(
                10.0, 62, 1.0
            )))
            .is_some(),
        "无关音符不得被移动"
    );

    // redo：按值重新前进到 120
    assert!(data.redo());
    let track = data.track_notes(1);
    let mut expected_moved = moved_val;
    expected_moved.start_tick = 120;
    expected_moved.end_tick = 121;
    let moved_idx = track
        .position_of(&expected_moved)
        .expect("redo 后被移动音符应存在");
    assert_eq!(
        track.get(moved_idx).expect("移动索引应有效").start_tick,
        120,
        "redo 必须按值前进"
    );
    assert!(
        track.position_of(&remote_val).is_some(),
        "redo 后远端音符仍不得被破坏"
    );
}
