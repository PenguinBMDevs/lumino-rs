use lumino_midi_loader::NoteInfo;

use crate::OnionCollectParams;
use crate::onion_skin_bucket::build_bucket_from_notes;

fn make_note_info(start: u32, length: u32, key: u8) -> NoteInfo {
    NoteInfo::new(start, length, key, 100, 0)
}

#[test]
fn test_bucket_build_and_collect() {
    let notes = vec![
        make_note_info(0, 10, 60),
        make_note_info(5, 10, 61),
        make_note_info(20, 10, 60),
    ];
    let bucket = build_bucket_from_notes(&notes, 1);
    assert_eq!(bucket.total_notes(), 3);
    assert_eq!(bucket.key_notes(60).len(), 2);
    assert_eq!(bucket.key_notes(61).len(), 1);

    let mut out = Vec::new();
    let mut cursor = [0usize; 256];
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(0.0, 15.0, 60, 61, 0.0),
        &mut cursor,
        |_| true,
        &mut out,
    );
    assert_eq!(out.len(), 2);

    out.clear();
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(15.0, 25.0, 60, 61, 0.0),
        &mut cursor,
        |_| true,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].start_tick, 20);
}

#[test]
fn test_cursor_reuse() {
    let notes = vec![
        make_note_info(0, 10, 60),
        make_note_info(100, 10, 60),
        make_note_info(200, 10, 60),
    ];
    let bucket = build_bucket_from_notes(&notes, 1);

    let mut out = Vec::new();
    let mut cursor = [0usize; 256];
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(0.0, 50.0, 60, 60, 0.0),
        &mut cursor,
        |_| true,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(cursor[60], 1);

    out.clear();
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(50.0, 150.0, 60, 60, 0.0),
        &mut cursor,
        |_| true,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].start_tick, 100);
    assert_eq!(cursor[60], 2);
}

#[test]
fn test_cursor_reset_on_backward() {
    let notes = vec![make_note_info(0, 10, 60), make_note_info(100, 10, 60)];
    let bucket = build_bucket_from_notes(&notes, 1);

    let mut out = Vec::new();
    let mut cursor = [0usize; 256];
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(50.0, 150.0, 60, 60, 0.0),
        &mut cursor,
        |_| true,
        &mut out,
    );
    assert_eq!(cursor[60], 2);

    out.clear();
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(0.0, 50.0, 60, 60, 50.0),
        &mut cursor,
        |_| true,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].start_tick, 0);
}

#[test]
fn test_update_user_track() {
    let notes = vec![make_note_info(0, 10, 60), make_note_info(100, 10, 61)];
    let mut bucket = build_bucket_from_notes(&notes, 1);

    let user_notes = vec![
        lumino_core::Note::new(50.0, 60, 10.0),
        lumino_core::Note::new(150.0, 62, 10.0),
    ];
    bucket.update_user_track(2, user_notes.iter());
    assert_eq!(bucket.total_notes(), 4);
    assert_eq!(bucket.key_notes(62).len(), 1);

    // 再次更新同一音轨：应该先移除旧音符再添加新音符
    let user_notes2 = vec![lumino_core::Note::new(200.0, 63, 5.0)];
    bucket.update_user_track(2, user_notes2.iter());
    assert_eq!(bucket.total_notes(), 3);
    assert_eq!(bucket.key_notes(62).len(), 0);
    assert_eq!(bucket.key_notes(63).len(), 1);
}

#[test]
fn test_track_filter() {
    let notes = vec![make_note_info(0, 10, 60), make_note_info(5, 10, 61)];
    let bucket = build_bucket_from_notes(&notes, 1);

    let mut out = Vec::new();
    let mut cursor = [0usize; 256];
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(0.0, 20.0, 60, 61, 0.0),
        &mut cursor,
        |_| false,
        &mut out,
    );
    assert_eq!(out.len(), 0);
}
