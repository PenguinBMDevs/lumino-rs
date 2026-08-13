use super::*;
use bit_vec::BitVec;

#[test]
fn test_apply_drag_state_streaming_moves_selected_and_syncs_track() {
    let mut data = EditorData::with_f32_notes(
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
        ],
    );

    let mut bv = BitVec::from_elem(3, false);
    bv.set(0, true);
    bv.set(2, true);
    let mut drag_state = DragState::new(bv, 0, 60);
    drag_state.set_delta(5, -2);

    let modified = data.apply_drag_state_streaming(&drag_state, 127);
    assert_eq!(modified, 2);

    // notes 已更新（document 唯一权威源）
    let view0 = data.get_note_view(0).expect("第 1 个音符视图应存在");
    assert_eq!(view0.tick, 5.0);
    assert_eq!(view0.key, 58);
    let view1 = data.get_note_view(1).expect("第 2 个音符视图应存在");
    assert_eq!(view1.tick, 10.0, "未选中音符不变");
    assert_eq!(view1.key, 62);
    let view2 = data.get_note_view(2).expect("第 3 个音符视图应存在");
    assert_eq!(view2.tick, 25.0);
    assert_eq!(view2.key, 62);

    // document 同步更新（track_notes(1) 即 document 当前轨）
    let track = data.track_notes(1);
    assert_eq!(track[0].start_tick as f32, 5.0);
    assert_eq!(track[0].key as u16, 58);
    assert_eq!(track[1].start_tick as f32, 10.0);
    assert_eq!(track[2].start_tick as f32, 25.0);
    assert_eq!(data.track_notes_gen, 1);
}

#[test]
fn test_apply_drag_state_streaming_zero_delta_is_noop() {
    let mut data = EditorData::with_f32_notes(1, &[Note::new(0.0, 60, 1.0)]);

    let ds = DragState::from_single(0, data.current_track_note_count(), 0, 60);
    let modified = data.apply_drag_state_streaming(&ds, 127);
    assert_eq!(modified, 0);
    assert_eq!(data.track_notes_gen, 0, "无变更时不应 bump 版本");
}

#[test]
fn test_sync_track_notes_at_indices_partial() {
    // 语义替代（2026-08）：sync_track_notes_at_indices 已随 track_notes 缓存层删除，
    // document 为唯一权威源。原测试意图「只修改部分索引、其余保持不变」
    // 由 apply_drag_state_streaming（只改选中索引）覆盖。
    let mut data = EditorData::with_f32_notes(
        2,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(10.0, 62, 1.0),
            Note::new(20.0, 64, 1.0),
        ],
    );

    // 只改索引 1（+89 tick, +8 key：10→99, 62→70）
    let mut bv = BitVec::from_elem(3, false);
    bv.set(1, true);
    let mut ds = DragState::new(bv, 0, 60);
    ds.set_delta(89, 8);
    let modified = data.apply_drag_state_streaming(&ds, 127);
    assert_eq!(modified, 1);

    let track = data.track_notes(2);
    assert_eq!(track[0].start_tick as f32, 0.0, "未修改索引保持不变");
    assert_eq!(track[1].start_tick as f32, 99.0, "修改索引已更新");
    assert_eq!(track[1].key as u16, 70);
    assert_eq!(track[2].start_tick as f32, 20.0, "未修改索引保持不变");
    assert_eq!(data.track_notes_gen, 1);
}

#[test]
fn test_sync_track_notes_at_indices_creates_entry_when_missing() {
    // 语义替代（2026-08）：sync_track_notes_at_indices 已删除。
    // 原意图「track_notes 缺失条目自动创建」→ document 构造即含全部轨道，
    // 断言 track 3 音符可直接读取（无需任何同步操作）。
    let data = EditorData::with_f32_notes(3, &[Note::new(5.0, 60, 2.0)]);
    let track = data.track_notes(3);
    assert_eq!(track.len(), 1);
    assert_eq!(track[0].start_tick as f32, 5.0);
    assert_eq!(data.track_notes_gen, 0);
}

#[test]
fn test_apply_drag_state_streaming_preserves_note_length() {
    // 移动不改变长度：start 平移时 end 必须跟随，length = end - start 保持不变
    let mut data = EditorData::with_f32_notes(1, &[Note::new(0.0, 60, 480.0)]);

    let mut drag_state = DragState::from_single(0, 1, 0, 60);
    drag_state.set_delta(200, 0); // 右移 200 tick
    let modified = data.apply_drag_state_streaming(&drag_state, 127);
    assert_eq!(modified, 1);

    let view = data.get_note_view(0).expect("第 1 个音符视图应存在");
    assert_eq!(view.tick, 200.0);
    assert_eq!(view.length, 480.0, "右移后长度必须保持 480");
}

#[test]
fn test_apply_drag_state_streaming_preserves_length_on_left_move() {
    // 左移（delta 为负）同样保持长度
    let mut data = EditorData::with_f32_notes(1, &[Note::new(300.0, 60, 480.0)]);

    let mut drag_state = DragState::from_single(0, 1, 300, 60);
    drag_state.set_delta(-100, 0);
    let modified = data.apply_drag_state_streaming(&drag_state, 127);
    assert_eq!(modified, 1);

    let view = data.get_note_view(0).expect("第 1 个音符视图应存在");
    assert_eq!(view.tick, 200.0);
    assert_eq!(view.length, 480.0, "左移后长度必须保持 480");
}
