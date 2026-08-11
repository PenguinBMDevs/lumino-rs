//! 颜料桶填充集成测试：封闭区域填充、边界点击、开放区域蔓延、模式分流

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;

/// 构造封闭矩形（两条路径围成，snap 固定 480）：
/// P1: (0,60) → (960,60) → (960,62) → (0,62)（顶 + 右 + 底）
/// P2: (0,62) → (0,60)（左侧竖线）
/// 内部格点 = 仅 (480, 61)
fn rect_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 480.0;
    editor.editor_state.line_tool.fill_enabled = true;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (0.0, 62.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
    }
    editor
}

#[test]
fn test_fill_enclosed_rect_generates_notes() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    let data = &editor.editor_state.data;
    assert_eq!(data.current_track_note_count(), 1, "矩形内部仅 1 格");
    let note = &data.current_track_notes()[0];
    assert_eq!(note.start_tick, 480);
    assert_eq!(note.key, 61);
}

#[test]
fn test_fill_enclosed_rect_undoable() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(editor.undo(), "填充操作可撤销");
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    assert!(editor.redo(), "可重做");
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
}

#[test]
fn test_fill_on_boundary_noop() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 1, 0, &[]);
    // 点击边界格点 (0,60) → 不生成
    editor.handle_fill_pressed(Point::new(0.0, 0.0), 0.0, 60);
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    // 点击矩形外（右侧开口方向被底/顶/右边界封闭，但 (1500,61) 在外部可达区域）
    // 外部蔓延同样从空区域开始填充
    editor.handle_fill_pressed(Point::new(0.0, 0.0), 1440.0, 61);
    assert!(
        editor.editor_state.data.current_track_note_count() > 0,
        "外部区域蔓延填充"
    );
}

#[test]
fn test_fill_mode_press_does_not_create_path() {
    // fill_enabled = true 时 pressed 走填充，不创建曲线路径
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.line_tool.fill_enabled = true;
    seed_notes(&mut editor, 1, 0, &[]);
    editor.handle_pressed(Point::new(120.0, 24.0), false);
    assert!(
        editor.editor_state.line_tool.paths.is_empty(),
        "填充模式点击不创建路径"
    );
}

#[test]
fn test_fill_enabled_state_lives_on_line_tool() {
    // 开关状态持久于 line_tool（Editor::set_fill_enabled 读写）
    let mut editor = Editor::new();
    assert!(!editor.fill_enabled());
    editor.set_fill_enabled(true);
    assert!(editor.fill_enabled());
    // 切到非曲线工具自动关闭
    editor.editor_state.tool = Tool::Curve;
    editor.set_tool(Tool::Pointer);
    assert!(!editor.fill_enabled(), "切走曲线工具自动关闭填充");
}
