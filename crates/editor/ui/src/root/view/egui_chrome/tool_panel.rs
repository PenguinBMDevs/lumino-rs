//! 悬浮绘制工具条：模型构建 + 动作解析。
//!
//! `lumino-ui-egui` 只认识纯数据（`ToolPanelModel` 与五个面板模型）和
//! `ToolbarAction`；这里把生产状态镜像进去，并把点击翻译回与 旧版 悬浮条
//! **同一批** `toolbar::Event` 构造器。

use lumino_core::{BrushConfig, TextLayoutMode, tr};
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::{
    BrushPanelModel, FillDivisionPanelModel, ShapeKind, ShapePanelModel, TextParamsPanelModel,
    ThicknessPanelModel, ToolPanelItemKind, ToolPanelItemModel, ToolPanelModel,
    ToolPanelSettingsPanel, ToolbarAction,
};
use lumino_ui_egui::icons::Icon;

use crate::Message;
use crate::message::{BrushSettingsAction, EditorAction, FillDivisionAction};
use crate::root::Root;
use crate::toolbar::{ShapeType, Tool, ToolPanelItem};

/// 构造悬浮条模型；未打开或当前视图不渲染卷帘时返回 `None`。
pub(super) fn model(root: &Root, left_inset: f32, top_inset: f32) -> Option<ToolPanelModel> {
    if !root.toolbar.tool_panel_open || !root.right_sidebar_visible() {
        return None;
    }
    let language = root.settings.display.language;
    let t = main_translations(language);
    let cur = root.toolbar.current_tool;
    let fill = root.toolbar.fill_enabled;
    let tip = |tool: Tool, fallback: &str| {
        lumino_ui_core::help::tool_tip(tool, language)
            .unwrap_or(fallback)
            .to_owned()
    };
    let items = vec![
        ToolPanelItemModel {
            kind: ToolPanelItemKind::Mouse,
            icon: Icon::MousePointer,
            tooltip: tip(Tool::ShapeSelect, t.tool_mouse),
            selected: cur == Tool::ShapeSelect,
            settings_request: false,
        },
        ToolPanelItemModel {
            kind: ToolPanelItemKind::Curve,
            icon: Icon::Curve,
            tooltip: tip(Tool::Curve, t.tool_curve),
            selected: cur == Tool::Curve && !root.toolbar.arc_mode,
            settings_request: false,
        },
        ToolPanelItemModel {
            kind: ToolPanelItemKind::Arc,
            icon: Icon::ArcTool,
            tooltip: tr!("三点圆弧").to_owned(),
            selected: cur == Tool::Curve && root.toolbar.arc_mode,
            settings_request: false,
        },
        ToolPanelItemModel {
            kind: ToolPanelItemKind::FillBucket,
            icon: Icon::PaintBucket,
            tooltip: t.tool_fill.to_owned(),
            selected: fill,
            settings_request: fill,
        },
        ToolPanelItemModel {
            kind: ToolPanelItemKind::Brush,
            icon: Icon::BrushTool,
            tooltip: tip(Tool::Brush, t.tool_brush),
            selected: cur == Tool::Brush,
            settings_request: cur == Tool::Brush,
        },
        ToolPanelItemModel {
            kind: ToolPanelItemKind::Shape,
            icon: shape_icon(root.toolbar.current_shape),
            tooltip: tip(Tool::Shape, t.tool_shape),
            selected: cur == Tool::Shape,
            settings_request: cur == Tool::Shape,
        },
        ToolPanelItemModel {
            kind: ToolPanelItemKind::Text,
            icon: Icon::TextInput,
            tooltip: tip(Tool::Text, t.tool_text),
            selected: cur == Tool::Text,
            settings_request: cur == Tool::Text,
        },
    ];

    Some(ToolPanelModel {
        offset: root.toolbar.tool_panel_offset,
        left_inset,
        top_inset,
        thickness_tooltip: t.tool_thickness.to_owned(),
        items,
        thickness_selected: root.toolbar.thickness_panel_open,
        panel: panel_model(root, language),
    })
}

/// 当前打开的面板（五者互斥，与生产 `draw_tool_settings_menu` 同序）。
fn panel_model(
    root: &Root,
    language: lumino_extras::i18n::Language,
) -> Option<ToolPanelSettingsPanel> {
    let t = main_translations(language);
    if root.toolbar.brush_dropdown_open {
        let thickness = root
            .toolbar
            .brush
            .thickness
            .clamp(BrushConfig::MIN_THICKNESS, BrushConfig::MAX_THICKNESS);
        return Some(ToolPanelSettingsPanel::Brush(BrushPanelModel {
            thickness,
            min: BrushConfig::MIN_THICKNESS,
            max: BrushConfig::MAX_THICKNESS,
            title: t.tool_thickness.to_owned(),
            value: format!(
                "{thickness} {} / {}",
                t.draw_thickness_unit,
                BrushConfig::MAX_THICKNESS
            ),
            behavior: tr!("绘制行为").to_owned(),
            show_behavior: thickness > 1,
        }));
    }
    if root.toolbar.shape_dropdown_open {
        return Some(ToolPanelSettingsPanel::Shape(ShapePanelModel {
            selected: shape_kind(root.toolbar.current_shape),
            rectangle: tr!("矩形").to_owned(),
            circle: tr!("圆形").to_owned(),
            triangle: tr!("三角形").to_owned(),
            drawer: tr!("形状抽屉").to_owned(),
        }));
    }
    if root.state.fill_division_dialog.is_open {
        let dialog = &root.state.fill_division_dialog;
        return Some(ToolPanelSettingsPanel::FillDivision(
            FillDivisionPanelModel {
                value: dialog.value.clone(),
                error: dialog.error.clone(),
                prefix: t.fill_division_prefix.to_owned(),
                suffix: t.fill_division_suffix.to_owned(),
                hint: t.fill_division_hint.to_owned(),
                disable: t.fill_division_disable.to_owned(),
                cancel: t.precision_cancel.to_owned(),
                ok: t.precision_ok.to_owned(),
            },
        ));
    }
    if root.toolbar.text_params_open {
        let text = &root.editor.editor_state.text_tool;
        let mut fonts: Vec<String> = lumino_note_core::font_scanner::get_cached_fonts()
            .iter()
            .map(|font| font.name.clone())
            .collect();
        fonts.sort_unstable();
        fonts.dedup();
        let current = &text.params.font;
        if fonts.is_empty() {
            fonts.push(current.clone());
        } else if !fonts.iter().any(|font| font.eq_ignore_ascii_case(current)) {
            // 当前字体可能来自旧工程/外部字体文件，不能被空选项列表吞掉。
            fonts.push(current.clone());
        }
        return Some(ToolPanelSettingsPanel::TextParams(TextParamsPanelModel {
            params: text.params.clone(),
            mode: text.layout_mode,
            fonts,
        }));
    }
    if root.toolbar.thickness_panel_open {
        let value = root
            .toolbar
            .brush
            .thickness
            .clamp(BrushConfig::MIN_THICKNESS, BrushConfig::MAX_THICKNESS);
        return Some(ToolPanelSettingsPanel::Thickness(ThicknessPanelModel {
            value,
            min: BrushConfig::MIN_THICKNESS,
            max: BrushConfig::MAX_THICKNESS,
            title: t.tool_thickness.to_owned(),
            value_text: format!(
                "{value} {} ({} - {})",
                t.draw_thickness_unit,
                BrushConfig::MIN_THICKNESS,
                BrushConfig::MAX_THICKNESS
            ),
            hint: t.draw_thickness_hint.to_owned(),
        }));
    }
    None
}

/// 悬浮条动作 → 生产 `Message`（非悬浮条动作返回 `None`）。
pub(super) fn action_message(root: &Root, action: &ToolbarAction) -> Option<Message> {
    Some(match action {
        ToolbarAction::ToolPanelItem { kind, settings } => {
            let item = tool_panel_item(*kind);
            if *settings {
                crate::toolbar::Event::tool_panel_item_settings_requested(item)
            } else {
                crate::toolbar::Event::tool_panel_item_selected(item)
            }
        }
        ToolbarAction::ThicknessToggle => crate::toolbar::Event::toggle_thickness_panel(),
        ToolbarAction::OpenDrawSettings => crate::toolbar::Event::open_draw_settings_dialog(),
        ToolbarAction::ToolPanelDragStarted => crate::toolbar::Event::tool_panel_drag_started(),
        ToolbarAction::ToolPanelDragged { x, y } => {
            crate::toolbar::Event::tool_panel_dragged(*x, *y)
        }
        ToolbarAction::ToolPanelDragEnded => crate::toolbar::Event::tool_panel_drag_ended(),
        ToolbarAction::CloseBrushPanel => crate::toolbar::Event::close_brush_dropdown(),
        ToolbarAction::CloseShapePanel => crate::toolbar::Event::close_shape_dropdown(),
        ToolbarAction::CloseFillPanel => Message::FillDivision(FillDivisionAction::CloseDialog),
        ToolbarAction::CloseTextPanel => crate::toolbar::Event::close_text_params_panel(),
        ToolbarAction::CloseThicknessPanel => crate::toolbar::Event::close_thickness_panel(),
        ToolbarAction::ShapeSelected(kind) => {
            crate::toolbar::Event::shape_type_selected(shape_type(*kind))
        }
        ToolbarAction::OpenShapeDrawer => crate::toolbar::Event::open_shape_drawer_dialog(),
        ToolbarAction::BrushSettingsOpen => {
            Message::BrushSettings(BrushSettingsAction::OpenDialog(root.toolbar.brush.clone()))
        }
        ToolbarAction::ThicknessChanged(value) => {
            crate::toolbar::Event::draw_thickness_changed(*value)
        }
        ToolbarAction::FillDivisionChanged(value) => {
            Message::FillDivision(FillDivisionAction::ValueChanged(value.clone()))
        }
        ToolbarAction::FillDivisionConfirm => Message::FillDivision(FillDivisionAction::Confirm),
        ToolbarAction::FillDisable => crate::toolbar::Event::fill_toggled(false),
        ToolbarAction::TextParamsChanged(params) => {
            Message::EditorAction(EditorAction::TextParamsChanged(params.clone()))
        }
        ToolbarAction::TextLayoutModeToggle => {
            let target = match root.editor.editor_state.text_tool.layout_mode {
                TextLayoutMode::LegacyBox => TextLayoutMode::Parametric,
                TextLayoutMode::Parametric => TextLayoutMode::LegacyBox,
            };
            Message::EditorAction(EditorAction::TextLayoutModeChanged(target))
        }
        _ => return None,
    })
}

/// 生产条目枚举。
fn tool_panel_item(kind: ToolPanelItemKind) -> ToolPanelItem {
    match kind {
        ToolPanelItemKind::Mouse => ToolPanelItem::Mouse,
        ToolPanelItemKind::Curve => ToolPanelItem::Curve,
        ToolPanelItemKind::Arc => ToolPanelItem::Arc,
        ToolPanelItemKind::FillBucket => ToolPanelItem::FillBucket,
        ToolPanelItemKind::Brush => ToolPanelItem::Brush,
        ToolPanelItemKind::Shape => ToolPanelItem::Shape,
        ToolPanelItemKind::Text => ToolPanelItem::Text,
    }
}

/// 生产图形类型。
fn shape_type(kind: ShapeKind) -> ShapeType {
    match kind {
        ShapeKind::Rectangle => ShapeType::Rectangle,
        ShapeKind::Circle => ShapeType::Circle,
        ShapeKind::Triangle => ShapeType::Triangle,
    }
}

/// egui 侧图形类型。
fn shape_kind(shape: ShapeType) -> ShapeKind {
    match shape {
        ShapeType::Rectangle => ShapeKind::Rectangle,
        ShapeType::Circle => ShapeKind::Circle,
        ShapeType::Triangle => ShapeKind::Triangle,
    }
}

/// 形状条目图标随当前类型切换（与生产 `shape_icon` 同口径）。
fn shape_icon(shape: ShapeType) -> Icon {
    match shape {
        ShapeType::Rectangle => Icon::ShapeRectangle,
        ShapeType::Circle => Icon::ShapeCircle,
        ShapeType::Triangle => Icon::ShapeTriangle,
    }
}
