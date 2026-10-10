//! 音符画悬浮条**右端「粗细」按钮**链路（本轮范围 = 入口 + 图标）端到端回归
//!
//! 「粗细」是与齿轮并列的右端设置区入口。本轮只交付**入口**：按钮渲染 + 点击发
//! `Event::ThicknessSettingsRequested`，粗细控件（滑杆 / 步进）下一轮接入。
//! 因此这一轮的验收口径是"**入口干净**"，而不是"能改到粗细"——四条不能破的语义：
//! 1. **不发窗口事件**：本轮不接落地，点它不得冒发任何开窗 / 关窗请求
//!    （否则用户会看到"什么都没发生"却已经有窗口请求飞出）；
//! 2. **不改变绘制工具状态**：它是设置入口，不是工具条目；
//! 3. **与其它工具栏动作同口径**：收起窗口内临时浮层（分音符填充面板）；
//! 4. **悬浮条保持打开**：入口在悬浮条上，点它不托管悬浮条开关。
//!
//! 走 `Root::update` 全链路（而非直接调 `Toolbar::update`）：第 1 / 3 条要经
//! `ToolbarHandler`，只测工具栏自身会漏掉。

use super::*;
use crate::toolbar::{Event, Tool, ToolPanelItem};

/// 打开悬浮条（「粗细」按钮所在容器）的 Root。
fn setup_toolbar_open() -> Root {
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.tool_panel_open = true;
    root
}

/// 本次发射的事件里是否含**任何窗口事件**（开窗 / 关窗都算）。
fn any_window_event() -> bool {
    crate::event::take_events()
        .iter()
        .any(|e| matches!(e, crate::event::Event::Window(_)))
}

/// 场景 1：点击「粗细」→ 本轮**不落任何窗口请求**，悬浮条照旧打开、照旧可渲染。
///
/// 反过来说：下一轮接入落地时，这条测试就是"入口真的接上了"的第一处改动点
/// （届时改为断言目标窗口 / 面板事件）。
#[test]
fn test_thickness_entry_emits_no_window_event_yet() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();

    root.update(Event::thickness_settings_requested());

    assert!(
        !any_window_event(),
        "本轮「粗细」只有入口：不得冒发窗口事件（落地由下一轮接入）"
    );
    assert!(
        root.toolbar.tool_panel_open,
        "点「粗细」不应关闭悬浮工具条（入口在悬浮条上，不托管其开关）"
    );
    assert!(root.view_draw_toolbar().is_some(), "悬浮条仍应正常渲染");
}

/// 场景 2（关键）：入口不干扰当前绘制工具与填充共存态。
///
/// 若「粗细」退化成"工具条目"（复用了 `ToolPanelItemSelected`），点设置就会顺手切走
/// 用户正在用的绘制工具——这正是把它做成**专属事件 + 右端设置区**的理由。
#[test]
fn test_thickness_entry_does_not_disturb_tool_state() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    root.editor.set_tool(Tool::Brush);
    root.toolbar.current_tool = Tool::Brush;
    root.editor.set_fill_enabled(true);
    root.toolbar.fill_enabled = true;
    let offset_before = root.toolbar.tool_panel_offset;

    root.update(Event::thickness_settings_requested());

    assert_eq!(
        root.toolbar.current_tool,
        Tool::Brush,
        "点「粗细」不应改变工具栏当前工具"
    );
    assert_eq!(
        root.editor.current_tool(),
        Tool::Brush,
        "点「粗细」不应改变编辑器当前工具"
    );
    assert!(
        root.toolbar.fill_enabled && root.editor.fill_enabled(),
        "点「粗细」不应改动颜料桶填充态"
    );
    assert_eq!(
        root.toolbar.tool_panel_offset, offset_before,
        "点「粗细」不应移动悬浮条"
    );
}

/// 场景 3：与齿轮 / 工具条目**同一口径**收起窗口内临时浮层（分音符填充面板）。
///
/// 三块窗口内面板本就是"外部一动作即收起"的临时浮层，且输入值存于 `state`、
/// 收起不丢数据——不给新入口单开例外（同一动作下面板行为不一致是最难解释的一类 BUG）。
#[test]
fn test_thickness_entry_closes_in_window_panels_like_any_other_action() {
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

    root.update(Event::thickness_settings_requested());

    assert!(
        !root.state.fill_division_dialog.is_open,
        "「粗细」应与其它工具栏动作同口径收起窗口内浮层"
    );
    assert_eq!(
        root.state.fill_division_dialog.value, value_before,
        "收起面板不得丢掉用户已输入的档位（值存在 state 上）"
    );
}
