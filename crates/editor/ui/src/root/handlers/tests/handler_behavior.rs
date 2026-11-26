//! 各消息处理器行为测试（协作对话框 / 自定义精度对话框 / 播放管理器 / 核心事件转发）

use super::*;
use crate::message::CustomPrecisionAction;

#[test]
fn test_collaboration_handler_opens_dialog() {
    let _guard = crate::test_helpers::event_queue_lock();
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
    let _guard = crate::test_helpers::event_queue_lock();
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
            crate::event::Event::Window(crate::event::window::Event::Dialog(d))
                if matches!(d.as_ref(), crate::event::window::dialog::Event::OpenCustomPrecisionDialog)
        )
    });
    assert!(has_open_event, "应发射 OpenCustomPrecisionDialog 窗口事件");
}

#[test]
fn test_toolbar_handler_play_creates_manager() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut handler = ToolbarHandler::new();
    let mut root = create_root();

    // 添加一个音符，使播放管理器能够初始化（document 唯一权威源）
    attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        crate::editor::note::Note::new(0.0, 60, 480.0),
    );

    assert!(root.playback.manager.is_none());
    handler.handle(&mut root, Message::Toolbar(crate::toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "Play 消息应创建播放管理器");
    assert!(root.toolbar.is_playing);
}

#[test]
fn test_transport_skip_steps_measure_and_ctrl_jumps_to_edges() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut handler = ToolbarHandler::new();
    let mut root = create_root();

    let mut doc = crate::test_helpers::make_test_document();
    doc.total_ticks = 7680;
    root.set_midi_document(doc);
    root.editor.editor_state.view.ppq = 1920;
    root.editor.playback_position = 0.0;

    use crate::toolbar::Event;

    // 单点：严格跳到下一条小节线（4/4 @ PPQ 1920 = 7680 tick）。
    handler.handle(&mut root, Message::Toolbar(Event::SkipForward));
    assert_eq!(root.editor.playback_position, 7680.0);
    handler.handle(&mut root, Message::Toolbar(Event::SkipForward));
    assert_eq!(root.editor.playback_position, 15360.0);

    // 单点后退：严格回到上一条小节线。
    handler.handle(&mut root, Message::Toolbar(Event::SkipBackward));
    assert_eq!(root.editor.playback_position, 7680.0);
    handler.handle(&mut root, Message::Toolbar(Event::SkipBackward));
    assert_eq!(root.editor.playback_position, 0.0);

    // Ctrl：跳到全局 EOT / 全曲开头。
    handler.handle(&mut root, Message::Toolbar(Event::SkipForwardToEnd));
    assert_eq!(root.editor.playback_position, 7680.0);
    handler.handle(&mut root, Message::Toolbar(Event::SkipBackwardToStart));
    assert_eq!(root.editor.playback_position, 0.0);

    // 快退阈值：本小节前 1/2 退上一小节，后 1/2（含正中）回本小节开头。
    // 4/4 @ PPQ 1920 一小节 7680，半小节 3840。
    root.editor.playback_position = 7680.0 + 100.0;
    handler.handle(&mut root, Message::Toolbar(Event::SkipBackward));
    assert_eq!(root.editor.playback_position, 0.0, "前 1/2 应退到 0");
    root.editor.playback_position = 7680.0 + 3840.0;
    handler.handle(&mut root, Message::Toolbar(Event::SkipBackward));
    assert_eq!(root.editor.playback_position, 7680.0, "正中回本小节开头");
}

#[test]
fn test_handle_core_event_re_emits_event() {
    let _guard = crate::test_helpers::event_queue_lock();
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
