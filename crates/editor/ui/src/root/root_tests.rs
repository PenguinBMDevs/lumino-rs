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
    // 单一权威源：先挂载 document（音符写入 document）
    let doc = crate::test_helpers::make_test_document();
    root.set_midi_document(doc);
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

// ================================================================
// 工程设置重置测试
//
// 修复背景：工程设置（标题/作者/版权/BPM/拍号）属于工程级数据，
// 但存放在程序全局 Root 状态中。关闭工程/新建工程只调用
// clear_editor()，曾遗漏重置 project_settings_dialog，导致旧工程的
// 数据残留到下一个工程。以下测试锁定 reset 行为。
// ================================================================

#[test]
fn test_reset_project_settings_restores_defaults() {
    let ui_config = lumino_core::storage::config::UiConfig::default();
    let mut root = Root::new(&ui_config);

    // 模拟用户在工程设置面板填写了数据
    root.set_project_settings_data(crate::root::ProjectSettingsDialogData {
        title: "我的工程".to_string(),
        tempo: "96".to_string(),
        copyright: "© 2026".to_string(),
        author: "张三".to_string(),
        created_display: "2026-07-01 10:00:00".to_string(),
        total_editing_time_seconds: 3600.0,
        time_signatures: vec![(0, 6, 8)],
    });
    assert_eq!(root.state.project_settings_dialog.title, "我的工程");
    assert_eq!(root.state.project_settings_dialog.tempo, "96");
    assert_eq!(root.state.project_settings_dialog.author, "张三");

    // 关闭工程：工程设置必须恢复默认值，不得残留
    root.reset_project_settings();

    let dialog = &root.state.project_settings_dialog;
    assert!(dialog.title.is_empty(), "关闭工程后标题应为空");
    assert_eq!(dialog.tempo, "120", "关闭工程后 BPM 应恢复默认 120");
    assert!(dialog.copyright.is_empty(), "关闭工程后版权应为空");
    assert!(dialog.author.is_empty(), "关闭工程后作者应为空");
    assert!(
        dialog.created_display.is_empty(),
        "关闭工程后创建日期应为空"
    );
    assert_eq!(
        dialog.total_editing_time_seconds, 0.0,
        "关闭工程后累计创作时间应为 0"
    );
    assert_eq!(
        dialog.time_signature_numerator, "4",
        "关闭工程后拍号分子应恢复默认 4"
    );
    assert_eq!(
        dialog.time_signature_denominator, "4",
        "关闭工程后拍号分母应恢复默认 4"
    );
}

// ================================================================
// 拆分说明（避免单文件超 400 行）：
// - `root_tests/speed_change.rs`：变速按钮 Ctrl+Click 测试
// - `root_tests/sidebar.rs`：右侧栏跟随钢琴卷帘 UI 显隐测试
// - `root_tests/cloud_snapshot.rs`：云存储快照同步边界测试
// ================================================================

mod cloud_snapshot;
mod sidebar;
mod speed_change;
