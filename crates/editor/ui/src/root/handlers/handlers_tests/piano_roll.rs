//! 钢琴卷帘演奏指示线与上下文菜单测试

use super::*;

#[test]
fn test_playhead_actions_do_not_change_notes() {
    let mut root = create_root();

    // 演奏指示线移动不应被识别为音符变化
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::Scrubbed { tick: 100.0 }),
        "Scrubbed 不应改变音符"
    );
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::IndicatorDragStart { x: 50.0 }),
        "IndicatorDragStart 不应改变音符"
    );
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::IndicatorDragMove { x: 60.0 }),
        "IndicatorDragMove 不应改变音符"
    );
    assert!(
        !root.handle_editor_action(crate::message::EditorAction::Scrolled {
            delta_x: 10.0,
            delta_y: 0.0,
        }),
        "Scrolled 不应改变音符"
    );
}

#[test]
fn test_piano_roll_context_menu_open_close() {
    let mut root = create_root();

    // 初始状态菜单关闭
    assert!(!root.editor.context_menu.open);
    assert!(root.editor.context_menu.position.is_none());

    // 打开菜单
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Open {
            position: lumino_message::Point2::new(120.0, 80.0),
        },
    ));
    assert!(root.editor.context_menu.open);
    assert_eq!(
        root.editor.context_menu.position,
        Some(iced_core::Point::new(120.0, 80.0))
    );

    // 关闭菜单
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Close,
    ));
    assert!(!root.editor.context_menu.open);
    assert!(root.editor.context_menu.position.is_none());
}

#[test]
fn test_piano_roll_context_menu_item_click_closes_and_dispatches() {
    let mut root = create_root();

    // 打开菜单
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Open {
            position: lumino_message::Point2::new(100.0, 100.0),
        },
    ));
    assert!(root.editor.context_menu.open);

    // 添加一个音符，使全选有意义（document 唯一权威源）
    attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        crate::editor::note::Note::new(0.0, 60, 480.0),
    );

    // 点击全选：菜单关闭且音符被选中
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::ItemClicked(
            lumino_message::PianoRollContextMenuItem::SelectAll,
        ),
    ));
    assert!(!root.editor.context_menu.open);
    assert_eq!(root.editor.editor_state.interaction.selected_notes.len(), 1);
}
