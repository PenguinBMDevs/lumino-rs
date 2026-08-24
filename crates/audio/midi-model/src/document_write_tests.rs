//! MidiDocument 可写 API（insert/remove/update）单元测试
//!
//! 独立文件引入（document.rs 底部 `#[path] mod tests`），保持 document.rs 行数合理。
//! 覆盖：插入排序不变式、稳定插入、越界处理、删除/替换一致性。

use super::MidiDocument;
use crate::note_event::NoteEvent;
use crate::track::TrackManager;

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
        next_note_id: 1,
        notes: tracks
            .into_iter()
            .map(crate::chunked_list::ChunkedList::from_sorted)
            .collect(),
        tempo_changes: vec![],
        time_signatures: vec![],
        key_signatures: vec![],
        control_events: crate::chunked_list::ChunkedList::new(),
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

        track_max_end_ticks: MidiDocument::new_track_max_ticks(track_count as usize),
    }
}

#[test]
fn test_insert_note_empty_track() {
    let mut doc = make_doc(vec![make_track(&[])]);

    // 空轨插入第一个音符
    let note = NoteEvent::new(100, 200, 60, 100, 0);
    assert!(doc.insert_note(0, note));

    // notes[0] 只有一个音符，内容字段与插入值一致，并被分配唯一非零 id
    assert_eq!(doc.notes[0].len(), 1);
    let stored = &doc.notes[0][0];
    assert_eq!(stored.start_tick, note.start_tick);
    assert_eq!(stored.end_tick, note.end_tick);
    assert_eq!(stored.key, note.key);
    assert_eq!(stored.velocity, note.velocity);
    assert_eq!(stored.channel, note.channel);
    assert_ne!(stored.id, NoteEvent::UNASSIGNED_ID);
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
    let before: Vec<NoteEvent> = doc.notes[0].to_vec();

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
    let before: Vec<NoteEvent> = doc.notes[0].to_vec();

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

    // 拿到可变引用后插入一个音符，len 增加
    {
        let track = doc.track_notes_mut(0).expect("轨道 0 应存在");
        track.insert(NoteEvent::new(300, 400, 62, 100, 0));
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

    // 删除中间（start=200）：返回被删除音符副本（含其被分配的唯一 id）
    let removed = doc.remove_note(0, 1);
    assert!(removed.is_some(), "应返回被删除的音符副本");
    let removed = removed.unwrap();
    assert_eq!(removed.start_tick, 200);
    assert_eq!(removed.end_tick, 300);
    assert_eq!(removed.key, 61);
    assert_eq!(removed.velocity, 100);
    assert_eq!(removed.channel, 0);
    assert_ne!(removed.id, NoteEvent::UNASSIGNED_ID);
    assert_eq!(doc.notes[0].len(), 2);

    // 再插入填补空隙，最终有序
    assert!(doc.insert_note(0, NoteEvent::new(250, 350, 65, 100, 0)));
    let starts: Vec<u32> = doc.notes[0].iter().map(|n| n.start_tick).collect();
    assert_eq!(starts, vec![100, 250, 300]);
}

#[test]
fn test_replace_track_notes() {
    let mut doc = make_doc(vec![make_track(&[(100, 200, 60)]), make_track(&[])]);

    // 整轨替换：新轨道数据（乱序输入由调用方负责排序，这里直接验证赋值）
    let new_notes = vec![
        NoteEvent::new(50, 100, 62, 90, 0),
        NoteEvent::new(200, 300, 64, 80, 0),
    ];
    assert!(doc.replace_track_notes(0, new_notes.clone()));
    // 整轨替换后内容字段与输入一致，且每个音符被赋予唯一非零 id
    assert_eq!(doc.notes[0].len(), 2);
    for (stored, expected) in doc.notes[0].iter().zip(new_notes.iter()) {
        assert_eq!(stored.start_tick, expected.start_tick);
        assert_eq!(stored.end_tick, expected.end_tick);
        assert_eq!(stored.key, expected.key);
        assert_eq!(stored.velocity, expected.velocity);
        assert_eq!(stored.channel, expected.channel);
        assert_ne!(stored.id, NoteEvent::UNASSIGNED_ID);
    }

    // 其他轨道不受影响
    assert!(doc.notes[1].is_empty());

    // track_id 越界返回 false，数据不变
    assert!(!doc.replace_track_notes(99, vec![NoteEvent::new(1, 2, 60, 100, 0)]));
    assert_eq!(doc.notes.len(), 2);
}

#[test]
fn test_clear_track_notes() {
    let mut doc = make_doc(vec![make_track(&[(100, 200, 60), (200, 300, 61)])]);

    assert!(doc.clear_track_notes(0));
    assert!(doc.notes[0].is_empty());
    assert_eq!(doc.notes.len(), 1);

    // track_id 越界返回 false
    assert!(!doc.clear_track_notes(99));
}

#[test]
fn test_empty_with_tracks_blank_project_can_insert() {
    // 2026-08 回归：空白工程（新建文件）必须立即可编辑——
    // empty_with_tracks 构造的空白文档插入音符应成功，且保持有序。
    let mut doc = MidiDocument::empty_with_tracks(2, 1920);
    assert_eq!(doc.track_count(), 2);
    assert!(doc.notes.iter().all(|t| t.is_empty()));
    assert_eq!(doc.track_name(0), Some("Conductor"));
    assert_eq!(doc.track_name(1), Some("Setup"));
    assert_eq!(doc.division, 1920);

    // 在 Setup 轨（track 1）创建音符，等价于空白工程走带添加
    assert!(doc.insert_note(1, NoteEvent::new(100, 200, 60, 100, 0)));
    assert!(doc.insert_note(1, NoteEvent::new(50, 150, 62, 100, 0)));
    // 保持升序不变式
    assert_eq!(doc.notes[1][0].start_tick, 50);
    assert_eq!(doc.notes[1][1].start_tick, 100);

    // track 0（Conductor）也能插入（空白工程语义不阻塞）
    assert!(doc.insert_note(0, NoteEvent::new(10, 20, 40, 100, 0)));
    assert_eq!(doc.notes[0].len(), 1);
}

#[test]
fn test_replace_track_notes_chunked_shares_blocks() {
    // 2026-08 回归：undo/redo 快照恢复走 replace_track_notes_chunked，
    // 必须保持块级 Arc 共享（O(块数) 浅拷贝，不复制音符数据）。
    let mut doc = make_doc(vec![make_track(&[(100, 200, 60), (300, 400, 62)])]);
    let snapshot = doc.notes[0].clone();
    assert_eq!(snapshot.len(), 2);

    assert!(doc.replace_track_notes_chunked(0, &snapshot));
    assert_eq!(doc.notes[0].len(), 2);
    assert_eq!(doc.notes[0].to_vec(), snapshot.to_vec());
    assert_eq!(doc.notes[0][0], NoteEvent::new(100, 200, 60, 100, 0));

    // 替换不改变快照；再次替换保持一致性
    assert!(doc.replace_track_notes_chunked(0, &snapshot));
    assert_eq!(doc.notes[0][1], NoteEvent::new(300, 400, 62, 100, 0));

    // track_id 越界返回 false
    assert!(!doc.replace_track_notes_chunked(99, &snapshot));
}

#[test]
fn test_track_max_end_tick_cache_incremental_and_per_track() {
    // 2026-08 回归：每轨 max_end_tick 缓存必须独立（构造时不能共享同一 Arc/Mutex），
    // 且插入应增量更新、删除/替换应置脏惰性重算。
    let mut doc = make_doc(vec![
        make_track(&[(100, 200, 60), (300, 900, 62)]), // track 0: max end = 900
        make_track(&[(50, 500, 64)]),                  // track 1: max end = 500
    ]);

    // 首次查询惰性重算
    assert_eq!(doc.track_max_end_tick(0), 900);
    assert_eq!(doc.track_max_end_tick(1), 500);
    assert_eq!(doc.tracks_max_end_tick(), 900);

    // 在 track 1 插入更长音符（end=1200），应增量更新为 1200；track 0 不受影响
    assert!(doc.insert_note(1, NoteEvent::new(600, 1200, 66, 100, 0)));
    assert_eq!(doc.track_max_end_tick(1), 1200);
    assert_eq!(
        doc.track_max_end_tick(0),
        900,
        "track 0 缓存不应被 track 1 串号"
    );
    assert_eq!(doc.tracks_max_end_tick(), 1200);

    // 删除 track 1 的最大音符（index 1，end=1200），缓存置脏后应重算为 500
    assert_eq!(doc.notes[1][1].end_tick, 1200);
    assert!(doc.remove_note(1, 1).is_some());
    assert_eq!(
        doc.track_max_end_tick(1),
        500,
        "删除原 max 后应惰性重算为次大值"
    );

    // track 0 仍独立正确
    assert_eq!(doc.track_max_end_tick(0), 900);
}
