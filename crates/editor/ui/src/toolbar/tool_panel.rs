//! 绘制工具选择面板渲染
//!
//! 点击工具栏「曲线工具」右侧的小三角（`ToolPanelCaret`）展开，以横向图标栏形式
//! 列出绘制类工具/设置入口。
//!
//! 视觉与交互对齐工具栏「右键悬浮面板」（`ui-editor/src/context_menu.rs`）：
//! 图标独占按钮（`BUTTON_SIZE=40`、`ICON_SIZE=36`），说明走 `tooltip` 悬浮显示，
//! 并在面板底部保留一条常驻「描述条」展示当前激活工具的说明（即用户要求的"描述位子"）。
//!
//! 点击面板外部区域关闭（由 `tools.rs` 的遮罩层驱动）。
//!
//! 共存规则（与工具栏选择逻辑一致）：
//! - 曲线工具 / 形状工具 可与 填充桶 共存（填充桶仅在二者激活时可切换/高亮）；
//! - 画刷 / 文字 / 橡皮擦 为独立工具，不可与填充桶共存（填充桶对其置灰禁用）。

use iced_core::{Alignment, Background, Color, Length};
use iced_widget::{button, column, container, row, text, tooltip};

use crate::resources::icon;
use crate::toolbar::{Event, Tool, ToolPanelItem};
use crate::{Element, Theme};
use lumino_extras::i18n::{Language, MainTranslations};

/// 图标按钮尺寸（宽高相同），与右键悬浮面板（context_menu.rs）保持一致
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 36;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;

/// 渲染「绘制工具选择面板」
///
/// - `current_tool`：当前激活工具，用于标记面板条目选中态与底部描述条；
/// - `fill_enabled`：填充桶开关，随时可切换；仅对曲线/形状绘制的封闭图形生效（作用范围由编辑器控制）；
/// - `language`：当前语言（用于面板条目标签与说明）；
/// - `panel_background`：面板背景色（由调用方据工具栏背景计算，贴近工具栏配色）；
/// - `theme`：当前主题（用于图标反色与文字配色）。
pub fn render_tool_panel<'a>(
    current_tool: Tool,
    fill_enabled: bool,
    language: Language,
    panel_background: Color,
    theme: &'a Theme,
) -> Element<'a> {
    let t = lumino_extras::i18n::main_translations(language);

    // 面板条目顺序：填充桶 / 画刷工具 / 形状工具 / 文字输入 / 橡皮擦
    // 第三项为该工具的说明文案（用于底部描述条与悬浮 tooltip）。
    // 绘制工具条目；曲线工具条目在「当前非曲线工具」时插入到首位，
    // 启用曲线工具后则不显示自身切换图标（避免冗余自选）。
    let mut items: Vec<(ToolPanelItem, icon::Icon, &'static str)> = vec![
        (ToolPanelItem::FillBucket, icon::PaintBucket, t.tool_fill_desc),
        (ToolPanelItem::Brush, icon::BrushTool, t.tool_brush_desc),
        (ToolPanelItem::Shape, icon::ShapeTool, t.tool_shape_desc),
        (ToolPanelItem::Text, icon::TextInput, t.tool_text_desc),
        (ToolPanelItem::Eraser, icon::Eraser, t.tool_eraser_desc),
    ];
    if current_tool != Tool::Curve {
        items.insert(0, (ToolPanelItem::Curve, icon::Curve, t.tool_curve_desc));
    }

    let buttons = items
        .iter()
        .map(|(item, ic, desc)| {
            let selected = panel_item_selected(*item, current_tool, fill_enabled);
            let disabled = panel_item_disabled(*item, current_tool);
            panel_button(*item, *ic, desc, selected, disabled, theme)
        })
        .collect::<Vec<_>>();

    // 底部常驻描述条：展示当前激活工具的说明（"描述位子"始终可见）；
    // 单个按钮悬浮时由 tooltip 另行显示该按钮的说明。
    let desc_bar = container(
        text(active_tool_desc(current_tool, fill_enabled, t))
            .size(12)
            .color(theme.extended_palette().background.weak.text),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.18))),
        border: iced_core::Border::default().rounded(6),
        ..Default::default()
    });

    // 横向图标栏 + 底部描述条，整体与右键悬浮面板同范式（图标独占、说明外置）。
    let panel = container(
        column![
            row(buttons)
                .spacing(BUTTON_SPACING)
                .align_y(Alignment::Center),
            desc_bar,
        ]
        .spacing(8)
        .align_x(Alignment::Center)
        .width(Length::Shrink)
        .height(Length::Shrink),
    )
    .padding(PANEL_PADDING)
    .width(Length::Shrink)
    .height(Length::Shrink)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(panel_background)),
        border: iced_core::Border::default().rounded(8),
        ..Default::default()
    });

    // 注意：面板整体在调用方（tools.rs）被 `mouse_area(panel).on_press(close)` 包裹，
    // 点击面板内空白即关闭下拉；面板内按钮仍优先响应自身 on_press（与右键悬浮面板
    // context_menu 同源，mouse_area 不会吞掉子按钮点击）。面板本身的渲染不含关闭逻辑。
    panel.into()
}

/// 面板条目是否处于选中（激活）态
fn panel_item_selected(item: ToolPanelItem, current_tool: Tool, fill_enabled: bool) -> bool {
    match item {
        ToolPanelItem::FillBucket => fill_enabled,
        ToolPanelItem::Curve => current_tool == Tool::Curve,
        ToolPanelItem::Brush => current_tool == Tool::Brush,
        ToolPanelItem::Shape => current_tool == Tool::Shape,
        ToolPanelItem::Text => current_tool == Tool::Text,
        ToolPanelItem::Eraser => current_tool == Tool::DrawEraser,
        // 描边设置无选中态（占位入口）
        ToolPanelItem::StrokeSettings => false,
    }
}

/// 面板条目是否禁用（填充桶在非曲线/形状工具下不可与当前工具共存）
fn panel_item_disabled(item: ToolPanelItem, _current_tool: Tool) -> bool {
    match item {
        // 颜料桶随时可切换（仅对曲线/形状绘制的封闭图形生效，由编辑器侧控制实际作用）
        ToolPanelItem::FillBucket => false,
        _ => false,
    }
}

/// 当前激活工具对应的说明文案（用于底部描述条）
fn active_tool_desc(current_tool: Tool, fill_enabled: bool, t: &MainTranslations) -> &'static str {
    match (current_tool, fill_enabled) {
        (Tool::Curve, true) => t.tool_fill_desc,
        (Tool::Brush, _) => t.tool_brush_desc,
        (Tool::Shape, _) => t.tool_shape_desc,
        (Tool::Text, _) => t.tool_text_desc,
        (Tool::Eraser, _) => t.tool_eraser_desc,
        (Tool::DrawEraser, _) => t.tool_eraser_desc,
        // 纯曲线工具（未开填充）或无对应面板项时，回落到曲线说明
        _ => t.tool_curve_desc,
    }
}

/// 构建面板中的图标独占按钮（对齐右键悬浮面板范式）
fn panel_button<'a>(
    item: ToolPanelItem,
    ic: icon::Icon,
    desc: &'static str,
    selected: bool,
    disabled: bool,
    theme: &'a Theme,
) -> Element<'a> {
    // 图标与右键悬浮面板（context_menu.rs）完全一致：ICON_SIZE=36 装在 BUTTON_SIZE=40 内，
    // 且使用「无裁切」渲染（view_with_size_and_theme），保留 FontAwesome 笔画四周的自然留白。
    // 之前误用 crop=0.82 会让笔画填满盒子、贴边显得"过大/被裁"，现已对齐参考实现。
    let icon_el = icon::view_with_size_and_theme(ic, ICON_SIZE, ICON_SIZE, Some(theme));

    let on_press = if disabled {
        None
    } else {
        Some(Event::tool_panel_item_selected(item))
    };

    let btn = button(icon_el)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press_maybe(on_press)
        .style(move |_theme: &Theme, status| icon_button_style(status, selected, disabled));

    // 悬浮 tooltip 显示该按钮说明（与右键悬浮面板一致，避免把文字塞进按钮撑宽）
    tooltip::Tooltip::new(btn, text(desc), tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 图标按钮样式（对齐右键悬浮面板 button_style）
///
/// - 禁用：透明背景；
/// - 选中：浅色高亮（与工具栏选中态呼应）；
/// - 常态：悬停/按下浅色高亮。
fn icon_button_style(status: button::Status, selected: bool, disabled: bool) -> button::Style {
    use button::Status;

    let background = if disabled {
        Color::TRANSPARENT
    } else if selected {
        Color::from_rgba(1.0, 1.0, 1.0, 0.16)
    } else {
        match status {
            Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.12),
            Status::Pressed => Color::from_rgba(1.0, 1.0, 1.0, 0.22),
            _ => Color::TRANSPARENT,
        }
    };

    button::Style {
        border: iced_core::Border::default().rounded(6),
        ..Default::default()
    }
    .with_background(background)
}

/// Tooltip 样式：深色背景 + 浅色文字（与右键悬浮面板一致）
fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.08, 0.08, 0.10, 0.96))),
        border: iced_core::Border::default().rounded(4),
        text_color: Some(Color::from_rgba(0.95, 0.95, 0.95, 1.0)),
        ..Default::default()
    }
}
