//! 音符画「设置按钮」（悬浮条右端齿轮 → 总面板）端到端回归
//!
//! 齿轮是**本轮新增的 UI 入口**，它必须：
//! 1. 点击即开合「音符画设置」总面板，且悬浮条本身保持打开；
//! 2. **不干扰绘制工具状态**——点设置不该顺手把当前工具切掉（用户只是想调参数）；
//! 3. 与其余三块设置面板（画刷 / 形状 / 分音符填充）**互斥**，至多一个打开。
//!
//! 走 `Root::update` 全链路（而非直接调 `Toolbar::update`）：第 3 条里分音符填充面板
//! 的状态挂在 `Root::state` 上，只在 `ToolbarHandler` 的「外部关闭」guard 里被收起 ——
//! 只测工具栏自身状态会漏掉这一环。

use super::*;
use crate::toolbar::{Event, Tool, ToolPanelItem};

/// 打开悬浮条（齿轮所在容器）的 Root。
fn setup_toolbar_open() -> Root {
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.tool_panel_open = true;
    root
}

/// 场景 1：齿轮点击 → 总面板打开；再次点击 → 关闭。悬浮条全程保持打开。
#[test]
fn test_gear_toggles_panel_and_keeps_toolbar_open() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    assert!(!root.toolbar.draw_settings_open, "前置：总面板初始关闭");

    root.update(Event::toggle_draw_settings());
    assert!(root.toolbar.draw_settings_open, "点击齿轮应打开总面板");
    assert!(
        root.toolbar.tool_panel_open,
        "打开总面板不应把悬浮条本身关掉（面板锚定其上）"
    );
    // 面板打开时视图必须能构建（锚点悬浮层走 CurveToolGroup，构建失败会 panic）
    assert!(
        root.view_draw_toolbar().is_some(),
        "悬浮条与总面板同时打开时应能渲染出元素"
    );

    root.update(Event::toggle_draw_settings());
    assert!(!root.toolbar.draw_settings_open, "再次点击齿轮应关闭总面板");
}

/// 场景 2：点击面板内空白（`CloseDrawSettings`）→ 总面板关闭。
#[test]
fn test_clicking_panel_blank_closes_draw_settings() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    root.update(Event::toggle_draw_settings());
    assert!(root.toolbar.draw_settings_open);

    root.update(Event::close_draw_settings());
    assert!(
        !root.toolbar.draw_settings_open,
        "CloseDrawSettings（点击面板内空白）应关闭总面板"
    );
}

/// 场景 3（关键）：齿轮是**纯入口**，不改变当前绘制工具与填充共存态。
///
/// 若齿轮退化为"工具条目"，点设置会顺手切走当前工具 —— 这正是本轮把它做成
/// 独立按钮的理由，用测试钉死，避免后续被合并回条目列表。
#[test]
fn test_gear_does_not_disturb_tool_state() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    root.editor.set_tool(Tool::Brush);
    root.toolbar.current_tool = Tool::Brush;
    root.editor.set_fill_enabled(true);
    root.toolbar.fill_enabled = true;

    root.update(Event::toggle_draw_settings());
    assert!(root.toolbar.draw_settings_open);

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

/// 场景 4：分音符填充面板打开时点齿轮 —— 填充面板（`Root::state`）必须被收起。
///
/// 这一环由 `ToolbarHandler::handle_toolbar_event` 的「外部关闭」guard 负责，
/// 不在 `Toolbar::update` 内，故必须走 `Root::update` 才测得到。
#[test]
fn test_gear_closes_fill_division_panel() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    // 前置：曲线 + 颜料桶开启 → 再次点击颜料桶条目打开「分音符填充」面板
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

    root.update(Event::toggle_draw_settings());
    assert!(root.toolbar.draw_settings_open, "齿轮应打开总面板");
    assert!(
        !root.state.fill_division_dialog.is_open,
        "打开总面板应收起分音符填充面板（三块设置面板互斥）"
    );
}

/// 场景 5：反向互斥 —— 总面板打开时弹出工具自带设置，总面板让位。
#[test]
fn test_tool_settings_request_closes_draw_settings() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    root.update(Event::toggle_draw_settings());
    assert!(root.toolbar.draw_settings_open);

    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::Shape,
    ));
    assert!(root.toolbar.shape_dropdown_open, "形状设置下拉应打开");
    assert!(
        !root.toolbar.draw_settings_open,
        "弹出形状设置下拉应关闭总面板"
    );
}

/// 场景 6：关闭悬浮条 → 其上承载的总面板一并收起（否则重开悬浮条会残留面板）。
#[test]
fn test_closing_toolbar_closes_draw_settings() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    root.update(Event::toggle_draw_settings());
    assert!(root.toolbar.draw_settings_open);

    root.update(Message::Toolbar(Event::ToggleToolPanel));
    assert!(
        !root.toolbar.tool_panel_open,
        "再次点击绘制入口应关闭悬浮条"
    );
    assert!(
        !root.toolbar.draw_settings_open,
        "关闭悬浮条应一并收起总面板"
    );
    assert!(
        root.view_draw_toolbar().is_none(),
        "悬浮条关闭后不应再渲染（更不该残留面板）"
    );
}

/// 场景 7（冰山）：**画布 Ctrl+单击**打开分音符填充面板时，总面板也必须让位。
///
/// 这条路径不经过工具栏事件（`Editor` 置请求位 → `Root::open_fill_division_dialog`），
/// 因此**不会被** `Toolbar::update` 的互斥 guard 覆盖。若只改工具栏一侧，用户
/// "先点齿轮、再 Ctrl+单击画布"就会同屏叠出两块面板——视图层的互斥 `debug_assert`
/// 会当场把它变成测试失败。
#[test]
fn test_canvas_ctrl_click_closes_draw_settings() {
    use crate::message::EditorAction;

    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();
    // 前置：曲线 + 颜料桶开启 + 画布有有效尺寸（让按下事件进入工具分发）
    root.editor.set_tool(Tool::Curve);
    root.toolbar.current_tool = Tool::Curve;
    root.editor.set_fill_enabled(true);
    root.toolbar.fill_enabled = true;
    root.editor.editor_state.data.current_track = 1;
    root.editor.editor_state.canvas.size_x = 2000.0;
    root.editor.editor_state.canvas.size_y = 1200.0;
    // 前置：总面板已打开
    root.update(Event::toggle_draw_settings());
    assert!(root.toolbar.draw_settings_open, "前置：总面板已打开");

    root.update(Message::CtrlKeyChanged(true));
    root.update(Message::EditorAction(EditorAction::Pressed {
        pos: crate::message::Point2::new(300.0, 200.0),
        shift: false,
        ctrl: true,
    }));

    assert!(
        root.state.fill_division_dialog.is_open,
        "画布 Ctrl+单击应打开分音符填充面板"
    );
    assert!(
        !root.toolbar.draw_settings_open,
        "画布 Ctrl+单击打开填充面板时，总面板必须让位（互斥）"
    );
    // 两块面板不得同屏：视图层互斥 debug_assert 在 debug 构建下即在此触发
    assert!(root.view_draw_toolbar().is_some());
}
