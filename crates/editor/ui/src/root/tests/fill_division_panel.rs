//! 颜料桶「分音符填充」面板 触发 / 关闭链路回归测试
//!
//! 面板状态 `state.fill_division_dialog` 有两条打开路径：
//! A) 画布上 Ctrl+单击（设计触发点）→ 面板打开 + 油漆桶状态不回退；
//! B) 音符画悬浮工具条上**再次点击已启用的「颜料桶」条目**
//!    （`ToolPanelItemSettingsRequested`）→ 打开面板。旧实现要求「Ctrl+单击」，
//!    用户反馈手势不可发现；且若该事件退化为 `ToolSelected(Curve)`，会把已开启的
//!    油漆桶打回曲线（历史 BUG「图标变回曲线工具 + 弹窗不出现」的根因）。
//! C) 无独立设置的条目（曲线）收到设置请求 → 退化为普通选择，不开面板。
//! D/E) 面板的外部关闭语义（无关工具栏事件先关面板）/ 再次点击保持打开。
//! F) 面板内「关闭填充」→ 面板关闭且填充停用（「再次点击 = 弹面板」语义下
//!    悬浮条上的唯一关闭入口；旧实现靠"再次点击颜料桶 = toggle 关闭"）。

use super::*;
use crate::message::EditorAction;
use crate::toolbar::{Event, Tool, ToolPanelItem};

/// 前置：曲线工具 + 油漆桶已开启（对齐用户「切换到油漆桶工具后」的状态）
fn setup_paint_bucket() -> Root {
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.set_tool(Tool::Curve);
    root.toolbar.current_tool = Tool::Curve;
    root.editor.set_fill_enabled(false);
    // 避开 Conductor 音轨（track 0）的曲线工具拦截，模拟用户在普通音轨上操作
    root.editor.editor_state.data.current_track = 1;
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(
        &mut root,
        Event::tool_panel_item_selected(ToolPanelItem::FillBucket),
    );
    assert!(root.toolbar.fill_enabled, "前置：工具栏油漆桶已开启");
    assert!(root.editor.fill_enabled(), "前置：编辑器油漆桶已开启");
    root
}

/// 场景 A：画布上 Ctrl+单击 → 面板打开，油漆桶状态保持
#[test]
fn test_canvas_ctrl_click_opens_dialog_and_keeps_fill() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_paint_bucket();
    // 给画布一个有效尺寸，让按下事件能进入工具分发
    root.editor.editor_state.canvas.size_x = 2000.0;
    root.editor.editor_state.canvas.size_y = 1200.0;

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
        root.toolbar.tool_panel_open,
        "画布 Ctrl+单击打开分音符填充时，应确保音符画工具箱展开（面板锚定其上）"
    );
    assert!(root.toolbar.fill_enabled, "工具栏油漆桶不应回退");
    assert!(root.editor.fill_enabled(), "编辑器油漆桶不应回退");
}

/// 场景 B：悬浮条上**再次点击**已启用的「颜料桶」条目 —— 全程不按 Ctrl。
///
/// 期望：与画刷/形状同语义 —— 打开该条目的设置面板（分音符填充），
/// 且油漆桶保持开启（不打回曲线）。
#[test]
fn test_panel_second_click_fill_bucket_opens_panel_without_ctrl() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_paint_bucket();
    root.toolbar.tool_panel_open = true;
    assert!(!root.toolbar.ctrl_pressed, "前置：全程未按 Ctrl");

    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::FillBucket,
    ));

    assert!(!root.toolbar.ctrl_pressed, "打开设置面板不得依赖 Ctrl 状态");
    assert!(
        root.toolbar.fill_enabled,
        "再次点击颜料桶不应打回曲线（图标回退根因）"
    );
    assert!(root.editor.fill_enabled(), "编辑器油漆桶不应回退");
    assert!(
        root.state.fill_division_dialog.is_open,
        "再次点击颜料桶应打开分音符填充面板"
    );
}

/// 场景 C：无独立设置的条目（曲线）收到设置请求：退化为普通选择，不开面板
#[test]
fn test_panel_settings_requested_curve_degrades_without_dialog() {
    let _guard = crate::test_helpers::event_queue_lock();
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.set_tool(Tool::Curve);
    root.toolbar.current_tool = Tool::Curve;
    root.editor.set_fill_enabled(false);
    root.toolbar.tool_panel_open = true;

    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::Curve,
    ));

    assert!(
        !root.state.fill_division_dialog.is_open,
        "曲线无独立设置，设置请求不应开面板"
    );
    assert_eq!(
        root.editor.current_tool(),
        Tool::Curve,
        "设置请求落在曲线条目上 = 选择曲线工具"
    );
}

/// 场景 D：面板已打开时，收到无关工具栏事件 → 自动关闭（与画刷/形状下拉同一"外部关闭"语义）。
#[test]
fn test_unrelated_toolbar_event_closes_fill_panel() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_paint_bucket();
    root.toolbar.tool_panel_open = true;

    // 打开分音符填充面板
    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::FillBucket,
    ));
    assert!(
        root.state.fill_division_dialog.is_open,
        "前置：再次点击颜料桶应打开分音符填充面板"
    );

    // 触发一个与颜料桶无关的工具栏事件 → 面板应被关闭
    root.update(Message::Toolbar(Event::ToggleOverflowMenu));
    assert!(
        !root.state.fill_division_dialog.is_open,
        "无关工具栏事件应先关闭分音符填充面板"
    );
}

/// 场景 E：再次点击颜料桶（命中小面板）不应被"外部关闭"守卫误关。
#[test]
fn test_second_click_fill_bucket_keeps_panel_open() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_paint_bucket();
    root.toolbar.tool_panel_open = true;

    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::FillBucket,
    ));
    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::FillBucket,
    ));

    assert!(
        root.state.fill_division_dialog.is_open,
        "再次点击颜料桶应保持面板打开（守卫放行该事件）"
    );
    assert!(root.toolbar.fill_enabled, "颜料桶应保持开启");
}

/// 场景 F：面板内「关闭填充」→ 停用填充且面板收起。
///
/// 「再次点击 = 弹出设置」后，悬浮条上不再有"再点一次即关闭填充"的 toggle 路径，
/// 关闭入口收敛到面板内的「关闭填充」按钮（`FillToggled(false)`）。
#[test]
fn test_fill_panel_disable_button_turns_fill_off() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_paint_bucket();
    root.toolbar.tool_panel_open = true;
    root.update(Event::tool_panel_item_settings_requested(
        ToolPanelItem::FillBucket,
    ));
    assert!(root.state.fill_division_dialog.is_open, "前置：面板已打开");

    root.update(Message::Toolbar(Event::FillToggled(false)));

    assert!(!root.toolbar.fill_enabled, "「关闭填充」应停用工具栏填充态");
    assert!(
        !root.editor.fill_enabled(),
        "「关闭填充」应停用编辑器填充态"
    );
    assert!(
        !root.state.fill_division_dialog.is_open,
        "「关闭填充」后分音符填充面板应收起"
    );
}
