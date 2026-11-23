//! DRAW-049 A：Bake 放置态绑定归属轨。
//!
//! 覆盖复核指出的阻塞缺陷：切轨不清会话、✓ 用 `current_track` 写入、
//! 撤销后把 8 手柄布到新轨。所有断言只认「音符落在哪条轨」与「历史是否新增」。

use super::{draw_rect, test_editor, track_note_count, undo_len};
use lumino_ui_core::message::EditorAction;

#[test]
fn switch_track_clears_placement_and_keeps_pending() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let notes_before = track_note_count(&editor, 1);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.bake_placement_active());

    editor.switch_to_track(2);

    assert_eq!(editor.current_track(), 2);
    assert!(!editor.bake_placement_active(), "切轨后放置态必须收口");
    assert!(!editor.is_shape_edit_armed(), "切轨后 8 手柄不得残留");
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "切轨后黄色选择框不得残留"
    );
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "pending 不得随切轨丢失"
    );
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "pending 预览音符必须保留"
    );
    assert_eq!(
        track_note_count(&editor, 1),
        notes_before,
        "切轨本身不得写任何音符"
    );
}

#[test]
fn confirm_on_other_track_does_not_write_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    let track1_before = track_note_count(&editor, 1);
    let track2_before = track_note_count(&editor, 2);
    let history_before = undo_len(&editor);

    editor.switch_to_track(2);
    editor.handle_action(EditorAction::ShapeToolConfirm);

    assert_eq!(
        track_note_count(&editor, 1),
        track1_before,
        "非归属轨 ✓ 不得写归属轨"
    );
    assert_eq!(
        track_note_count(&editor, 2),
        track2_before,
        "非归属轨 ✓ 不得写当前轨"
    );
    assert_eq!(undo_len(&editor), history_before, "非归属轨 ✓ 不得产生历史");
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "pending 必须原样保留"
    );
    assert!(!editor.bake_placement_active());
}

#[test]
fn return_to_owner_track_can_reenter_and_confirm() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());

    editor.switch_to_track(2);
    editor.handle_action(EditorAction::ShapeToolConfirm);
    assert!(!editor.bake_placement_active(), "非归属轨 ✓ 必须 no-op");

    editor.switch_to_track(1);
    editor.handle_action(EditorAction::ShapeToolConfirm);
    assert!(
        editor.bake_placement_active(),
        "回到归属轨按 ✓ 应重新进入放置态"
    );

    let history_before = undo_len(&editor);
    let track2_before = track_note_count(&editor, 2);
    editor.handle_action(EditorAction::ShapeToolConfirm);

    assert!(!editor.bake_placement_active());
    assert!(
        editor.editor_state.pending_composed.is_empty(),
        "提交后 pending 清空"
    );
    assert!(
        track_note_count(&editor, 1) > 0,
        "音符必须只进归属轨（track 1）"
    );
    assert_eq!(
        track_note_count(&editor, 2),
        track2_before,
        "新轨不得被写入"
    );
    assert_eq!(undo_len(&editor), history_before + 1, "提交应恰好一组历史");
}

#[test]
fn switch_rejected_by_pending_commit_keeps_placement() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());

    // 构造一个在途异步提交：切轨必须被 DEBT-03 守卫拒绝。
    // 这是「exit 放在确认换轨之后」的直接防线——若误放到 early return 之前，
    // 被拒绝的切轨会把用户的放置会话清掉。
    let original = lumino_midi_model::NoteEvent {
        start_tick: 0,
        end_tick: 10,
        key: 60,
        velocity: 100,
        release_velocity: 0,
        channel: 0,
    };
    let moved = lumino_midi_model::NoteEvent {
        start_tick: 1,
        end_tick: 11,
        ..original
    };
    let op = lumino_note_core::history::MoveOp {
        track_id: 1,
        originals: vec![original],
        moved: vec![moved],
        delta_tick: 1,
        delta_key: 0,
        seq: 0,
    };
    assert!(
        editor
            .editor_state
            .data
            .apply_move_ops_async(vec![op], 255)
            .expect("应能启动异步提交"),
        "在途提交应已启动"
    );
    assert!(editor.has_pending_commit());

    editor.switch_to_track(2);
    editor.editor_state.data.cancel_async_commit();

    assert_eq!(editor.current_track(), 1, "在途提交期间切轨应被拒绝");
    assert!(editor.bake_placement_active(), "被拒绝的切轨不得误清放置态");
    assert!(editor.is_shape_edit_armed(), "8 手柄必须保持布防");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
}

#[test]
fn undo_after_commit_keeps_owner_track() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.confirm_bake_placement());
    assert!(editor.editor_state.pending_composed.is_empty());

    editor.switch_to_track(2);
    let track2_before = track_note_count(&editor, 2);
    assert!(editor.undo(), "应能撤销本次 Bake");

    assert_eq!(editor.current_track(), 2, "撤销不得自动跳轨");
    assert!(!editor.bake_placement_active(), "非归属轨不得布防手柄");
    assert!(!editor.is_shape_edit_armed(), "非归属轨不得残留 8 手柄");
    assert_eq!(
        track_note_count(&editor, 2),
        track2_before,
        "pending 不得落到新轨"
    );
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "pending 应随撤销恢复"
    );
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "恢复的 pending 必须带预览音符"
    );

    editor.switch_to_track(1);
    assert!(
        editor.enter_bake_placement_from_pending(),
        "切回归属轨后应能继续放置"
    );
    assert!(editor.bake_placement_active());
}
