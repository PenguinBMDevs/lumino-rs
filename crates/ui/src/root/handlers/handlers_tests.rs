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
