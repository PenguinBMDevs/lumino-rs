//! 直接 `track_notes_mut` 编辑：索引提示合成按值变更，密度/索引不整建。

use super::*;

#[test]
fn direct_edit_overflow_bbox_uses_ranges_not_full() {
    let before = document();
    let mut after = before.clone();
    stretch_note(&mut after, 1, 0, 160);

    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &before, &MUTED, None, None);
    let merged = state
        .merge_direct_edits(&after, None, &[], true, Some((1, 0, 0)))
        .expect("包围盒必须能合成脏区");
    assert!(!merged.full, "索引提示溢出不得整帧兜底");
    assert!(!merged.ranges.is_empty(), "必须转成脏 tick 区间");
}

#[test]
fn direct_edit_hint_merges_into_incremental_changes() {
    let before = document();
    let mut after = before.clone();
    stretch_note(&mut after, 1, 0, 160);

    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &before, &MUTED, None, None);
    let merged = state
        .merge_direct_edits(&after, None, &[(1, 0)], false, None)
        .expect("索引提示必须能合成脏区");
    assert!(!merged.full, "单索引直接编辑不得升级整帧");
    assert_eq!(merged.changes.len(), 1);
    assert_eq!(merged.changes[0].track, 1);
    assert!(merged.changes[0].old.is_some() && merged.changes[0].new.is_some());

    let output = state
        .update(key(2, 0), &after, &MUTED, Some(merged), Some(&tracks(&[1])))
        .frame
        .expect("增量更新必须生成帧");
    assert_eq!(
        output.pixels,
        reference_pixels(&after, 0),
        "直接编辑增量帧必须与冷启动整帧逐像素一致"
    );
}
