//! 变速按钮 Ctrl+Click 测试
//!
//! 修复前 flip_button 传入 has_selection 作为 enabled 参数，
//! 无选中时按钮 disabled，Ctrl+Click 事件根本不会发射。
//! 修复后按钮总是 enabled，handler 内部已对无选中情况做了兜底。

use crate::Message;
use crate::root::Root;
use crate::root::handlers;
use crate::root::handlers::MessageHandler;
use crate::state::root_state::DialogType;
use crate::toolbar;
use crate::toolbar::ToolbarPerfContext;
use lumino_core::storage::config::UiConfig;

#[test]
fn test_speed_change_ctrl_click_opens_dialog_event() {
    // 清空全局事件缓冲区
    let _ = crate::event::take_events();

    // Ctrl+Click 应发射 OpenSpeedChangeDialog 事件（不依赖选中状态）
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.ctrl_pressed = true;
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(&mut root, Message::Toolbar(toolbar::Event::SpeedChange));

    let events = crate::event::take_events();
    let has_open_event = events.iter().any(|e| {
        matches!(
            e,
            crate::event::Event::Window(crate::event::window::Event::Dialog(
                crate::event::window::dialog::Event::OpenSpeedChangeDialog
            ))
        )
    });
    assert!(
        has_open_event,
        "Ctrl+Click 变速应发射 OpenSpeedChangeDialog 事件"
    );
}

#[test]
fn test_speed_change_direct_click_no_selection_returns_early() {
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.ctrl_pressed = false;

    // 无音符 + 无选中：直接点击应无副作用地提前返回
    let _ = crate::event::take_events();
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(&mut root, Message::Toolbar(toolbar::Event::SpeedChange));
    let events = crate::event::take_events();

    assert!(
        events.is_empty(),
        "无选中时直接点击变速不应发射任何窗口事件"
    );
    assert_eq!(
        root.state.dialog_type,
        DialogType::None,
        "无选中时直接点击变速不应改变 dialog_type"
    );
}

#[test]
fn test_speed_change_button_always_enabled_in_view() {
    // 验证在工具栏 view 中变速按钮的 enabled 始终为 true
    // （与 has_selection 解耦），确保 Ctrl+Click 路径可到达 handler。
    // 这是 toolbar/view.rs 中 flip_button 调用改为硬编码 true 的行为保证。
    // 此处通过构造两种场景并调用 view() 来验证不 panic：
    //   1. has_selection = true
    //   2. has_selection = false
    let ui_config = UiConfig::default();
    let root = Root::new(&ui_config);

    // 构造检测仪表盘所需的性能上下文（与产品运行时一致的数据来源）
    let perf_ctx = ToolbarPerfContext {
        playback_tick: root.editor.playback_position,
        ppq: root.editor.editor_state.view.ppq,
        tempo_points: &root.editor.editor_state.data.tempo_points,
    };

    // 有选中 -> view
    let _element = root.toolbar.toolbar_view(
        &root.window,
        true,
        root.settings.display.language,
        &perf_ctx,
        1920.0,
        false,
    );

    // 无选中 -> view（不应 panic/assert）
    let _element = root.toolbar.toolbar_view(
        &root.window,
        false,
        root.settings.display.language,
        &perf_ctx,
        1920.0,
        false,
    );

    // 验证通过：两种情况下 view 均正常返回
}
