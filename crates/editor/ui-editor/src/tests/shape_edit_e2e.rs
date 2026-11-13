//! 020「编辑图形」端到端：走真实输入链（`handle_action`）+ 屏幕坐标。
//!
//! DEC-001：Bake 后形状对象只读。本文件锁定「已 Bake 图形经真实输入链
//! 也不会进入编辑态、不会改动音符」；pending 的编辑手柄由
//! `interaction::shape_edit_ui` 的单元测试覆盖。

use crate::Editor;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use iced_core::Point;
use lumino_editor_state::{DrawnShapeSource, ShapeKind, ShapeNote};
use lumino_message::Tool;
use lumino_ui_core::message::{EditorAction, Point2};

/// 已 Bake 矩形（200..1200 tick × key 60..64）+ 一条对应文档音符
fn editor_with_baked_shape() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[Note::new(200.0, 60, 1.0)]);
    editor.editor_state.view.snap_precision = 1.0;
    editor.set_canvas_size(Point::new(4000.0, 4000.0));
    editor.set_canvas_offset(Point::new(0.0, 0.0));
    let track = editor.editor_state.data.current_track;
    let id = editor.editor_state.shape_select.add(
        track,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (200.0, 60.0, 1200.0, 64.0),
            shift_constrained: false,
            filled: false,
            apex_high: true,
        },
        vec![ShapeNote {
            track: 1,
            tick: 200.0,
            key: 60,
            length: 1.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.shape_select.select_only(id);
    editor
}

fn press_at(editor: &mut Editor, screen: Point) {
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(screen.x, screen.y),
        shift: false,
        ctrl: false,
    });
}

#[test]
fn test_baked_shape_never_enters_edit_mode() {
    let mut editor = editor_with_baked_shape();
    let notes_before: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();
    assert!(
        !editor.enable_shape_edit(),
        "已 Bake 图形不得进入编辑图形（DEC-001）"
    );
    assert!(!editor.is_shape_edit_armed());
    assert!(editor.shape_edit_box().is_none());

    // 旧的「左上角锚点」位置：现在只应作为普通图形命中选择/拖动，
    // 绝不能进入 020 的锚点拖拽。
    let corner = editor.line_pos_screen_pos((200.0, 64.0));
    press_at(&mut editor, corner);
    assert!(!editor.is_shape_editing(), "不得开始锚点拖拽");
    editor.handle_action(EditorAction::Released);
    let notes_after: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();
    assert_eq!(notes_after, notes_before, "拒绝编辑不得改动任何音符");
    assert!(
        editor.selection_box().is_some(),
        "只读图形仍应保留 ShapeSelect 选框"
    );
}

#[test]
fn test_baked_shape_edit_escape_is_noop() {
    let mut editor = editor_with_baked_shape();
    let notes_before: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();
    editor.handle_action(EditorAction::EscapePressed);
    assert!(!editor.is_shape_edit_armed());
    let notes_after: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();
    assert_eq!(notes_after, notes_before, "Esc 退出编辑态不得改动音符");
}
