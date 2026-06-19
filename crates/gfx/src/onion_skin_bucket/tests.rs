use lumino_midi_loader::NoteInfo;

use crate::OnionCollectParams;
use crate::onion_skin_bucket::build_bucket_from_notes;

fn make_note_info(start: u32, length: u32, key: u8) -> NoteInfo {
    NoteInfo::new(start, length, key, 100, 0)
}

/// 测试用颜色表：所有音轨返回白色
const TEST_TRACK_COLORS: [u32; 256] = [0xFFFFFFFF; 256];

/// 测试 flatten_with_key_offsets 生成的扁平数组和累积偏移正确
#[test]
fn test_flatten_with_key_offsets() {
    let notes = vec![
        make_note_info(0, 10, 60),
        make_note_info(5, 10, 61),
        make_note_info(20, 10, 60),
    ];
    let bucket = build_bucket_from_notes(&notes, 1);

    let mut note_pool = Vec::new();
    let mut key_offsets = [0u32; 257];
    let colors = [0xFF0000FFu32; 256];
    bucket.flatten_with_key_offsets(&mut note_pool, &mut key_offsets, &colors);

    assert_eq!(note_pool.len(), 3);
    // key 60 的音符应排在 key 61 之前
    assert_eq!(note_pool[0].pitch(), 60);
    assert_eq!(note_pool[1].pitch(), 60);
    assert_eq!(note_pool[2].pitch(), 61);

    // 颜色应被填充
    assert_eq!(note_pool[0].color_packed(), colors[1]);
    assert_eq!(note_pool[2].color_packed(), colors[1]);

    assert_eq!(key_offsets[60], 0);
    assert_eq!(key_offsets[61], 2);
    assert_eq!(key_offsets[62], 3);
    assert_eq!(key_offsets[256], 3);
}

/// 测试 find_visible_range 二分查找结果
#[test]
fn test_find_visible_range() {
    let notes = vec![
        make_note_info(0, 10, 60),  // end=10
        make_note_info(20, 10, 60), // end=30
        make_note_info(50, 10, 60), // end=60
    ];
    let bucket = build_bucket_from_notes(&notes, 1);

    // 视口 [15, 45)：只有第二个音符 start=20 < 45 且 end=30 > 15
    let (start, end) = bucket.find_visible_range(60, 15, 45);
    assert_eq!((start, end), (1, 2));

    // 视口 [0, 15)：第一个音符 end=10 <= 15，第二个 start=20 >= 15
    let (start, end) = bucket.find_visible_range(60, 0, 15);
    assert_eq!((start, end), (0, 1));

    // 空 key
    let (start, end) = bucket.find_visible_range(70, 0, 100);
    assert_eq!((start, end), (0, 0));
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
        &TEST_TRACK_COLORS,
        &mut out,
    );
    assert_eq!(out.len(), 2);

    out.clear();
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(15.0, 25.0, 60, 61, 0.0),
        &mut cursor,
        |_| true,
        &TEST_TRACK_COLORS,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].start_tick, 20);
}

#[test]
fn test_cursor_reuse() {
    // Notes at ticks 0, 100, 200 — all key 60
    let notes = vec![
        make_note_info(0, 10, 60),
        make_note_info(100, 10, 60),
        make_note_info(200, 10, 60),
    ];
    let bucket = build_bucket_from_notes(&notes, 1);

    let mut out = Vec::new();
    let mut cursor = [0usize; 256];

    // Frame 1: ts=0, te=50
    //  - cursor[60]=0, while: note[0].end=10 <= 0? No. cursor stays 0.
    //  - scan from 0: note 0 visible.
    //  - cursor stays 0 (no post-scan advancement).
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(0.0, 50.0, 60, 60, 0.0),
        &mut cursor,
        |_| true,
        &TEST_TRACK_COLORS,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(
        cursor[60], 0,
        "cursor should stay at 0 since note[0].end=10 > ts=0"
    );

    // Frame 2: ts=50, te=150
    //  - cursor[60]=0, while: note[0].end=10 <= 50? Yes → cursor=1.
    //  - while: note[1].end=110 <= 50? No.
    //  - scan from 1: note 100 visible.
    //  - cursor stays 1.
    out.clear();
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(50.0, 150.0, 60, 60, 0.0),
        &mut cursor,
        |_| true,
        &TEST_TRACK_COLORS,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].start_tick, 100);
    assert_eq!(
        cursor[60], 1,
        "cursor should advance to 1 after note[0] is behind viewport"
    );
}

#[test]
fn test_cursor_reset_on_backward() {
    let notes = vec![make_note_info(0, 10, 60), make_note_info(100, 10, 60)];
    let bucket = build_bucket_from_notes(&notes, 1);

    let mut out = Vec::new();
    let mut cursor = [0usize; 256];

    // Frame 1: ts=50, te=150 (forward from beginning)
    //  - cursor[60]=0, while: note[0].end=10 <= 50? Yes → cursor=1.
    //  - scan from 1: note 100 collected.
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(50.0, 150.0, 60, 60, 0.0),
        &mut cursor,
        |_| true,
        &TEST_TRACK_COLORS,
        &mut out,
    );
    assert_eq!(cursor[60], 1);

    // Frame 2: ts=0, te=50 (backward! last_tick_start=50 > ts=0)
    //  - params.tick_start < params.last_tick_start → cursor filled with 0
    out.clear();
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(0.0, 50.0, 60, 60, 50.0),
        &mut cursor,
        |_| true,
        &TEST_TRACK_COLORS,
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
        &TEST_TRACK_COLORS,
        &mut out,
    );
    assert_eq!(out.len(), 0);
}

/// 回归测试：per-key 有序输出不能用于全局 start_tick 二分查找。
///
/// key 60 有音符跨越 tick_end，key 61 的音符全部在 tick_end 内。
/// 若对全局 slice 做 start_tick 二分查找，会误把 key 61 的音符切掉。
/// 本测试保证 collect_visible_with_cursor 会把两个 key 的可见音符都输出，
/// 供 GPU 全量裁剪。
#[test]
fn test_cross_key_collection_not_truncated() {
    let notes = vec![
        make_note_info(0, 200, 60), // 长音符，start=0, end=200
        make_note_info(1, 2, 61),   // 短音符，start=1, end=3
        make_note_info(2, 2, 61),   // 短音符，start=2, end=4
    ];
    let bucket = build_bucket_from_notes(&notes, 1);

    let mut out = Vec::new();
    let mut cursor = [0usize; 256];
    // tick 窗口 [0, 50)：key 60 的长音符与 key 61 的两个短音符都应可见
    bucket.collect_visible_with_cursor(
        OnionCollectParams::new(0.0, 50.0, 60, 61, 0.0),
        &mut cursor,
        |_| true,
        &TEST_TRACK_COLORS,
        &mut out,
    );

    assert_eq!(out.len(), 3, "key 60 和 key 61 的可见音符都应被收集");

    let keys: std::collections::HashSet<u8> = out.iter().map(|n| n.pitch()).collect();
    assert!(keys.contains(&60));
    assert!(keys.contains(&61));
}
