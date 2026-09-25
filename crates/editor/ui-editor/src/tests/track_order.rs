//! 轨道有序不变式端到端回归（用户可见：结构编辑后音符不得从渲染/命中中消失）
//!
//! 背景：单音符拖动流式路径为性能就地改 tick，移动跨过其它音符后列表失序；
//! `collect_via_window`（渲染可见性）与 `hit_test_note`（命中检测）依赖
//! `window_range` 二分框选窗口——失序会让二分漏检音符，表现为音符**不渲染/
//! 不可点击**（非仅性能退化）。

use crate::tests::test_helpers;
use crate::{EditState, Editor, Note};
use lumino_editor_state::DragState;

fn ticks(editor: &Editor) -> Vec<u32> {
    editor
        .editor_state
        .data
        .track_notes(1)
        .iter()
        .map(|n| n.start_tick)
        .collect()
}

#[test]
fn test_single_drag_crossing_keeps_track_sorted_and_findable() {
    let mut editor = Editor::default();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
            Note::from_raw(960.0, 64, 240.0, 100, 0),
        ],
    );
    // 拖动首个音符 +1000（越过其余两个）
    let count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices([0], count, 0, 60);
    drag.set_delta(1000, 0);
    assert!(editor.finalize_dragging(0, drag));

    assert_eq!(
        ticks(&editor),
        vec![480, 960, 1000],
        "拖动后轨道必须保持升序（渲染/命中二分依赖）"
    );
    // 视口窗口必须覆盖全部音符（修复前：失序导致二分漏检 → 音符不渲染/不可点）
    let (lo, hi) = editor
        .editor_state
        .data
        .track_notes(1)
        .window_range(0, 1100, 0);
    assert_eq!((lo, hi), (0, 3), "window_range 必须框住全部音符");
    // 命中检测窗口：tick 500 处应能框到 480 的音符（失序时二分可能返回空窗口）
    let (hlo, hhi) = editor
        .editor_state
        .data
        .track_notes(1)
        .window_range(500, 501, 240);
    assert!(hlo < hhi, "命中窗口不得为空");
    assert!(
        editor
            .editor_state
            .data
            .track_notes(1)
            .iter_window(hlo, hhi)
            .any(|(_, n)| n.start_tick == 480),
        "命中窗口必须包含 tick 480 的音符"
    );
}

#[test]
fn test_undo_move_keeps_track_sorted() {
    let mut editor = Editor::default();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
            Note::from_raw(960.0, 64, 240.0, 100, 0),
        ],
    );
    // 拖动首个音符 +2000（越过其余两个）并提交历史
    let count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices([0], count, 0, 60);
    drag.set_delta(2000, 0);
    assert!(editor.finalize_dragging(0, drag));
    assert_eq!(ticks(&editor), vec![480, 960, 2000]);

    // undo 回放必须恢复升序（apply_move_ops 就地回改后重排）
    assert!(editor.undo());
    assert_eq!(ticks(&editor), vec![0, 480, 960], "undo 后必须恢复升序");
    assert_eq!(
        editor
            .editor_state
            .data
            .track_notes(1)
            .window_range(0, 1000, 0),
        (0, 3),
        "undo 后窗口必须框住全部音符"
    );
}

// ── 同类路径泛化：走带变速 / 框选拉伸左边缘同样就地改 tick ──

#[test]
fn test_arrange_speed_change_restores_track_order_and_selection() {
    let mut editor = Editor::default();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
            Note::from_raw(960.0, 64, 240.0, 100, 0),
        ],
    );
    // 主选择：选中最后一个（索引 2）——重排后必须跟随原音符
    editor.editor_state.interaction.selected_notes.insert(2);
    // 走带选择：覆盖 tick [0,100) 与 [900,1000)（跳过 480 的音符）
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect(0, 100, 0, 127);
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect(900, 1000, 0, 127);
    // factor 0.25：min=0 → 960 → 240（越过未选中的 480）
    assert_eq!(editor.arrange_apply_speed_change(0.25), 2);
    assert_eq!(ticks(&editor), vec![0, 240, 480], "走带变速后必须恢复升序");
    assert_eq!(
        editor.get_selected_indices(),
        vec![1],
        "重排后主选择必须跟随原音符（新索引 1）"
    );
    // 重排 → 受影响闭区间增量更新（替代全量重建）
    assert!(
        !editor.editor_state.data.note_delta_dirty,
        "重排必须增量更新（受影响区间），不得触发全量重建"
    );
}

#[test]
fn test_resize_start_crossing_keeps_track_sorted() {
    let mut editor = Editor::default();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 480.0, 100, 0),
            Note::from_raw(240.0, 62, 240.0, 100, 0),
        ],
    );
    editor.editor_state.view.snap_precision = 1.0;
    editor.editor_state.interaction.selected_notes.insert(0);
    // 框选拉伸左边缘：+300 → 首个音符 start 0 → 300（越过 240）
    editor.editor_state.interaction.edit_state = EditState::ResizingSelectionStart {
        origin_tick: 0.0,
        last_tick: 300.0,
    };
    editor.handle_released();
    assert_eq!(
        ticks(&editor),
        vec![240, 300],
        "拉伸左边缘越过后必须恢复升序"
    );
}

// ── 选择漂移联动：重排后选中集必须跟随原音符（按 id） ──

#[test]
fn test_speed_change_reorder_keeps_selection_on_same_note() {
    let mut editor = Editor::default();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
            Note::from_raw(960.0, 64, 240.0, 100, 0),
        ],
    );
    editor.editor_state.interaction.selected_notes.insert(0);
    editor.editor_state.interaction.selected_notes.insert(2);
    // min=0；factor 0.25 → 960 → 240（越过未选中的 480）
    assert!(editor.apply_speed_change(0.25) > 0);
    assert_eq!(ticks(&editor), vec![0, 240, 480]);
    // 注意：get_selected_indices 返回 HashSet 迭代序（非确定），断言前排序
    let mut selected = editor.get_selected_indices();
    selected.sort_unstable();
    assert_eq!(
        selected,
        vec![0, 1],
        "重排后选中必须跟随原音符（960 的音符移到索引 1）"
    );
}

#[test]
fn test_batch_edit_tick_reorder_keeps_selection_on_same_note() {
    let mut editor = Editor::default();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
            Note::from_raw(960.0, 64, 240.0, 100, 0),
        ],
    );
    editor.editor_state.interaction.selected_notes.insert(2);
    // tick 表达式 "/4"：960 → 240（越过未选中的 480）
    assert!(editor.apply_batch_edit("", "", "", "/4", 127) > 0);
    assert_eq!(ticks(&editor), vec![0, 240, 480]);
    assert_eq!(
        editor.get_selected_indices(),
        vec![1],
        "重排后选中必须跟随原音符（新索引 1）"
    );
}
