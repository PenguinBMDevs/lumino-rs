//! 悬浮绘制工具条：模型镜像与动作翻译单测（从 `tests.rs` 拆出）。

use super::*;
use lumino_core::{BrushConfig, TextLayoutMode};
use lumino_ui_egui::chrome::{
    ShapeKind, ToolPanelItemKind, ToolPanelPanelOwner, ToolPanelSettingsPanel, ToolbarAction,
};

use crate::Message;
use crate::message::{BrushSettingsAction, EditorAction, FillDivisionAction};
use crate::toolbar::{Event as ToolbarEvent, ShapeType, Tool, ToolPanelItem};

fn root() -> Root {
    Root::new(&lumino_core::storage::config::UiConfig::default())
}

#[test]
fn tool_panel_model_reflects_production_state() {
    let mut root = root();
    root.toolbar.tool_panel_open = true;
    let model = root.egui_chrome_model().tool_panel.expect("应接管悬浮条");
    assert_eq!(model.items.len(), 6, "全部绘制工具常显");
    assert!(model.panel.is_none(), "默认无面板");
    assert_eq!(model.offset, (0.0, 44.0), "默认位置");
    assert!(model.items.iter().all(|item| !item.selected));
    assert!(
        model.items.iter().all(|item| !item.settings_request),
        "未启用条目点击 = 普通选择"
    );

    // 激活工具 → 对应条目高亮；已启用条目再次点击弹面板。
    root.toolbar.current_tool = Tool::Curve;
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    assert!(model.items[1].selected && !model.items[1].settings_request);
    root.toolbar.current_tool = Tool::Brush;
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    assert!(model.items[3].selected && model.items[3].settings_request);
    assert!(!model.items[1].selected);

    // 填充开关独立于当前工具：颜料桶可共存高亮。
    root.toolbar.current_tool = Tool::ShapeSelect;
    root.toolbar.fill_enabled = true;
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    assert!(model.items[0].selected, "鼠标/选形");
    assert!(model.items[2].selected && model.items[2].settings_request);

    // 形状图标随当前图形类型。
    root.toolbar.current_shape = ShapeType::Triangle;
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    assert_eq!(
        model.items[4].icon,
        lumino_ui_egui::icons::Icon::ShapeTriangle
    );

    // 画刷面板。
    root.toolbar.brush_dropdown_open = true;
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    assert_eq!(
        model.panel.as_ref().map(ToolPanelSettingsPanel::owner),
        Some(ToolPanelPanelOwner::Item(ToolPanelItemKind::Brush))
    );
    let Some(ToolPanelSettingsPanel::Brush(brush)) = model.panel else {
        panic!("应为画刷面板");
    };
    assert_eq!(brush.max, BrushConfig::MAX_THICKNESS);
    assert_eq!(brush.behavior, "绘制行为");
    assert_eq!(brush.show_behavior, brush.thickness > 1);

    // 形状面板。
    root.toolbar.brush_dropdown_open = false;
    root.toolbar.shape_dropdown_open = true;
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    let Some(ToolPanelSettingsPanel::Shape(shape)) = model.panel else {
        panic!("应为形状面板");
    };
    assert_eq!(shape.selected, ShapeKind::Triangle);
    assert_eq!(shape.drawer, "形状抽屉");

    // 分音符填充面板。
    root.toolbar.shape_dropdown_open = false;
    root.state.fill_division_dialog.is_open = true;
    root.state.fill_division_dialog.value = "16".to_owned();
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    let Some(ToolPanelSettingsPanel::FillDivision(fill)) = model.panel else {
        panic!("应为填充面板");
    };
    assert_eq!(fill.value, "16");
    assert_eq!(fill.error, None);
    assert!(!fill.ok.is_empty() && !fill.cancel.is_empty());

    // 文字参数面板：当前字体必然在候选里（字体扫描为空时也成立）。
    root.state.fill_division_dialog.is_open = false;
    root.toolbar.text_params_open = true;
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    let Some(ToolPanelSettingsPanel::TextParams(text)) = model.panel else {
        panic!("应为文字参数面板");
    };
    let current = &root.editor.editor_state.text_tool.params.font;
    assert!(
        text.fonts.iter().any(|font| font == current),
        "当前字体必须在候选列表内"
    );

    // 粗细面板。
    root.toolbar.text_params_open = false;
    root.toolbar.thickness_panel_open = true;
    root.toolbar.brush.set_thickness(42);
    let model = root.egui_chrome_model().tool_panel.expect("悬浮条");
    let Some(ToolPanelSettingsPanel::Thickness(thickness)) = model.panel else {
        panic!("应为粗细面板");
    };
    assert_eq!(thickness.value, 42);
    assert_eq!(thickness.min, BrushConfig::MIN_THICKNESS);
    assert!(thickness.value_text.contains("42"));
    assert!(!thickness.hint.is_empty());
    assert!(model.thickness_selected);
}

#[test]
fn tool_panel_actions_map_to_production_messages() {
    let mut root = root();
    root.toolbar.tool_panel_open = true;

    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ToolPanelItem {
            kind: ToolPanelItemKind::Curve,
            settings: false,
        }),
        Some(Message::Toolbar(ToolbarEvent::ToolPanelItemSelected(
            ToolPanelItem::Curve
        )))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ToolPanelItem {
            kind: ToolPanelItemKind::Brush,
            settings: true,
        }),
        Some(Message::Toolbar(
            ToolbarEvent::ToolPanelItemSettingsRequested(ToolPanelItem::Brush)
        ))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ShapeSelected(ShapeKind::Circle)),
        Some(Message::Toolbar(ToolbarEvent::ShapeTypeSelected(
            ShapeType::Circle
        )))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ThicknessChanged(42)),
        Some(Message::Toolbar(ToolbarEvent::DrawThicknessChanged(42)))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::FillDivisionChanged("16".to_owned())),
        Some(Message::FillDivision(FillDivisionAction::ValueChanged(value))) if value == "16"
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::FillDivisionConfirm),
        Some(Message::FillDivision(FillDivisionAction::Confirm))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::FillDisable),
        Some(Message::Toolbar(ToolbarEvent::FillToggled(false)))
    ));

    // 拖拽三个动作。
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ToolPanelDragStarted),
        Some(Message::Toolbar(ToolbarEvent::ToolPanelDragStarted))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ToolPanelDragged { x: 12.0, y: 34.0 }),
        Some(Message::Toolbar(ToolbarEvent::ToolPanelDragged(x, y))) if x == 12.0 && y == 34.0
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ToolPanelDragEnded),
        Some(Message::Toolbar(ToolbarEvent::ToolPanelDragEnded))
    ));

    // 五个面板的关闭动作。
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::CloseBrushPanel),
        Some(Message::Toolbar(ToolbarEvent::CloseBrushDropdown))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::CloseShapePanel),
        Some(Message::Toolbar(ToolbarEvent::CloseShapeDropdown))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::CloseFillPanel),
        Some(Message::FillDivision(FillDivisionAction::CloseDialog))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::CloseTextPanel),
        Some(Message::Toolbar(ToolbarEvent::CloseTextParamsPanel))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::CloseThicknessPanel),
        Some(Message::Toolbar(ToolbarEvent::CloseThicknessPanel))
    ));

    // 齿轮 / 形状抽屉 / 绘制行为。
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::OpenDrawSettings),
        Some(Message::Toolbar(ToolbarEvent::OpenDrawSettingsDialog))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::OpenShapeDrawer),
        Some(Message::Toolbar(ToolbarEvent::OpenShapeDrawerDialog))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::BrushSettingsOpen),
        Some(Message::BrushSettings(BrushSettingsAction::OpenDialog(_)))
    ));

    // 文字参数：整体回传 + 模式切换目标由生产当前模式推导。
    let mut params = root.editor.editor_state.text_tool.params.clone();
    params.size = 48.0;
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::TextParamsChanged(params.clone())),
        Some(Message::EditorAction(EditorAction::TextParamsChanged(next))) if next.size == 48.0
    ));
    let expected = match root.editor.editor_state.text_tool.layout_mode {
        TextLayoutMode::LegacyBox => TextLayoutMode::Parametric,
        TextLayoutMode::Parametric => TextLayoutMode::LegacyBox,
    };
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::TextLayoutModeToggle),
        Some(Message::EditorAction(EditorAction::TextLayoutModeChanged(mode))) if mode == expected
    ));
}
