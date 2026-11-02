//! `TrackIndex` 聚合 / 掩码 / 上限回归（`index.rs` 的子模块，守住单文件 <400 行）。

use lumino_midi_model::{MidiDocument, NoteEvent};

use super::{MAX_MEASURE_BUCKETS, TrackIndex};

fn document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(0, NoteEvent::new(0, 100, 60, 100, 0));
    // 600 tick > LOOKBACK_TICKS，进入 long_notes。
    document.insert_note(0, NoteEvent::new(100, 700, 62, 100, 0));
    document.insert_note(1, NoteEvent::new(200, 300, 64, 100, 0));
    document
}

#[test]
fn build_aggregates_without_copying_notes() {
    let index = TrackIndex::build(&document());
    assert_eq!(index.max_len, vec![600, 100]);
    assert_eq!(index.long_note_count, 1);
    assert_eq!(index.long_notes(0).len(), 1);
    assert!(index.long_notes(0).iter().any(|note| note.key == 62));
    assert_eq!(index.measure_ticks, 1920);
    // 三个音符都落在第 0 小节。
    assert_eq!(index.measure_counts[0], 3);
    assert_eq!(index.densest_measure, 0);
    assert_eq!(index.densest_measure_notes, 3);
    assert_eq!(index.total_notes, 3);
    assert_eq!(index.track_note_counts, vec![2, 1]);
}

#[test]
fn estimate_visible_uses_measure_buckets() {
    let index = TrackIndex::build(&document());
    assert_eq!(index.estimate_visible(0, 1919), 3);
    assert_eq!(index.estimate_visible(1920, 10_000), 0);
    assert_eq!(index.estimate_visible(200, 100), 0);
}

#[test]
fn measure_counts_are_capped_for_extreme_ticks() {
    let mut document = MidiDocument::empty_with_tracks(1, 1);
    document.insert_note(0, NoteEvent::new(u32::MAX - 1, u32::MAX, 60, 100, 0));
    let index = TrackIndex::build(&document);
    assert!(index.measure_counts.len() <= MAX_MEASURE_BUCKETS);
    assert_eq!(index.total_notes, 1);
    // 正常 tick 仍可估算；超限 tick 返回 0 而不是巨量分配。
    assert_eq!(index.estimate_visible(u32::MAX - 1, u32::MAX), 0);
}

#[test]
fn active_keys_excludes_current_and_muted_tracks() {
    let index = TrackIndex::build(&document());
    let onion_only = index.active_keys(0, 1000, 1, &[]);
    assert_ne!(onion_only & (1u128 << 60), 0);
    assert_ne!(onion_only & (1u128 << 62), 0);
    assert_eq!(onion_only & (1u128 << 64), 0, "当前轨不得计入覆盖目标");

    let muted_track = index.active_keys(0, 1000, usize::MAX, &[true, false]);
    assert_eq!(muted_track & (1u128 << 60), 0, "静音轨不得计入覆盖目标");
    assert_ne!(muted_track & (1u128 << 64), 0);
}

#[test]
fn inserted_note_extends_key_mask_removed_note_is_safe() {
    let index = TrackIndex::build(&document());
    let inserted = NoteEvent::new(0, 10, 100, 100, 0);
    let next = index.with_changes_fast(0, &[], &[inserted]);
    assert_ne!(
        next.active_keys(0, 1000, usize::MAX, &[]) & (1u128 << 100),
        0,
        "插入必须扩展 key 掩码"
    );
    let removed = next.with_changes_fast(0, &[inserted], &[]);
    assert_ne!(
        removed.active_keys(0, 1000, usize::MAX, &[]) & (1u128 << 100),
        0,
        "删除保留陈旧位只会让目标偏大，不得提前跳过新像素"
    );
}
