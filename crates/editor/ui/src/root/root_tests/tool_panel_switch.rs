//! 绘制工具面板（曲线工具下拉）选择逻辑测试
//!
//! 背景：用户多次反馈「下拉面板内点工具无法切换」。根因曾有两层：
//! 1) 面板内按钮的 `on_press` 曾被 `mouse_area(...).on_press(Message::Null)` 吞掉；
//! 2) `ToolPanelItemSelected` 事件未同步到编辑器状态，而 `view_arrangement` 每帧用
//!    `editor.current_tool()` 反向覆盖 `toolbar.current_tool`。
//! 这里用单测把「事件 → 工具栏状态 → 编辑器状态」整条链路钉死，避免再靠肉眼回归。

use super::*;
use crate::toolbar::{Event, Tool, ToolPanelItem};

/// 直接走 ToolbarHandler 主入口，验证面板选择事件端到端同步到编辑器
fn select_and_assert(item: ToolPanelItem, expect_tool: Tool, expect_fill: bool) {
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(
        &mut root,
        Message::Toolbar(Event::ToolPanelItemSelected(item)),
    );
    assert_eq!(
        root.editor.current_tool(),
        expect_tool,
        "面板选择 {:?} 必须同步到编辑器 current_tool",
        item
    );
    assert_eq!(
        root.toolbar.current_tool,
        expect_tool,
        "工具栏 current_tool 必须与编辑器一致（{:?}）",
        item
    );
    assert_eq!(
        root.editor.fill_enabled(),
        expect_fill,
        "面板选择 {:?} 的填充状态应为 {}",
        item,
        expect_fill
    );
}

#[test]
fn test_panel_brush_switches_to_brush() {
    select_and_assert(ToolPanelItem::Brush, Tool::Brush, false);
}

#[test]
fn test_panel_shape_switches_to_shape() {
    select_and_assert(ToolPanelItem::Shape, Tool::Shape, false);
}

#[test]
fn test_panel_text_switches_to_text() {
    select_and_assert(ToolPanelItem::Text, Tool::Text, false);
}

#[test]
fn test_panel_eraser_switches_to_eraser() {
    select_and_assert(ToolPanelItem::Eraser, Tool::Eraser, false);
}

#[test]
fn test_panel_fill_bucket_from_brush_switches_to_curve_with_fill() {
    // 填充桶在非曲线/形状工具下点击：应切到曲线 + 开启填充
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.set_tool(Tool::Brush);
    root.toolbar.current_tool = Tool::Brush;
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(
        &mut root,
        Message::Toolbar(Event::ToolPanelItemSelected(ToolPanelItem::FillBucket)),
    );
    assert_eq!(root.editor.current_tool(), Tool::Curve, "填充桶应从画刷切换为曲线");
    assert!(root.editor.fill_enabled(), "填充桶应开启填充");
}

#[test]
fn test_panel_fill_bucket_from_curve_toggles_fill() {
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.set_tool(Tool::Curve);
    root.toolbar.current_tool = Tool::Curve;
    root.editor.set_fill_enabled(false);
    let mut handler = handlers::ToolbarHandler::new();

    handler.handle(
        &mut root,
        Message::Toolbar(Event::ToolPanelItemSelected(ToolPanelItem::FillBucket)),
    );
    assert_eq!(root.editor.current_tool(), Tool::Curve);
    assert!(root.editor.fill_enabled(), "曲线下首次点填充桶应开启填充");

    // 再点一次应关闭填充（仍保持曲线）
    handler.handle(
        &mut root,
        Message::Toolbar(Event::ToolPanelItemSelected(ToolPanelItem::FillBucket)),
    );
    assert_eq!(root.editor.current_tool(), Tool::Curve);
    assert!(!root.editor.fill_enabled(), "曲线下再次点填充桶应关闭填充");
}

#[test]
fn test_toolbar_update_sets_current_tool_before_sync() {
    // 隔离验证第一段：toolbar.update 自身就把 current_tool 设对（sync 之前）
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.tool_panel_open = true;
    root.toolbar
        .update(Event::ToolPanelItemSelected(ToolPanelItem::Brush));
    assert_eq!(
        root.toolbar.current_tool, Tool::Brush,
        "toolbar.update 应把 current_tool 设为 Brush"
    );
}

/// 渲染冒烟测试：render_tool_panel 在当前主题下不应 panic，
/// 间接保证面板结构（图标独占按钮 + 描述条）可正常构建。
#[test]
fn test_render_tool_panel_does_not_panic() {
    use iced_core::Color;
    use lumino_core::storage::config::UiConfig;

    let ui_config = UiConfig::default();
    let root = Root::new(&ui_config);
    let _element = crate::toolbar::tool_panel::render_tool_panel(
        root.toolbar.current_tool,
        root.toolbar.fill_enabled,
        root.settings.display.language,
        Color::from_rgba(0.1, 0.1, 0.1, 1.0),
        &root.window.theme,
    );
}
