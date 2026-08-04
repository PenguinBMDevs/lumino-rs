use super::*;
use crate::message::{CustomPrecisionAction, Message};
use crate::root::Root;
use lumino_core::storage::config::UiConfig;

fn create_root() -> Root {
    Root::new(&UiConfig::default())
}

#[test]
fn test_message_router_consumes_message() {
    struct ConsumingHandler;
    impl MessageHandler for ConsumingHandler {
        fn handle(&mut self, _root: &mut Root, _msg: Message) -> Option<Message> {
            None
        }
    }

    let mut router = MessageRouter::new();
    router.register(Box::new(ConsumingHandler));
    let mut root = create_root();

    // 不应 panic；消息被消费后不再继续传递
    router.route(&mut root, Message::ToggleSettings);
}

#[test]
fn test_message_router_falls_through_when_not_consumed() {
    use std::cell::RefCell;
    use std::rc::Rc;

    struct PassThroughHandler;
    impl MessageHandler for PassThroughHandler {
        fn handle(&mut self, _root: &mut Root, msg: Message) -> Option<Message> {
            Some(msg)
        }
    }

    let received: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    struct CapturingHandler {
        received: Rc<RefCell<bool>>,
    }
    impl MessageHandler for CapturingHandler {
        fn handle(&mut self, _root: &mut Root, _msg: Message) -> Option<Message> {
            *self.received.borrow_mut() = true;
            None
        }
    }

    let mut router = MessageRouter::new();
    let received2 = Rc::clone(&received);
    router.register(Box::new(PassThroughHandler));
    router.register(Box::new(CapturingHandler {
        received: received2,
    }));

    let mut root = create_root();
    router.route(&mut root, Message::ToggleSettings);

    // 由于第一个 handler 返回 Some，消息应落到第二个 handler
    assert!(*received.borrow(), "消息应透传到第二个处理器");
}

#[test]
fn test_collaboration_handler_opens_dialog() {
    let mut handler = CollaborationHandler::new();
    let mut root = create_root();

    assert!(!root.state.collaboration_dialog.is_open);
    handler.handle(
        &mut root,
        Message::Collaboration(lumino_message::CollaborationAction::OpenDialog),
    );
    assert!(root.state.collaboration_dialog.is_open);
}

#[test]
fn test_dialog_handler_opens_custom_precision() {
    let mut handler = DialogHandler::new();
    let mut root = create_root();

    let _ = crate::event::take_events();
    let result = handler.handle(
        &mut root,
        Message::CustomPrecision(CustomPrecisionAction::OpenDialog),
    );
    assert!(result.is_none(), "处理器应消费消息");

    let emitted = crate::event::take_events();
    let has_open_event = emitted.iter().any(|e| {
        matches!(
            e,
            crate::event::Event::Window(crate::event::window::Event::Dialog(
                crate::event::window::dialog::Event::OpenCustomPrecisionDialog
            ))
        )
    });
    assert!(has_open_event, "应发射 OpenCustomPrecisionDialog 窗口事件");
}

#[test]
fn test_toolbar_handler_play_creates_manager() {
    let mut handler = ToolbarHandler::new();
    let mut root = create_root();

    // 添加一个音符，使播放管理器能够初始化
    root.editor
        .editor_state
        .data
        .notes
        .push_back(crate::editor::note::Note::new(0.0, 60, 480.0));

    assert!(root.playback.manager.is_none());
    handler.handle(&mut root, Message::Toolbar(crate::toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "Play 消息应创建播放管理器");
    assert!(root.toolbar.is_playing);
}

#[test]
fn test_handle_core_event_re_emits_event() {
    let mut root = create_root();

    // 清空已有事件
    let _ = crate::event::take_events();

    let event = crate::event::Event::menu_file(crate::event::menu::file::Event::New);
    root.handle_core_event(event.clone());

    let emitted = crate::event::take_events();
    assert!(
        emitted
            .iter()
            .any(|e| format!("{:?}", e) == format!("{:?}", event)),
        "handle_core_event 应重新发出传入的事件"
    );
}

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

    // 添加一个音符，使全选有意义
    root.editor
        .editor_state
        .data
        .notes
        .push_back(crate::editor::note::Note::new(0.0, 60, 480.0));

    // 点击全选：菜单关闭且音符被选中
    root.update(Message::PianoRollContextMenu(
        lumino_message::PianoRollContextMenuAction::ItemClicked(
            lumino_message::PianoRollContextMenuItem::SelectAll,
        ),
    ));
    assert!(!root.editor.context_menu.open);
    assert_eq!(root.editor.editor_state.interaction.selected_notes.len(), 1);
}

// ===== 力度面板双向滚轮测试 =====

use crate::editor::velocity::EditMode;
use crate::message::VelocityAction;

/// 双向滚轮（对角线）：水平分量滚动时间轴，垂直分量滚动自动化曲线，同时生效
#[test]
fn test_velocity_wheel_scrolled_bidirectional() {
    let mut root = create_root();
    // 水平滚动需要横向内容空间（与网格测试一致）
    root.editor.editor_state.canvas.size_x = 1000.0;
    // 垂直滚动需要 zoom > 1 才有滚动余量（默认 zoom=1.0 时可见范围=满量程，会被 clamp 到 0）
    root.editor.velocity_panel.value_zoom = 2.0;
    root.editor.velocity_panel.edit_mode = EditMode::Cc(1);

    let before_x = root.editor.editor_state.view.smooth_scroll.target_x;
    VelocityHandler::new().handle(
        &mut root,
        Message::Velocity(VelocityAction::WheelScrolled {
            delta_x: -100.0,
            delta_y: -1.0, // 上滑 → 自动化曲线 value_scroll 增大
        }),
    );

    // 水平：左滑 → scroll_x 增大（内容跟随手指）
    assert!(
        root.editor.editor_state.view.smooth_scroll.target_x > before_x,
        "水平分量应滚动时间轴，target_x={}",
        root.editor.editor_state.view.smooth_scroll.target_x
    );
    // 垂直：自动化曲线滚动（CC 模式生效）
    assert!(
        root.editor.velocity_panel.value_scroll > 0.0,
        "垂直分量应滚动自动化曲线，value_scroll={}",
        root.editor.velocity_panel.value_scroll
    );
}

/// 双向滚轮：Velocity 模式垂直分量不生效（保持无操作语义），水平分量仍生效
#[test]
fn test_velocity_wheel_scrolled_vertical_ignored_in_velocity_mode() {
    let mut root = create_root();
    root.editor.editor_state.canvas.size_x = 1000.0;
    root.editor.velocity_panel.edit_mode = EditMode::Velocity;

    let before_x = root.editor.editor_state.view.smooth_scroll.target_x;
    VelocityHandler::new().handle(
        &mut root,
        Message::Velocity(VelocityAction::WheelScrolled {
            delta_x: -100.0,
            delta_y: 1.0,
        }),
    );

    assert_eq!(
        root.editor.velocity_panel.value_scroll, 0.0,
        "Velocity 模式垂直分量应被忽略"
    );
    assert!(
        root.editor.editor_state.view.smooth_scroll.target_x > before_x,
        "水平分量仍应滚动时间轴"
    );
}
