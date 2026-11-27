//! 绘制工具面板（曲线工具下拉）选择逻辑测试
//!
//! 背景：用户多次反馈「下拉面板内点工具无法切换」。根因曾有两层：
//! 1) 面板内按钮的 `on_press` 曾被 `mouse_area(...).on_press(Message::Null)` 吞掉；
//! 2) `ToolPanelItemSelected` 事件未同步到编辑器状态，而 `view_arrangement` 每帧用
//!    `editor.editor_state.current_tool()` 反向覆盖 `toolbar.current_tool`。
//!
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
        root.editor.editor_state.current_tool(),
        expect_tool,
        "面板选择 {:?} 必须同步到编辑器 current_tool",
        item
    );
    assert_eq!(
        root.toolbar.current_tool, expect_tool,
        "工具栏 current_tool 必须与编辑器一致（{:?}）",
        item
    );
    assert_eq!(
        root.editor.editor_state.line_tool.fill_enabled, expect_fill,
        "面板选择 {:?} 的填充状态应为 {}",
        item, expect_fill
    );
}

#[test]
fn test_panel_brush_switches_to_brush() {
    let _guard = crate::test_helpers::event_queue_lock();
    select_and_assert(ToolPanelItem::Brush, Tool::Brush, false);
}

#[test]
fn test_panel_shape_switches_to_shape() {
    let _guard = crate::test_helpers::event_queue_lock();
    select_and_assert(ToolPanelItem::Shape, Tool::Shape, false);
}

#[test]
fn test_panel_text_switches_to_text() {
    let _guard = crate::test_helpers::event_queue_lock();
    select_and_assert(ToolPanelItem::Text, Tool::Text, false);
}

#[test]
fn test_panel_curve_switches_to_curve() {
    let _guard = crate::test_helpers::event_queue_lock();
    // 曲线条目把当前工具切换为曲线（关闭填充共存态）
    select_and_assert(ToolPanelItem::Curve, Tool::Curve, false);
}

#[test]
fn test_panel_mouse_switches_to_shape_select() {
    let _guard = crate::test_helpers::event_queue_lock();
    // 鼠标工具 = 图形选中工具（Tool::ShapeSelect），独立于音符编辑的 Pointer
    select_and_assert(ToolPanelItem::Mouse, Tool::ShapeSelect, false);
}

#[test]
fn test_panel_fill_bucket_from_brush_toggles_fill_keeps_tool() {
    let _guard = crate::test_helpers::event_queue_lock();
    // 填充桶现在随时可切换：从画刷点击仅开启填充，不强制切换到曲线
    let _ = crate::event::take_events();
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.set_tool(Tool::Brush);
    root.toolbar.current_tool = Tool::Brush;
    let mut handler = handlers::ToolbarHandler::new();
    handler.handle(
        &mut root,
        Message::Toolbar(Event::ToolPanelItemSelected(ToolPanelItem::FillBucket)),
    );
    assert_eq!(
        root.editor.editor_state.current_tool(),
        Tool::Brush,
        "填充桶不应改变当前工具"
    );
    assert!(
        root.editor.editor_state.line_tool.fill_enabled,
        "填充桶应开启填充"
    );

    // 事件层的 toggle 语义（`apply_tool_panel_item`）仍可关闭填充。
    // 注意：视图层对**已启用**的颜料桶发的是 `ToolPanelItemSettingsRequested`（弹面板），
    // 故悬浮条上的关闭路径是面板内「关闭填充」按钮，而非再次点击条目本身。
    handler.handle(
        &mut root,
        Message::Toolbar(Event::ToolPanelItemSelected(ToolPanelItem::FillBucket)),
    );
    assert!(
        !root.editor.editor_state.line_tool.fill_enabled,
        "再次点击填充桶应关闭填充"
    );
    assert_eq!(root.editor.editor_state.current_tool(), Tool::Brush);
}

#[test]
fn test_panel_fill_bucket_from_curve_toggles_fill() {
    let _guard = crate::test_helpers::event_queue_lock();
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
    assert_eq!(root.editor.editor_state.current_tool(), Tool::Curve);
    assert!(
        root.editor.editor_state.line_tool.fill_enabled,
        "曲线下首次点填充桶应开启填充"
    );

    // 事件层 toggle 语义仍可关闭填充（视图层关闭路径见上一测试的说明；仍保持曲线）
    handler.handle(
        &mut root,
        Message::Toolbar(Event::ToolPanelItemSelected(ToolPanelItem::FillBucket)),
    );
    assert_eq!(root.editor.editor_state.current_tool(), Tool::Curve);
    assert!(
        !root.editor.editor_state.line_tool.fill_enabled,
        "曲线下再次点填充桶应关闭填充"
    );
}

#[test]
fn test_toolbar_update_sets_current_tool_before_sync() {
    let _guard = crate::test_helpers::event_queue_lock();
    // 隔离验证第一段：toolbar.update 自身就把 current_tool 设对（sync 之前）
    let mut root = Root::new_dialog("dark", DialogType::None);
    root.toolbar.tool_panel_open = true;
    root.toolbar
        .update(Event::ToolPanelItemSelected(ToolPanelItem::Brush));
    assert_eq!(
        root.toolbar.current_tool,
        Tool::Brush,
        "toolbar.update 应把 current_tool 设为 Brush"
    );
}

/// 渲染冒烟测试：悬浮工具条模型在打开状态下构建不应 panic。
#[test]
fn test_render_draw_toolbar_does_not_panic() {
    let _guard = crate::test_helpers::event_queue_lock();
    use lumino_core::storage::config::UiConfig;

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);
    root.toolbar.tool_panel_open = true;
    let model = root.egui_chrome_model();
    assert!(model.tool_panel.is_some(), "打开态应产出悬浮条模型");
}

// ── DRAW-048：工具切换后的待确认几何（口径 B）──────────────────────

/// 带一条待确认组合结果的 Root（模拟柔化 / 组合 / 抽屉插入之后的状态）。
fn root_with_composed_pending() -> Root {
    use lumino_core::shape::{ShapeKind, ShapeModel};
    use lumino_editor_state::{ComposedKind, PendingComposed, ShapeNote};

    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.editor_state.data.current_track = 1;
    let model = ShapeModel::new(1, 1, ShapeKind::Polyline, 1.0);
    let notes = vec![ShapeNote {
        track: 1,
        tick: 0.0,
        key: 60,
        length: 4.0,
        velocity: 100,
        channel: 0,
    }];
    root.editor
        .editor_state
        .pending_composed
        .push(PendingComposed::single(
            ComposedKind::Single,
            model,
            notes,
            Vec::new(),
        ));
    root
}

#[test]
fn test_composed_pending_stays_visible_in_drawing_tools() {
    let mut root = root_with_composed_pending();
    for tool in [
        Tool::Shape,
        Tool::Curve,
        Tool::Brush,
        Tool::Text,
        Tool::ShapeSelect,
    ] {
        root.editor.set_tool(tool);
        assert!(
            !root.shape_preview_model().shapes.is_empty(),
            "切到 {tool:?} 后待确认组合仍应只读可见（切走就消失会让用户以为内容丢了）"
        );
    }
}

#[test]
fn test_composed_pending_hidden_in_non_drawing_tools() {
    let mut root = root_with_composed_pending();
    for tool in [Tool::Pointer, Tool::Pencil, Tool::Eraser, Tool::Razor] {
        root.editor.set_tool(tool);
        assert!(
            root.shape_preview_model().shapes.is_empty(),
            "{tool:?} 下不得渲染待确认几何（避免浮层干扰音符编辑）"
        );
    }
}

#[test]
fn test_plain_shape_pending_not_drawn_in_other_drawing_tools() {
    // 只读放宽只针对「组合结果」（柔化 / 组合产物）；工具自己的待确认图形
    // 仍只在拥有者工具与鼠标工具下绘制，否则会与当前工具的绘制手势抢视觉。
    use lumino_editor_state::{ShapeInstance, ShapeKind};

    let mut root = Root::new_dialog("dark", DialogType::None);
    root.editor.editor_state.data.current_track = 1;
    root.editor
        .editor_state
        .shape_tool
        .shapes
        .push(ShapeInstance {
            kind: ShapeKind::Rectangle,
            rect: (0.0, 60.0, 100.0, 64.0),
            shift_constrained: false,
            filled: true,
            apex_high: true,
            mask: None,
        });
    root.editor.set_tool(Tool::Curve);
    assert!(
        root.shape_preview_model().shapes.is_empty(),
        "形状工具的待确认图形不得在曲线工具下绘制"
    );
}
