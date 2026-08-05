use super::*;
use bit_vec::BitVec;

#[test]
fn test_apply_drag_state_streaming_moves_selected_and_syncs_track() {
    let mut data = EditorData::new();
    data.current_track = 1;
    data.notes.push_back(Note::new(0.0, 60, 1.0));
    data.notes.push_back(Note::new(10.0, 62, 1.0));
    data.notes.push_back(Note::new(20.0, 64, 1.0));
    data.track_notes.insert(1, data.notes.clone());

    let mut bv = BitVec::from_elem(3, false);
    bv.set(0, true);
    bv.set(2, true);
    let mut drag_state = DragState::new(bv, 0, 60);
    drag_state.set_delta(5, -2);

    let modified = data.apply_drag_state_streaming(&drag_state, 127);
    assert_eq!(modified, 2);

    // notes 已更新
    assert_eq!(data.notes[0].tick, 5.0);
    assert_eq!(data.notes[0].key, 58);
    assert_eq!(data.notes[1].tick, 10.0, "未选中音符不变");
    assert_eq!(data.notes[1].key, 62);
    assert_eq!(data.notes[2].tick, 25.0);
    assert_eq!(data.notes[2].key, 62);

    // track_notes 同步更新
    let track = data.track_notes.get(&1).unwrap();
    assert_eq!(track[0].tick, 5.0);
    assert_eq!(track[0].key, 58);
    assert_eq!(track[1].tick, 10.0);
    assert_eq!(track[2].tick, 25.0);
    assert_eq!(data.track_notes_gen, 1);
}

#[test]
fn test_apply_drag_state_streaming_zero_delta_is_noop() {
    let mut data = EditorData::new();
    data.current_track = 1;
    data.notes.push_back(Note::new(0.0, 60, 1.0));
    data.track_notes.insert(1, data.notes.clone());

    let ds = DragState::from_single(0, 1, 0, 60);
    let modified = data.apply_drag_state_streaming(&ds, 127);
    assert_eq!(modified, 0);
    assert_eq!(data.track_notes_gen, 0, "无变更时不应 bump 版本");
}

#[test]
fn test_sync_track_notes_at_indices_partial() {
    let mut data = EditorData::new();
    data.current_track = 2;
    data.notes.push_back(Note::new(0.0, 60, 1.0));
    data.notes.push_back(Note::new(10.0, 62, 1.0));
    data.notes.push_back(Note::new(20.0, 64, 1.0));
    data.track_notes.insert(2, data.notes.clone());

    // 只改 notes[1]
    data.notes[1].tick = 99.0;
    data.notes[1].key = 70;

    data.sync_track_notes_at_indices(&[1]);

    let track = data.track_notes.get(&2).unwrap();
    assert_eq!(track[0].tick, 0.0, "未同步索引保持不变");
    assert_eq!(track[1].tick, 99.0, "同步索引已更新");
    assert_eq!(track[1].key, 70);
    assert_eq!(track[2].tick, 20.0, "未同步索引保持不变");
    assert_eq!(data.track_notes_gen, 1);
}

#[test]
fn test_sync_track_notes_at_indices_creates_entry_when_missing() {
    let mut data = EditorData::new();
    data.current_track = 3;
    data.notes.push_back(Note::new(5.0, 60, 2.0));

    data.sync_track_notes_at_indices(&[0]);

    let track = data.track_notes.get(&3).unwrap();
    assert_eq!(track[0].tick, 5.0);
    assert_eq!(data.track_notes_gen, 1);
}
