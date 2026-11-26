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
            target: lumino_message::ContextMenuTarget::Notes,
        },
    ));
    assert!(root.editor.context_menu.open);
    assert_eq!(
        root.editor.context_menu.position,
        Some(lumino_ui_core::geom::Point::new(120.0, 80.0))
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
            target: lumino_message::ContextMenuTarget::Notes,
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

#[test]
fn test_context_menu_shape_list_opens_readonly_panel() {
    use lumino_core::shape::{ShapeKind, ShapeModel};

    let mut root = create_root();
    root.editor.editor_state.data.current_track = 1;
    root.editor.editor_state.canvas.size_x = 1000.0;
    root.editor.editor_state.canvas.size_y = 600.0;
    let model = ShapeModel::new(0, 1, ShapeKind::Polyline, 1.0);
    root.editor.editor_state.shape_store.insert(model, 0);

    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::ItemClicked(
            lumino_message::PianoRollContextMenuItem::ShapeList,
        ),
    ));

    assert!(
        !root.shape_list_model().is_empty(),
        "点击「形状列表」后应打开只读列表（有模型行）"
    );
}

/// 图形目标右键菜单：Open 记录目标（决定渲染条目集），删除项走 DeletePressed 链路
#[test]
fn test_piano_roll_context_menu_drawn_shape_target() {
    let mut root = create_root();

    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Open {
            position: lumino_message::Point2::new(60.0, 40.0),
            target: lumino_message::ContextMenuTarget::DrawnShape,
        },
    ));
    assert!(root.editor.context_menu.open);
    assert_eq!(
        root.editor.context_menu.target,
        lumino_message::ContextMenuTarget::DrawnShape,
        "图形目标应被记录（渲染层据此只显示「删除」）"
    );

    // 无命中图形时点删除：安全退化（不 panic、菜单关闭）
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::ItemClicked(
            lumino_message::PianoRollContextMenuItem::DeleteDrawnShape,
        ),
    ));
    assert!(!root.editor.context_menu.open, "点菜单项应关闭菜单");
}

/// 右键菜单可用性：随音符选区与刀的会话状态变化（置灰判据的唯一来源）。
#[test]
fn test_context_menu_availability_follows_selection_and_knife() {
    let mut root = create_root();
    attach_test_document(&mut root);

    // 先打开菜单（空文档 → 无选区、无刀源、无刀）。
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::Open {
            position: lumino_message::Point2::new(100.0, 100.0),
            target: lumino_message::ContextMenuTarget::Notes,
        },
    ));
    let model = root.piano_roll_context_menu_model().expect("菜单应已打开");
    assert!(!model.availability.has_note_selection);
    assert!(!model.availability.has_knife_source);
    assert!(!model.availability.has_knife);

    // 加入并选中一个音符 → 剪切/复制/删除/批量编辑与设刀都具备前置状态。
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        crate::editor::note::Note::new(0.0, 60, 480.0),
    );
    root.editor.select_all_notes();
    let model = root.piano_roll_context_menu_model().expect("菜单应已打开");
    assert!(model.availability.has_note_selection);
    assert!(model.availability.has_knife_source);

    // 设刀 → 取消刀可用。
    root.editor.mould_set_knife().expect("有选中音符时应能设刀");
    let model = root.piano_roll_context_menu_model().expect("菜单应已打开");
    assert!(model.availability.has_knife);

    // 取消刀 → 回到不可用。
    assert!(root.editor.mould_clear_knife(), "设过刀后应能取消");
    let model = root.piano_roll_context_menu_model().expect("菜单应已打开");
    assert!(!model.availability.has_knife);
}
