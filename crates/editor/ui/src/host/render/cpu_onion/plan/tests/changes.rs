//! 变更聚合与矩形值语义（`plan/tests.rs` 的子模块，保持每个测试文件 < 400 行）。

use lumino_cpu_onion::CpuOnionRect;
use lumino_editor_state::CpuOnionChange;
use lumino_midi_model::NoteEvent;

use super::super::group_changes;

fn change(track: usize, old: Option<NoteEvent>, new: Option<NoteEvent>) -> CpuOnionChange {
    CpuOnionChange { track, old, new }
}

/// 按轨聚合：同一轨的 old/new 分别进 removed/added，轨序升序。
#[test]
fn group_changes_aggregates_per_track() {
    let a = NoteEvent::new(0, 10, 60, 100, 0);
    let b = NoteEvent::new(20, 30, 62, 100, 0);
    let changes = vec![
        change(2, Some(a), Some(b)),
        change(1, None, Some(a)),
        change(2, Some(b), None),
    ];
    let grouped = group_changes(&changes);
    assert_eq!(grouped.len(), 2, "两轨必须聚合为两组");
    assert_eq!(grouped[0].0, 1);
    assert_eq!(grouped[0].1, Vec::<NoteEvent>::new());
    assert_eq!(grouped[0].2, vec![a]);
    assert_eq!(grouped[1].0, 2);
    assert_eq!(grouped[1].1, vec![a, b], "removed 按记录顺序累加");
    assert_eq!(grouped[1].2, vec![b], "added 只收 new");
}

/// 空变更列表聚合为空（调用方无需特判）。
#[test]
fn group_changes_empty_is_empty() {
    assert!(group_changes(&[]).is_empty());
}

/// 单笔插入（old 为 None）不得产生 removed 项。
#[test]
fn group_changes_insert_has_no_removed() {
    let note = NoteEvent::new(0, 10, 60, 100, 0);
    let grouped = group_changes(&[change(3, None, Some(note))]);
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].0, 3);
    assert!(grouped[0].1.is_empty());
    assert_eq!(grouped[0].2, vec![note]);
}

/// 矩形相等语义（`CpuOnionRect` 值语义，供断言使用）。
#[test]
fn rect_equality_is_value_based() {
    assert_eq!(CpuOnionRect::new(1, 2, 3, 4), CpuOnionRect::new(1, 2, 3, 4));
    assert_ne!(CpuOnionRect::new(1, 2, 3, 4), CpuOnionRect::new(1, 2, 3, 5));
}
