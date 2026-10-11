//! 音符画悬浮条**右端「粗细」按钮 → 全局粗细面板**端到端回归
//!
//! 「粗细」是与齿轮并列的右端设置区入口，区别在于它弹出的是**窗口内锚定面板**
//! （向上弹出、内容 = 全局粗细滑杆 1-100 步进 1）。四条不能破的语义：
//! 1. **开合在主窗内完成、不冒发任何窗口事件**（面板是窗口内浮层，不是独立窗口）；
//! 2. **不改变绘制工具状态**：它是设置入口，不是工具条目；
//! 3. **与其它工具栏动作同口径**：收起窗口内临时浮层（分音符填充面板）；
//! 4. **值是全局的**：滑杆一路走到编辑器（`Editor::brush.thickness` /
//!    `Editor::draw_thickness`），曲线/形状描边与画刷笔画共用同一份。
//!
//! 走 `Root::update` 全链路（而非直接调 `Toolbar::update`）：第 1 / 3 条要经
//! `ToolbarHandler`，第 4 条要经 handler 里 `editor.brush = toolbar.brush.clone()`
//! 的既有同步通道——只测工具栏自身会漏掉"值到不了编辑器"。

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

/// 场景 1：点击「粗细」→ **窗口内**开合面板；不冒发窗口事件、悬浮条照旧打开。
#[test]
fn test_thickness_entry_toggles_in_window_panel_without_window_events() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();

    root.update(Event::toggle_thickness_panel());

    assert!(
        root.toolbar.thickness_panel_open,
        "点击「粗细」应打开窗口内面板"
    );
    assert!(
        !any_window_event(),
        "「粗细」面板是窗口内浮层：不得冒发任何开窗 / 关窗请求"
    );
    assert!(
        root.toolbar.tool_panel_open,
        "点「粗细」不应关闭悬浮工具条（面板锚点在它身上）"
    );
    assert!(root.view_draw_toolbar().is_some(), "悬浮条仍应正常渲染");

    // 再点一次收起（开合语义）
    root.update(Event::toggle_thickness_panel());
    assert!(!root.toolbar.thickness_panel_open, "再次点击应收起面板");
}

/// 场景 2（全局口径，关键）：滑杆值一路同步到编辑器，且是**同一份**值。
///
/// 「粗细全局」的落点在这里：Toolbar 存值 → `ToolbarHandler` 每次事件把
/// `toolbar.brush` 全量 clone 进 `editor.brush`（既有通道）→ 曲线/形状描边与画刷笔画
/// 都从 `Editor` 侧读同一个 `draw_thickness()`。若哪天有人给粗细在别处再存一份，
/// 这条测试会因为两侧不一致而失败。
#[test]
fn test_thickness_slider_value_is_global_in_editor() {
    let _guard = crate::test_helpers::event_queue_lock();
    let mut root = setup_toolbar_open();

    for value in [7_u8, 42, 100, 1] {
        root.update(Event::draw_thickness_changed(value));
        assert_eq!(
            root.toolbar.brush.thickness, value,
            "工具栏侧应存下滑杆值 {value}"
        );
        assert_eq!(
            root.editor.brush.thickness, value,
            "编辑器侧应同步同一份粗细（曲线/形状/画刷共用）"
        );
        assert_eq!(
            root.editor.draw_thickness(),
            value,
            "编辑器读到的全局粗细应与滑杆一致"
        );
    }

    // 画刷命中带宽 = 层数 × 单 key 高（同源；至少 1px）——描边预览现在直接画音符块，
    // 不再引用这个线宽（见 `Editor::shape_outline_blocks`）
    let zoom_y = root.editor.editor_state.view.zoom_y;
    root.update(Event::draw_thickness_changed(10));
    assert!(
        (root.editor.draw_line_width_px() - (10.0 * zoom_y).max(1.0)).abs() < 1e-3,
        "画刷命中带宽应为 层数 × 单 key 高"
    );
}

/// 场景 3（关键）：入口不干扰当前绘制工具与填充共存态。
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

    root.update(Event::toggle_thickness_panel());
    root.update(Event::draw_thickness_changed(5));

    assert_eq!(
        root.toolbar.current_tool,
        Tool::Brush,
        "调粗细不应改变工具栏当前工具"
    );
    assert_eq!(
        root.editor.current_tool(),
        Tool::Brush,
        "调粗细不应改变编辑器当前工具"
    );
    assert!(
        root.toolbar.fill_enabled && root.editor.fill_enabled(),
        "调粗细不应改动颜料桶填充态"
    );
    assert_eq!(
        root.toolbar.tool_panel_offset, offset_before,
        "调粗细不应移动悬浮条"
    );
}

/// 场景 4：与齿轮 / 工具条目**同一口径**收起窗口内临时浮层（分音符填充面板）。
///
/// 四块窗口内面板本就是"外部一动作即收起"的临时浮层，且输入值存于 `state`、
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

    root.update(Event::toggle_thickness_panel());

    assert!(root.toolbar.thickness_panel_open, "粗细面板应打开");
    assert!(
        !root.state.fill_division_dialog.is_open,
        "「粗细」应与其它工具栏动作同口径收起窗口内浮层"
    );
    assert_eq!(
        root.state.fill_division_dialog.value, value_before,
        "收起面板不得丢掉用户已输入的档位（值存在 state 上）"
    );
}
