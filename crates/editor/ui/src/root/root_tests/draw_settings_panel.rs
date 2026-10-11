//! 音符画「设置按钮」（悬浮条右端齿轮 → **独立对话框窗口**）端到端回归
//!
//! 齿轮是设置类入口，且已从"主窗口内锚定小面板"改为**独立 OS 窗口**
//! （`DialogType::DrawSettings`）。它必须：
//! 1. 点击即发 `window::Event::OpenDrawSettingsDialog`（开窗交给 Runner，不在主窗落浮层）；
//! 2. **不干扰绘制工具状态**——点设置不该顺手把当前工具 / 填充态 / 窗口内设置下拉收掉；
//! 3. 对话框自己的「关闭」→ `DialogResult::Cancel`（Runner 收尾关窗）。
//!
//! 走 `Root::update` 全链路（而非直接调 `Toolbar::update`）：第 1 条要经
//! `ToolbarHandler` 转成窗口事件，第 3 条要经 `DialogHandler`，只测工具栏自身会全部漏掉。

use super::*;
use crate::toolbar::{Event, Tool, ToolPanelItem};

/// 打开悬浮条（齿轮所在容器）的 Root。
fn setup_toolbar_open() -> Root {
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.tool_panel_open = true;
    root
}

/// 断言本次发射的事件里含"打开音符画设置对话框"。
fn assert_emitted_open_dialog() {
    let events = crate::event::take_events();
    let found = events.iter().any(|e| {
        matches!(
            e,
            crate::event::Event::Window(crate::event::window::Event::Dialog(d))
                if matches!(d.as_ref(), crate::event::window::dialog::Event::OpenDrawSettingsDialog)
        )
    });
    assert!(found, "点击齿轮应发射 OpenDrawSettingsDialog 窗口事件");
}

/// 场景 1：齿轮点击 → 发射开窗事件；悬浮条保持打开（给个可见的落点，不闪退）。
#[test]
fn test_gear_emits_open_dialog_window_event() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();

    root.update(Event::open_draw_settings_dialog());

    assert_emitted_open_dialog();
    assert!(
        root.toolbar.tool_panel_open,
        "请求开窗后悬浮条应保持打开（齿轮不托管对话框开关）"
    );
    assert!(root.view_draw_toolbar().is_some(), "悬浮条仍应正常渲染");
}

/// 场景 2（关键）：齿轮是**纯入口**，不改变当前绘制工具与填充共存态。
///
/// 若齿轮退化为"工具条目"，点设置会顺手切走当前工具 —— 这正是把它做成独立按钮的理由。
#[test]
fn test_gear_does_not_disturb_tool_state() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    root.editor.set_tool(Tool::Brush);
    root.toolbar.current_tool = Tool::Brush;
    root.editor.set_fill_enabled(true);
    root.toolbar.fill_enabled = true;

    root.update(Event::open_draw_settings_dialog());

    assert_eq!(
        root.toolbar.current_tool,
        Tool::Brush,
        "点击齿轮不应改变工具栏当前工具"
    );
    assert_eq!(
        root.editor.current_tool(),
        Tool::Brush,
        "点击齿轮不应改变编辑器当前工具"
    );
    assert!(
        root.toolbar.fill_enabled && root.editor.fill_enabled(),
        "点击齿轮不应改动颜料桶填充态"
    );
}

/// 场景 3：齿轮与其它工具栏动作**同一口径**处理窗口内临时浮层（分音符填充面板被收起）。
///
/// 这条曾经反过来写过（"独立窗口不与之争位置，故不应关闭"），但那样会让同一面板在
/// "点工具条目"时关闭、在"点齿轮"时不关，行为不一致。窗口内三块面板本就是
/// "外部一动作即收起"的临时浮层，且输入值存于 `state`、收起不丢数据——
/// 统一收敛到既有语义，不给齿轮开例外。
#[test]
fn test_gear_closes_in_window_panels_like_any_other_action() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    root.editor.set_tool(Tool::Curve);
    root.toolbar.current_tool = Tool::Curve;
    root.editor.editor_state.data.current_track = 1;
    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::FillBucket,
    ));
    assert!(
        root.state.fill_division_dialog.is_open,
        "前置：分音符填充面板已打开"
    );
    let value_before = root.state.fill_division_dialog.value.clone();

    root.update(Event::open_draw_settings_dialog());

    assert_emitted_open_dialog();
    assert!(
        !root.state.fill_division_dialog.is_open,
        "齿轮应与其它工具栏动作同口径收起窗口内浮层"
    );
    assert_eq!(
        root.state.fill_division_dialog.value, value_before,
        "收起面板不得丢掉用户已输入的档位（值存在 state 上）"
    );
}

/// 场景 4：对话框窗口 Root 按 `DialogType::DrawSettings` 渲染，不 panic。
#[test]
fn test_draw_settings_dialog_root_renders() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = Root::new_dialog("dark", DialogType::DrawSettings);
    root.set_draw_settings_dialog_open(true);
    assert_eq!(
        root.state.dialog_type,
        DialogType::DrawSettings,
        "对话框类型应为 DrawSettings（否则 overlay 会落到空容器分支）"
    );

    let _element = root.view();
}

/// 场景 5：对话框内「关闭」→ `DialogResult::Cancel`（Runner 据此收尾关窗）。
#[test]
fn test_dialog_close_action_yields_cancel_result() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = Root::new_dialog("dark", DialogType::DrawSettings);
    assert!(root.state.dialog_result.is_none(), "前置：无待处理结果");

    root.update(Message::DrawSettings(
        lumino_message::DrawSettingsAction::CloseDialog,
    ));

    assert!(
        matches!(
            root.state.dialog_result,
            Some(crate::host::DialogResult::Cancel)
        ),
        "「关闭」应产出 DialogResult::Cancel，实际 {:?}",
        root.state.dialog_result
    );
}

/// 场景 6：对话框内「关闭」不冒发"打开"事件（避免关窗动作把窗口又开回来）。
#[test]
fn test_dialog_close_does_not_emit_open_event() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = Root::new_dialog("dark", DialogType::DrawSettings);
    let _ = crate::event::take_events();

    root.update(Message::DrawSettings(
        lumino_message::DrawSettingsAction::CloseDialog,
    ));

    let events = crate::event::take_events();
    let reopens = events.iter().any(|e| {
        matches!(
            e,
            crate::event::Event::Window(crate::event::window::Event::Dialog(d))
                if matches!(d.as_ref(), crate::event::window::dialog::Event::OpenDrawSettingsDialog)
        )
    });
    assert!(!reopens, "关闭动作不应冒发 OpenDrawSettingsDialog");
}
