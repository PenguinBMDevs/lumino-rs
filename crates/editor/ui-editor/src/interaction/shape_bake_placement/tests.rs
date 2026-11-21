//! DRAW-049：Bake 放置确认态（A+B）测试。

use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{EditState, ShapeKind};

fn test_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.view.snap_precision = 1.0;
    editor.editor_state.line_tool.set_min_note_division(None);
    editor
}

fn draw_rect(editor: &mut Editor) {
    editor.set_shape(ShapeKind::Rectangle);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
}

fn pending_bounds(editor: &Editor) -> (f32, f32, f32, f32) {
    editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .bounds()
        .expect("pending 模型应有几何")
}

fn marquee_edit_state(
    start_tick: f32,
    current_tick: f32,
    start_key: u16,
    current_key: u16,
) -> EditState {
    EditState::Selecting {
        start_tick,
        current_tick,
        start_key,
        current_key,
        start_y: 0.0,
        current_y: 0.0,
    }
}

#[test]
fn bake_enters_placement_without_writing_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let before = editor.editor_state.data.current_track_note_count();

    assert!(
        editor.begin_shape_bake_placement(),
        "单形状 Bake 应进入放置确认态"
    );

    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        before,
        "✓ 前不得写文档"
    );
    assert!(editor.bake_placement_active());
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(editor.is_shape_edit_armed(), "应自动进入 8 手柄编辑态");
    assert!(
        editor.editor_state.shape_tool.shapes.is_empty(),
        "待确认形状应转交给 pending composed"
    );
}

#[test]
fn placement_confirm_writes_notes_and_closes_session() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let before = editor.editor_state.data.current_track_note_count();
    assert!(editor.begin_shape_bake_placement());

    assert!(editor.confirm_bake_placement());
    assert!(
        editor.editor_state.data.current_track_note_count() > before,
        "✓ 应写入音符"
    );
    assert!(!editor.bake_placement_active());
    assert!(editor.editor_state.pending_composed.is_empty());
}

#[test]
fn placement_cancel_discards_without_writing_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let before = editor.editor_state.data.current_track_note_count();
    assert!(editor.begin_shape_bake_placement());

    editor.cancel_bake_placement();
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        before,
        "× 不得写文档"
    );
    assert!(!editor.bake_placement_active());
    assert!(editor.editor_state.pending_composed.is_empty());
}

#[test]
fn undo_after_confirm_restores_placement_then_second_undo_returns_to_pending() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.confirm_bake_placement());

    // 第一次撤销：回到黄色 8 手柄放置态
    assert!(editor.undo(), "应能撤销本次 Bake");
    assert!(
        editor.bake_placement_active(),
        "第一次撤销应回到 8 手柄放置态"
    );
    assert!(editor.is_shape_edit_armed(), "8 手柄应重新布防");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "pending 应保留音符数据"
    );

    // 第二次撤销：退出放置态，回到 pending 图形预览
    assert!(editor.undo(), "第二次撤销应退出放置态");
    assert!(!editor.bake_placement_active());
    assert!(!editor.is_shape_edit_armed(), "不应残留 8 手柄");
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "不应残留不可操作的黄色选择框"
    );
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "pending 预览音符必须保留"
    );
}

#[test]
fn pending_confirm_enters_selection_and_undo_keeps_preview_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.confirm_bake_placement());
    assert!(editor.undo(), "第一次撤销回放置态");
    assert!(editor.undo(), "第二次撤销回 pending");
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "pending 应有预览音符"
    );

    assert!(
        editor.enter_bake_placement_from_pending(),
        "pending 再点 ✓ 应回到选择态，而不是直接 Bake"
    );
    assert!(editor.bake_placement_active());
    assert!(editor.is_shape_edit_armed());
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "进入选择态不得丢预览音符"
    );

    assert!(editor.undo(), "选择态撤销应回到 pending");
    assert!(!editor.bake_placement_active());
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "选择态回到 pending 不得丢预览音符"
    );
}

#[test]
fn exiting_handle_edit_clears_placement_and_keeps_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.bake_placement_active());
    assert!(editor.is_shape_edit_armed());

    // Esc / 双击退出 8 手柄：会话收口，pending 保留
    assert!(editor.exit_shape_edit());
    assert!(!editor.bake_placement_active());
    assert!(!editor.is_shape_edit_armed());
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "退出 8 手柄后不得残留黄色选择框"
    );
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "退出 8 手柄后 pending 音符预览必须保留"
    );

    // 回到 pending 再点 ✓：应重新进入选择态，而不是直接 Bake
    assert!(editor.enter_bake_placement_from_pending());
    assert!(editor.bake_placement_active());
    assert!(editor.is_shape_edit_armed());
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "重新进入选择态不得丢音符"
    );
}

#[test]
fn blank_drag_maps_pending_model_then_blank_click_restores() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let original = {
        assert!(editor.begin_shape_bake_placement());
        pending_bounds(&editor)
    };

    // B 阶段：拉一个更宽的新框
    editor.handle_bake_marquee_released(marquee_edit_state(10.0, 20.0, 60, 64));
    let mapped = pending_bounds(&editor);
    assert!(
        mapped.0 >= 9.9 && mapped.2 <= 20.1,
        "模型应映射到新框：{mapped:?}"
    );
    assert!(
        mapped.2 - mapped.0 > original.2 - original.0,
        "新框更宽，模型宽度应增加"
    );

    // 空白点击（无拖动）→ 复位到 Bake 前
    editor.handle_bake_marquee_released(marquee_edit_state(10.0, 10.0, 60, 60));
    assert_eq!(pending_bounds(&editor), original, "空白点击应复位");
}
