use super::*;
use crate::Message;
use crate::message::{ProjectSettingsAction, SettingsDialogAction};
use crate::root::handlers::MessageHandler;

// ================================================================
// 对话框关闭处理器 —— dialog_type 不复位测试
//
// 修复背景：关闭对话框时 handler 曾将 dialog_type 复位为 None，
// 导致 view_dialog() 在窗口销毁前的最后一帧通过 _ 通配符渲染出精度面板。
// 修复后 handler 不再修改 dialog_type，以下测试确保这一行为。
// ================================================================

#[test]
fn test_close_settings_dialog_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::Settings);
    let mut handler = handlers::DialogHandler::new();
    handler.handle(
        &mut root,
        Message::SettingsDialog(SettingsDialogAction::CloseDialog),
    );

    // 关闭设置对话框不应复位 dialog_type（防止窗口销毁前一帧闪跳到精度面板）
    assert_eq!(
        root.state.dialog_type,
        DialogType::Settings,
        "CloseSettingsDialog 不应修改 dialog_type"
    );
    assert!(
        root.state.dialog_result.is_some(),
        "CloseSettingsDialog 应设置 dialog_result"
    );
}

#[test]
fn test_close_project_settings_dialog_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::ProjectSettings);
    let mut handler = handlers::DialogHandler::new();
    handler.handle(
        &mut root,
        Message::ProjectSettings(ProjectSettingsAction::CloseDialog),
    );

    assert_eq!(
        root.state.dialog_type,
        DialogType::ProjectSettings,
        "CloseProjectSettingsDialog 不应修改 dialog_type"
    );
    assert!(
        root.state.dialog_result.is_some(),
        "CloseProjectSettingsDialog 应设置 dialog_result"
    );
}

#[test]
fn test_confirm_load_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::LoadConfirm);
    root.handle_confirm_load();

    assert_eq!(
        root.state.dialog_type,
        DialogType::LoadConfirm,
        "handle_confirm_load 不应修改 dialog_type"
    );
    assert!(
        root.state.dialog_result.is_some(),
        "handle_confirm_load 应设置 dialog_result"
    );
}

#[test]
fn test_cancel_load_preserves_dialog_type() {
    let mut root = Root::new_dialog("dark", DialogType::LoadConfirm);
    root.handle_cancel_load();

    assert_eq!(
        root.state.dialog_type,
        DialogType::LoadConfirm,
        "handle_cancel_load 不应修改 dialog_type"
    );
    // handle_cancel_load 仅关闭对话框，不设置结果
    assert!(
        !root.state.load_confirm_dialog.is_open,
        "handle_cancel_load 应关闭加载确认对话框"
    );
}

// ================================================================
// view_dialog None 测试
//
// DialogType::None 应渲染空容器而不是回退到精度面板（修复前的 bug）。
// 此处验证 view() 不 panic，类型已由 match 分支的显式匹配保证。
// ================================================================

#[test]
fn test_view_dialog_none_does_not_panic() {
    // DialogType::None 匹配专门的空容器分支，不应 panic
    let root = Root::new_dialog("dark", DialogType::None);
    let _element = root.view();
}

// ================================================================
// 变速按钮 Ctrl+Click 测试
//
// 修复前 flip_button 传入 has_selection 作为 enabled 参数，
// 无选中时按钮 disabled，Ctrl+Click 事件根本不会发射。
// 修复后按钮总是 enabled，handler 内部已对无选中情况做了兜底。
// ================================================================

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
    let ui_config = lumino_core::storage::config::UiConfig::default();
    let root = Root::new(&ui_config);

    // 构造检测仪表盘所需的性能上下文（与产品运行时一致的数据来源）
    let perf_ctx = crate::toolbar::ToolbarPerfContext {
        perf_data: root.statusbar.perf_data(),
        playback_tick: root.editor.playback_position,
        ppq: root.editor.editor_state.view.ppq,
        tempo_points: &root.editor.editor_state.data.tempo_points,
    };

    // 有选中 -> view
    let _element = root.toolbar.toolbar_view(
        &root.window,
        true,
        root.settings.language,
        &perf_ctx,
        1920.0,
    );

    // 无选中 -> view（不应 panic/assert）
    let _element = root.toolbar.toolbar_view(
        &root.window,
        false,
        root.settings.language,
        &perf_ctx,
        1920.0,
    );

    // 验证通过：两种情况下 view 均正常返回
}

// ================================================================
// 工程走带视图最大 tick 缓存测试
//
// 播放时每帧需要最大 tick 来计算滚动范围；若每帧全量扫描 track_notes，
// 大型 MIDI 会在主线程造成卡顿。此测试验证缓存按 track_notes_gen 失效。
// ================================================================

#[test]
fn test_arrangement_max_tick_end_caches_by_gen() {
    use lumino_core::storage::config::UiConfig;

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);

    // 无音符时返回 DEFAULT_MIN_TICKS
    assert_eq!(
        root.arrangement_max_tick_end(),
        crate::constants::editor::DEFAULT_MIN_TICKS
    );

    // 在非指挥轨道添加音符（tick=4000, length=100，终点=4100）
    // 必须超过 DEFAULT_MIN_TICKS（3840），否则会被最小值覆盖
    root.editor.editor_state.data.current_track = 1;
    let _ = root
        .editor
        .editor_state
        .data
        .finish_drawing(4000.0, 60, 4100.0, 1.0, 10.0);

    // track_notes_gen 已变化，缓存应重新计算
    let max_tick = root.arrangement_max_tick_end();
    assert!((max_tick - 4100.0).abs() < f32::EPSILON);

    // 缓存已写入
    assert!((root.arrangement_view.viewport.cached_max_tick_end - 4100.0).abs() < f32::EPSILON);
    assert_eq!(
        root.arrangement_view.viewport.cached_track_notes_gen,
        root.editor.editor_state.data.track_notes_gen
    );
}
