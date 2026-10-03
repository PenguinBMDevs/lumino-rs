//! 填充桶 Ctrl+单击 → 「分音符填充」弹窗 链路回归测试
//!
//! 用户反馈 BUG：切到油漆桶后按住 Ctrl+单击，工具图标回退为曲线、弹窗不出现。
//! 相关触发路径全部钉死：
//! A) 画布上 Ctrl+单击（设计触发点）→ 弹窗打开 + 油漆桶状态不回退；
//! B) 音符画悬浮工具条上 Ctrl+单击「颜料桶」条目（`ToolPanelItemCtrlSelected`）→
//!    旧主工具栏入口按钮的 Ctrl 行为已**整体迁到悬浮条**；此处应打开「分音符填充」
//!    弹窗而非退化为 `ToolSelected(Curve)`（后者会把已开启的油漆桶打回曲线 = 图标回退根因）。
//! C) 无独立设置的条目（曲线）Ctrl+单击 → 退化为普通选择，不开弹窗。

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

/// 场景 A：画布上 Ctrl+单击 → 弹窗打开，油漆桶状态保持
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
        "画布 Ctrl+单击应打开分音符填充弹窗"
    );
    assert!(root.toolbar.fill_enabled, "工具栏油漆桶不应回退");
    assert!(root.editor.fill_enabled(), "编辑器油漆桶不应回退");
}

/// 场景 B：悬浮条上 Ctrl+单击「颜料桶」条目
///
/// 期望：与画刷/形状的 Ctrl+点击同语义 —— 打开该工具的设置（分音符填充弹窗），
/// 而不是退化为 `ToolSelected(Curve)` 把油漆桶打回曲线（这正是用户看到的
/// 「图标变回曲线工具 + 弹窗不出现」）。
#[test]
fn test_panel_ctrl_click_fill_bucket_opens_dialog() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_paint_bucket();
    root.toolbar.tool_panel_open = true;
    root.update(Message::CtrlKeyChanged(true));

    root.update(Event::tool_panel_item_ctrl_selected(ToolPanelItem::FillBucket));

    assert!(
        root.toolbar.fill_enabled,
        "悬浮条 Ctrl+点颜料桶不应打回曲线（图标回退根因）"
    );
    assert!(root.editor.fill_enabled(), "编辑器油漆桶不应回退");
    assert!(
        root.state.fill_division_dialog.is_open,
        "悬浮条 Ctrl+点颜料桶应打开分音符填充弹窗"
    );
}

/// 场景 C：无独立设置的条目（曲线）Ctrl+单击：退化为普通选择，不开弹窗
#[test]
fn test_panel_ctrl_click_curve_degrades_without_dialog() {
    let _guard = crate::test_helpers::event_queue_lock();
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.set_tool(Tool::Curve);
    root.toolbar.current_tool = Tool::Curve;
    root.editor.set_fill_enabled(false);
    root.toolbar.tool_panel_open = true;
    root.update(Message::CtrlKeyChanged(true));

    root.update(Event::tool_panel_item_ctrl_selected(ToolPanelItem::Curve));

    assert!(
        !root.state.fill_division_dialog.is_open,
        "曲线无独立设置，Ctrl+单击不应开弹窗"
    );
    assert_eq!(
        root.editor.current_tool(),
        Tool::Curve,
        "Ctrl+点曲线条目 = 选择曲线工具"
    );
}
