use super::*;
use crate::DragState;
use crate::EditorData;
use bit_vec::BitVec;
use lumino_note_core::history::MoveOp;

/// 取指定音轨按索引的 id 列表（测试构造 MoveOp 用）
fn ids_of(data: &EditorData, track: usize, indices: &[usize]) -> Vec<u64> {
    let track_notes = data.track_notes(track);
    indices
        .iter()
        .filter_map(|&i| track_notes.get(i).map(|n| n.id))
        .collect()
}

#[test]
fn test_apply_move_ops_forward() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        ids: ids_of(&data, 1, &[0, 1]),
        delta_tick: 5,
        delta_key: -2,
        seq: 0,
        original_ticks: vec![0.0, 10.0],
        original_keys: vec![60, 62],
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
        ids: ids_of(&data, 1, &[0, 1, 2]),
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
    let ids = ids_of(&data, 1, &[0, 1]);
    let ops = vec![MoveOp {
        track_id: 1,
        ids: vec![ids[0]],
        delta_tick: 0,
        delta_key: -100,
        seq: 0,
        original_ticks: vec![0.0],
        original_keys: vec![60],
    }];
    data.apply_move_ops(&ops, false, 20);
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        0,
        "key 应 clamp 到 0"
    );

    let ops2 = vec![MoveOp {
        track_id: 1,
        ids: vec![ids[1]],
        delta_tick: 0,
        delta_key: 100,
        seq: 0,
        original_ticks: vec![10.0],
        original_keys: vec![62],
    }];
    data.apply_move_ops(&ops2, false, 20);
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        20,
        "key 应 clamp 到 max_key"
    );
}

#[test]
fn test_apply_move_ops_empty_ids_is_noop() {
    let mut data = make_data_with_notes();
    let ops = vec![MoveOp {
        track_id: 1,
        ids: vec![],
        delta_tick: 5,
        delta_key: 0,
        seq: 0,
        original_ticks: vec![],
        original_keys: vec![],
    }];
    assert_eq!(
        data.apply_move_ops(&ops, false, 127),
        0,
        "空 id 列表不得修改任何音符"
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
        ids: ids_of(&data, 2, &[0]),
        delta_tick: 3,
        delta_key: 1,
        seq: 0,
        original_ticks: vec![0.0],
        original_keys: vec![60],
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
    assert_eq!(ops[0].ids, ids_of(&data, 1, &[0, 1]), "段 1 捕获 id");
    assert_eq!(ops[0].delta_tick, 5);
    assert_eq!(ops[0].delta_key, -2);
    assert_eq!(ops[0].seq, 0);
    assert_eq!(ops[0].original_ticks, vec![0.0, 10.0]);
    assert_eq!(ops[0].original_keys, vec![60, 62]);

    assert_eq!(ops[1].ids, ids_of(&data, 1, &[3]), "段 2 捕获 id");
    assert_eq!(ops[1].delta_tick, 5);
    assert_eq!(ops[1].delta_key, -2);
    assert_eq!(ops[1].seq, 1);
    assert_eq!(ops[1].original_ticks, vec![30.0]);
    assert_eq!(ops[1].original_keys, vec![66]);
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
/// undo/redo 必须按 id 恢复/前进被移动音符，且不得破坏远端与无关音符。
///
/// 旧实现按索引区间应用：漂移后会把索引 2 处的「无关音符」改到原位置
/// （数据损坏），并在 redo 时继续错位。
#[test]
fn test_undo_move_op_survives_index_drift() {
    let mut data = make_data_with_notes();
    let moved_id = data.track_notes(1).get(2).expect("第 3 个音符应存在").id;

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
    let remote_id = data
        .track_notes(1)
        .iter()
        .find(|n| n.key == 70)
        .expect("远端音符应存在")
        .id;

    // undo：按 id 精确恢复（旧实现按索引 2 会损坏 tick=10 的音符）
    assert!(data.undo());
    let track = data.track_notes(1);
    let restored = track
        .iter()
        .find(|n| n.id == moved_id)
        .expect("被移动音符应仍存在");
    assert_eq!(restored.start_tick, 20, "undo 必须按 id 恢复被移动音符");
    let remote = track
        .iter()
        .find(|n| n.id == remote_id)
        .expect("远端音符应仍存在");
    assert_eq!(remote.start_tick, 5, "远端音符不得被破坏");
    assert!(
        track.iter().any(|n| n.start_tick == 10 && n.key == 62),
        "无关音符不得被移动"
    );

    // redo：按 id 重新前进到 120
    assert!(data.redo());
    let track = data.track_notes(1);
    let moved = track
        .iter()
        .find(|n| n.id == moved_id)
        .expect("redo 后被移动音符应存在");
    assert_eq!(moved.start_tick, 120, "redo 必须按 id 前进");
    assert!(
        track.iter().any(|n| n.id == remote_id && n.start_tick == 5),
        "redo 后远端音符仍不得被破坏"
    );
}
