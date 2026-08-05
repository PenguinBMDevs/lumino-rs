//! MidiDocument 可写 API（insert/remove/update）单元测试
//!
//! 独立文件引入（document.rs 底部 `#[path] mod tests`），保持 document.rs 行数合理。
//! 覆盖：插入排序不变式、稳定插入、越界处理、删除/替换一致性。

use super::*;

/// 构造音轨音符列表：`(start_tick, end_tick, key)` → NoteEvent（velocity=100, channel=0）。
/// 按 start_tick 升序排序，模拟加载后的有序轨道。
fn make_track(notes: &[(u32, u32, u8)]) -> Vec<NoteEvent> {
    let mut v: Vec<NoteEvent> = notes
        .iter()
        .map(|&(s, e, k)| NoteEvent::new(s, e, k, 100, 0))
        .collect();
    v.sort_unstable_by_key(|n| n.start_tick);
    v
}

/// 构造测试文档：给定每轨音符列表，其余字段取最小合理值。
fn make_doc(tracks: Vec<Vec<NoteEvent>>) -> MidiDocument {
    let track_count = tracks.len() as u16;
    MidiDocument {
        notes: tracks,
        tempo_changes: vec![],
        time_signatures: vec![],
        key_signatures: vec![],
        control_events: vec![],
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: (0..track_count)
            .map(|i| Some(format!("Track {i}")))
            .collect(),
        total_ticks: 0,
        track_count,
        tracks: TrackManager::new(track_count),
        division: 480,
        track_ports: vec![0; track_count as usize],
    }
}

#[test]
fn test_insert_note_empty_track() {
    let mut doc = make_doc(vec![make_track(&[])]);

    // 空轨插入第一个音符
    let note = NoteEvent::new(100, 200, 60, 100, 0);
    assert!(doc.insert_note(0, note));

    // notes[0] 只有一个音符，且与插入值一致
    assert_eq!(doc.notes[0].len(), 1);
    assert_eq!(doc.notes[0][0], note);
}

#[test]
fn test_insert_note_keeps_sorted() {
    let mut doc = make_doc(vec![make_track(&[])]);

    // 乱序插入 5 个音符
    for (s, e, k) in [
        (300, 400, 60),
        (100, 200, 61),
        (500, 600, 62),
        (200, 300, 63),
        (400, 500, 64),
    ] {
        assert!(doc.insert_note(0, NoteEvent::new(s, e, k, 100, 0)));
    }

    // 验证最终按 start_tick 升序
    let starts: Vec<u32> = doc.notes[0].iter().map(|n| n.start_tick).collect();
    assert_eq!(starts, vec![100, 200, 300, 400, 500]);
    // 顺带验证对应 key 未被扰动
    let keys: Vec<u8> = doc.notes[0].iter().map(|n| n.key).collect();
    assert_eq!(keys, vec![61, 63, 60, 64, 62]);
}

#[test]
fn test_insert_note_same_tick_stable() {
    let mut doc = make_doc(vec![make_track(&[])]);

    // 同 tick 插入 3 个，用 key 区分插入顺序
    for (k, e) in [(60u8, 200u32), (61, 300), (62, 400)] {
        assert!(doc.insert_note(0, NoteEvent::new(100, e, k, 100, 0)));
    }

    // 稳定插入：后插的在后
    let keys: Vec<u8> = doc.notes[0].iter().map(|n| n.key).collect();
    assert_eq!(keys, vec![60, 61, 62]);
}

#[test]
fn test_insert_note_track_out_of_range() {
    let mut doc = make_doc(vec![make_track(&[(100, 200, 60)])]);
    let before: Vec<NoteEvent> = doc.notes[0].clone();

    // track_id = 999 越界：返回 false，数据不变
    assert!(!doc.insert_note(999, NoteEvent::new(300, 400, 61, 100, 0)));
    assert_eq!(doc.notes[0], before);
    assert_eq!(doc.notes.len(), 1);
}

#[test]
fn test_remove_note_middle() {
    let mut doc = make_doc(vec![make_track(&[
        (100, 200, 60),
        (200, 300, 61),
        (300, 400, 62),
    ])]);

    // 删除中间的（index=1）
    let removed = doc.remove_note(0, 1);
    assert_eq!(
        removed,
        Some(NoteEvent::new(200, 300, 61, 100, 0)),
        "应返回被删除的音符副本"
    );

    // 顺序保持：剩下 100, 300
    let starts: Vec<u32> = doc.notes[0].iter().map(|n| n.start_tick).collect();
    assert_eq!(starts, vec![100, 300]);
    assert_eq!(doc.notes[0].len(), 2);
}

#[test]
fn test_remove_note_out_of_range() {
    let mut doc = make_doc(vec![make_track(&[(100, 200, 60), (200, 300, 61)])]);
    let before: Vec<NoteEvent> = doc.notes[0].clone();

    // index 越界
    assert!(doc.remove_note(0, 5).is_none());
    // track_id 越界
    assert!(doc.remove_note(99, 0).is_none());

    // 数据不变
    assert_eq!(doc.notes[0], before);
}

#[test]
fn test_update_note_reorders() {
    let mut doc = make_doc(vec![make_track(&[
        (100, 200, 60),
        (200, 300, 61),
        (300, 400, 62),
    ])]);

    // 把 index=0（start=100）替换为 start=600，应重排到最后
    assert!(doc.update_note(0, 0, NoteEvent::new(600, 700, 70, 110, 0)));

    // 轨道仍有序且位置正确
    let starts: Vec<u32> = doc.notes[0].iter().map(|n| n.start_tick).collect();
    assert_eq!(starts, vec![200, 300, 600]);
    let keys: Vec<u8> = doc.notes[0].iter().map(|n| n.key).collect();
    assert_eq!(keys, vec![61, 62, 70]);
    assert_eq!(doc.notes[0].len(), 3);
}

#[test]
fn test_update_note_out_of_range() {
    let mut doc = make_doc(vec![make_track(&[(100, 200, 60)])]);

    // index 越界
    assert!(!doc.update_note(0, 9, NoteEvent::new(500, 600, 70, 100, 0)));
    // track_id 越界
    assert!(!doc.update_note(9, 0, NoteEvent::new(500, 600, 70, 100, 0)));
    // 数据不变
    assert_eq!(doc.notes[0].len(), 1);
    assert_eq!(doc.notes[0][0], NoteEvent::new(100, 200, 60, 100, 0));
}

#[test]
fn test_track_notes_mut() {
    let mut doc = make_doc(vec![make_track(&[(100, 200, 60), (200, 300, 61)])]);

    // 拿到可变引用后 push 一个音符，len 增加
    {
        let track = doc.track_notes_mut(0).expect("轨道 0 应存在");
        track.push(NoteEvent::new(300, 400, 62, 100, 0));
    }
    assert_eq!(doc.notes[0].len(), 3);

    // track_id 越界返回 None
    assert!(doc.track_notes_mut(99).is_none());
}

#[test]
fn test_insert_after_remove_consistency() {
    let mut doc = make_doc(vec![make_track(&[])]);

    // 插入-删除-插入循环后数据与预期一致
    assert!(doc.insert_note(0, NoteEvent::new(300, 400, 62, 100, 0)));
    assert!(doc.insert_note(0, NoteEvent::new(100, 200, 60, 100, 0)));
    assert!(doc.insert_note(0, NoteEvent::new(200, 300, 61, 100, 0)));
    assert_eq!(doc.notes[0].len(), 3);

    // 删除中间（start=200）
    let removed = doc.remove_note(0, 1);
    assert_eq!(
        removed,
        Some(NoteEvent::new(200, 300, 61, 100, 0)),
        "应返回被删除的音符副本"
    );
    assert_eq!(doc.notes[0].len(), 2);

    // 再插入填补空隙，最终有序
    assert!(doc.insert_note(0, NoteEvent::new(250, 350, 65, 100, 0)));
    let starts: Vec<u32> = doc.notes[0].iter().map(|n| n.start_tick).collect();
    assert_eq!(starts, vec![100, 250, 300]);
}
