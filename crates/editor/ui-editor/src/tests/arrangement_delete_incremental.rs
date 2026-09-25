//! 走带批量删除/擦除的**增量路径**回归
//!
//! 背景（用户报告「批量删除会全工程范围重建」）：
//! - 旧实现逐音符 `remove_note` → K 条 `RemoveAt { count: 1 }`，渲染侧每条都做
//!   一次 GPU 尾部搬移 + `update_cull_info`（重建 bind group），等于 K 次全缓冲搬移；
//! - 多轨选择的非当前轨走整轨 `TrackDelta` 重建（逐轨全量构建实例 = 全工程重建）。
//!
//! 修复后断言：
//! - 连续删除段合并为**单条**区间事件；
//! - 当前轨 → `note_delta_events`（主轨段内增量）；
//! - 非当前轨 → `pending_track_remove_ranges`（UI 转 `TrackRemoveRanges` 区间增量）；
//! - 全程不置 `note_delta_dirty`（不触发全量兜底重建）。

use crate::tests::test_helpers;
use crate::{Editor, Note};
use lumino_editor_state::NoteDeltaEvent;

/// 3 轨各 4 音符（tick 0/10/20/30），当前轨 0
fn seed_three_tracks(editor: &mut Editor) {
    let notes: Vec<Note> = (0..4)
        .map(|i| Note::from_raw((i * 10) as f32, 60, 5.0, 100, 0))
        .collect();
    test_helpers::seed_notes(editor, 3, 0, &notes);
    for track in [1usize, 2] {
        for i in 0..4 {
            assert!(
                editor
                    .editor_state
                    .data
                    .insert_note(track, Note::from_raw((i * 10) as f32, 60, 5.0, 100, 0)),
                "种子音符写入其他轨"
            );
        }
    }
    // 种子插入会记录事件（仅当前轨），清空后测量目标操作
    editor.editor_state.data.note_delta_events.clear();
}

/// 全选 3 轨（tick 0..1000）
fn select_all_tracks(editor: &mut Editor) {
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 1000, 0, 127, 0, 2);
}

fn current_remove_ranges(editor: &Editor) -> Vec<(usize, usize)> {
    editor
        .editor_state
        .data
        .note_delta_events
        .iter()
        .filter_map(|e| match e {
            NoteDeltaEvent::RemoveAt { index, count } => Some((*index, *count)),
            _ => None,
        })
        .collect()
}

#[test]
fn arrange_delete_merges_ranges_and_routes_per_track() {
    let mut editor = Editor::default();
    seed_three_tracks(&mut editor);
    select_all_tracks(&mut editor);

    let deleted = editor.arrange_delete_selected_notes();
    assert_eq!(deleted, 12, "3 轨 × 4 音符全部删除");

    let data = &editor.editor_state.data;
    assert!(
        !data.note_delta_dirty,
        "批量删除不得触发全量兜底重建（note_delta_dirty 必须为 false）"
    );
    assert_eq!(
        current_remove_ranges(&editor),
        vec![(0, 4)],
        "当前轨连续段合并为单条区间事件（而非 4 条 count=1）"
    );
    let mut pending: Vec<(usize, Vec<(usize, usize)>)> = data.pending_track_remove_ranges.clone();
    pending.sort_by_key(|(t, _)| *t);
    assert_eq!(
        pending,
        vec![(1, vec![(0, 4)]), (2, vec![(0, 4)])],
        "非当前轨走区间增量队列（而非整轨 TrackDelta 重建）"
    );
    for track in 0..3 {
        assert_eq!(data.track_notes(track).len(), 0, "轨 {track} 应清空");
    }
}

#[test]
fn arrange_erase_merges_ranges_and_routes_per_track() {
    let mut editor = Editor::default();
    seed_three_tracks(&mut editor);

    // 擦除全部 3 轨（视觉位置 0..=2）的 tick 0..1000
    let deleted = editor.arrange_erase(0.0, 1000.0, 0, 2);
    assert_eq!(deleted, 12);

    let data = &editor.editor_state.data;
    assert!(!data.note_delta_dirty, "擦除不得触发全量兜底重建");
    assert_eq!(
        current_remove_ranges(&editor),
        vec![(0, 4)],
        "当前轨合并为单条区间事件"
    );
    let mut pending: Vec<(usize, Vec<(usize, usize)>)> = data.pending_track_remove_ranges.clone();
    pending.sort_by_key(|(t, _)| *t);
    assert_eq!(pending, vec![(1, vec![(0, 4)]), (2, vec![(0, 4)])]);
}

#[test]
fn arrange_delete_no_selection_is_noop() {
    let mut editor = Editor::default();
    seed_three_tracks(&mut editor);
    assert_eq!(editor.arrange_delete_selected_notes(), 0);
    assert!(
        editor
            .editor_state
            .data
            .pending_track_remove_ranges
            .is_empty()
    );
    assert!(editor.editor_state.data.note_delta_events.is_empty());
}
