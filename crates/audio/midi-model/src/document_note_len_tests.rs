//! 每轨最大音符长度缓存（cull 可见区间裁剪用）单元测试

use crate::document::MidiDocument;
use crate::note_event::NoteEvent;

fn doc_with_one_track() -> MidiDocument {
    MidiDocument::empty_with_tracks(1, 480)
}

fn note(start: u32, len: u32) -> NoteEvent {
    NoteEvent::new(start, start + len, 60, 100, 0)
}

#[test]
fn test_empty_tracks_have_known_zero() {
    let doc = MidiDocument::empty_with_tracks(3, 480);
    for t in 0..3 {
        assert_eq!(
            doc.track_max_note_len(t),
            Some(0),
            "空轨最大长度应为已知真值 0（不是脏）"
        );
    }
}

#[test]
fn test_insert_grows_max_incrementally() {
    let mut doc = doc_with_one_track();
    assert_eq!(doc.track_max_note_len(0), Some(0));

    assert!(doc.insert_note(0, note(0, 100)));
    assert_eq!(doc.track_max_note_len(0), Some(100));

    // 插入更短音符：max 不变
    assert!(doc.insert_note(0, note(200, 50)));
    assert_eq!(doc.track_max_note_len(0), Some(100));

    // 插入更长音符：O(1) 取大
    assert!(doc.insert_note(0, note(400, 600)));
    assert_eq!(doc.track_max_note_len(0), Some(600));
}

#[test]
fn test_remove_max_invalidates_but_normal_remove_keeps_cache() {
    let mut doc = doc_with_one_track();
    doc.insert_note(0, note(0, 100)); // idx 0
    doc.insert_note(0, note(200, 50)); // idx 1
    doc.insert_note(0, note(400, 600)); // idx 2

    // 删除非最长音符：缓存保持有效
    let removed = doc.remove_note(0, 1).expect("删除普通音符");
    assert_eq!(removed.end_tick - removed.start_tick, 50);
    assert_eq!(
        doc.track_max_note_len(0),
        Some(600),
        "删除非最长音符不应置脏"
    );

    // 删除最长音符：保守置脏（调用方退化整轨）
    let removed = doc.remove_note(0, 1).expect("删除最长音符");
    assert_eq!(removed.end_tick - removed.start_tick, 600);
    assert_eq!(doc.track_max_note_len(0), None, "删除最长音符必须置脏");
}

#[test]
fn test_replace_and_clear_cache_semantics() {
    let mut doc = doc_with_one_track();
    doc.insert_note(0, note(0, 100));
    assert_eq!(doc.track_max_note_len(0), Some(100));

    // 整轨替换 → 置脏（新内容未知，宁可退化为整轨范围）
    assert!(doc.replace_track_notes(0, vec![note(0, 900)]));
    assert_eq!(doc.track_max_note_len(0), None);

    // 清空 → 空轨真值 0
    assert!(doc.clear_track_notes(0));
    assert_eq!(doc.track_max_note_len(0), Some(0));
}

#[test]
fn test_mut_access_invalidates() {
    let mut doc = doc_with_one_track();
    doc.insert_note(0, note(0, 100));
    assert_eq!(doc.track_max_note_len(0), Some(100));

    let _track = doc.track_notes_mut(0).expect("音轨可变引用");
    assert_eq!(
        doc.track_max_note_len(0),
        None,
        "可变引用逃逸后无法感知修改，必须置脏"
    );
}

#[test]
fn test_clone_isolates_cache_cells() {
    let mut doc = doc_with_one_track();
    doc.insert_note(0, note(0, 100));
    let snapshot = doc.clone();

    // 实时文档继续插入更长音符
    doc.insert_note(0, note(500, 900));
    assert_eq!(doc.track_max_note_len(0), Some(900));
    assert_eq!(
        snapshot.track_max_note_len(0),
        Some(100),
        "快照必须保持自己的缓存世代，不能被实时文档改写"
    );
}

#[test]
fn test_missing_track_returns_none() {
    let doc = doc_with_one_track();
    assert_eq!(doc.track_max_note_len(9), None, "越界音轨不得 panic");
}

/// 最小合法 MIDI：一轨两个音符，长度 10 与 100（最大 = 100）。
fn minimal_midi() -> Vec<u8> {
    let mut track = Vec::new();
    track.extend_from_slice(&[0x00, 0x90, 0x3C, 0x64]); // NoteOn C4
    track.extend_from_slice(&[0x0A, 0x80, 0x3C, 0x00]); // +10 NoteOff
    track.extend_from_slice(&[0x00, 0x90, 0x3E, 0x64]); // NoteOn D4
    track.extend_from_slice(&[0x64, 0x80, 0x3E, 0x00]); // +100 NoteOff
    track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]); // End of Track

    let mut file = Vec::new();
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6u32.to_be_bytes());
    file.extend_from_slice(&0u16.to_be_bytes()); // format 0
    file.extend_from_slice(&1u16.to_be_bytes()); // 1 track
    file.extend_from_slice(&480u16.to_be_bytes()); // division
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&(track.len() as u32).to_be_bytes());
    file.extend_from_slice(&track);
    file
}

#[test]
fn test_build_populates_max_note_len() {
    let bytes = minimal_midi();
    let (doc, _division, notes) =
        MidiDocument::from_notes_bytes(&bytes, None).expect("最小 MIDI 应解析成功");
    assert_eq!(notes, 2);
    assert_eq!(
        doc.track_max_note_len(0),
        Some(100),
        "MIDI 构建路径必须写入每轨最大音符长度真值"
    );
}
