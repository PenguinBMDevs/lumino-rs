use lumino_midi_loader::NoteInfo;

use crate::onion_skin_bucket::build_list_from_notes;

fn make_note_info(start: u32, length: u32, key: u8) -> NoteInfo {
    NoteInfo::new(start, length, key, 100, 0)
}

/// 测试构建和基本属性
#[test]
fn test_build_and_basic() {
    let notes = vec![
        make_note_info(0, 10, 60),
        make_note_info(5, 10, 61),
        make_note_info(20, 10, 60),
    ];
    let list = build_list_from_notes(&notes, 1);
    assert_eq!(list.len(), 3);
    assert!(!list.is_empty());
    assert_eq!(list.as_slice().len(), 3);
}

/// 测试清空
#[test]
fn test_clear() {
    let notes = vec![make_note_info(0, 10, 60)];
    let mut list = build_list_from_notes(&notes, 1);
    assert_eq!(list.len(), 1);
    list.clear();
    assert_eq!(list.len(), 0);
    assert!(list.is_empty());
}

/// 测试版本号变化
#[test]
fn test_version_on_change() {
    let mut list = crate::OnionNoteList::new();
    let v0 = list.version();

    // 清空空列表不改变 version
    list.clear();
    assert_eq!(list.version(), v0 + 1);

    // update_user_track 改变 version
    let user_notes = vec![lumino_core::Note::new(50.0, 60, 10.0)];
    list.update_user_track(2, user_notes.iter());
    assert!(list.version() > v0 + 1);
    assert_eq!(list.len(), 1);
}

/// 测试 update_user_track：先移除再添加
#[test]
fn test_update_user_track() {
    let notes = vec![make_note_info(0, 10, 60), make_note_info(100, 10, 61)];
    let mut list = build_list_from_notes(&notes, 1);
    assert_eq!(list.len(), 2);

    let user_notes = vec![
        lumino_core::Note::new(50.0, 60, 10.0),
        lumino_core::Note::new(150.0, 62, 10.0),
    ];
    list.update_user_track(2, user_notes.iter());
    assert_eq!(list.len(), 4);

    // 再次更新同一音轨：应该先移除旧音符再添加新音符
    let user_notes2 = vec![lumino_core::Note::new(200.0, 63, 5.0)];
    list.update_user_track(2, user_notes2.iter());
    assert_eq!(list.len(), 3);
}

/// 测试 remove_track
#[test]
fn test_remove_track() {
    let notes = vec![
        make_note_info(0, 10, 60),
        make_note_info(5, 10, 61),
        make_note_info(10, 10, 62),
    ];
    let mut list = build_list_from_notes(&notes, 1);
    assert_eq!(list.len(), 3);

    // 移除 track 2（不存在的音轨）
    list.remove_track(2);
    assert_eq!(list.len(), 3);

    // 移除 track 1
    list.remove_track(1);
    assert_eq!(list.len(), 0);
}

/// 测试 key > 255 被过滤
#[test]
fn test_skip_out_of_range_key() {
    let mut list = crate::OnionNoteList::new();
    let user_notes = vec![
        lumino_core::Note::new(0.0, 60, 10.0),  // key=60, valid
        lumino_core::Note::new(10.0, 300, 5.0), // key=300, invalid
    ];
    list.update_user_track(1, user_notes.iter());
    assert_eq!(list.len(), 1, "key > 255 should be filtered");
    assert_eq!(list.as_slice()[0].pitch(), 60);
}

/// 测试 length = 0 的音符
#[test]
fn test_zero_length_note() {
    let mut list = crate::OnionNoteList::new();
    let user_notes = vec![lumino_core::Note::new(0.0, 60, 0.0)];
    list.update_user_track(1, user_notes.iter());
    // length=0 应该被正常添加，end = start + 0
    assert_eq!(list.len(), 1);
    let note = &list.as_slice()[0];
    assert_eq!(note.start_tick, 0);
    assert_eq!(note.end_tick, 0);
}

/// 测试音符按 start_tick 排序并正确分块
#[test]
fn test_notes_sorted_and_chunked() {
    let notes = vec![
        make_note_info(100, 10, 60),
        make_note_info(0, 10, 61),
        make_note_info(50, 10, 62),
    ];
    let list = build_list_from_notes(&notes, 1);

    // 按 start_tick 排序：0, 50, 100
    let slice = list.as_slice();
    assert_eq!(slice[0].start_tick(), 0);
    assert_eq!(slice[1].start_tick(), 50);
    assert_eq!(slice[2].start_tick(), 100);

    // 只有 3 个音符，应该只有 1 个 chunk
    let chunks = list.chunks();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].tick_start, 0);
    assert_eq!(chunks[0].tick_end, 110); // 100 + 10
    assert_eq!(chunks[0].note_start, 0);
    assert_eq!(chunks[0].note_end, 3);
}

/// 测试 update_user_track 后重新排序并分块
#[test]
fn test_update_user_track_rechunks() {
    let mut list = build_list_from_notes(&[make_note_info(10, 5, 60)], 1);
    assert_eq!(list.chunks().len(), 1);

    list.update_user_track(2, [lumino_core::Note::new(5.0, 61, 3.0)].iter());
    assert_eq!(list.len(), 2);

    let slice = list.as_slice();
    assert_eq!(slice[0].start_tick(), 5);
    assert_eq!(slice[1].start_tick(), 10);

    let chunks = list.chunks();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].tick_start, 5);
    assert_eq!(chunks[0].tick_end, 15); // 10 + 5
}
