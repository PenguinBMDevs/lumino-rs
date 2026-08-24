//! 绘制工具选择面板渲染
//!
//! 点击工具栏「曲线工具」右侧的小三角（`ToolPanelCaret`）展开，以垂直列表形式
//! 列出绘制类工具/设置入口。
//!
//! 视觉与交互参考工具栏「更多工具」溢出菜单（overflow）：深色背景、圆角 8、
//! 点击面板外部区域关闭（由 `tools.rs` 的遮罩层驱动）。
//!
//! 共存规则（与工具栏选择逻辑一致）：
//! - 曲线工具 / 形状工具 可与 填充桶 共存（填充桶仅在二者激活时可切换/高亮）；
//! - 画刷 / 文字 / 橡皮擦 为独立工具，不可与填充桶共存（填充桶对其置灰禁用）。

use iced_core::{Alignment, Background, Color, Length};
use iced_widget::{button, column, container, mouse_area, row, space, text};

use crate::resources::icon;
use crate::toolbar::{Event, Tool, ToolPanelItem};
use crate::{Element, Message, Theme};
use lumino_extras::i18n::Language;

/// 渲染「绘制工具选择面板」
///
/// - `current_tool`：当前激活工具，用于标记面板条目选中态；
/// - `fill_enabled`：填充桶开关，仅当 `current_tool` 为曲线/形状时有效；
/// - `language`：当前语言（用于面板条目标签）；
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

    // 面板条目顺序：1 描边设置 / 2 填充桶 / 3 画刷工具 / 4 形状工具 / 5 文字输入 / 6 橡皮擦
    let items: &[(ToolPanelItem, icon::Icon, &'static str)] = &[
        (ToolPanelItem::StrokeSettings, icon::StrokeSettings, t.tool_stroke),
        (ToolPanelItem::FillBucket, icon::PaintBucket, t.tool_fill),
        (ToolPanelItem::Brush, icon::BrushTool, t.tool_brush),
        (ToolPanelItem::Shape, icon::ShapeTool, t.tool_shape),
        (ToolPanelItem::Text, icon::TextInput, t.tool_text),
        (ToolPanelItem::Eraser, icon::Eraser, t.tool_eraser),
    ];

    let rows = items
        .iter()
        .map(|(item, ic, label)| {
            let selected = panel_item_selected(*item, current_tool, fill_enabled);
            let disabled = panel_item_disabled(*item, current_tool);
            panel_row(*item, *ic, label, selected, disabled, theme)
        })
        .collect::<Vec<_>>();

    let panel = container(
        column(rows)
            .spacing(4)
            .align_x(Alignment::Start)
            .height(Length::Shrink),
    )
    .padding(8)
    .width(Length::Fixed(220.0))
    .height(Length::Shrink)
    .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(panel_background)),
            border: iced_core::Border::default().rounded(8),
            ..Default::default()
        });

    // 用 mouse_area 包裹面板，吞掉面板内部的点击事件，
    // 避免触发下层的「关闭遮罩」导致面板立刻关闭
    mouse_area(panel).on_press(Message::Null).into()
}

/// 面板条目是否处于选中（激活）态
fn panel_item_selected(item: ToolPanelItem, current_tool: Tool, fill_enabled: bool) -> bool {
    match item {
        ToolPanelItem::FillBucket => {
            fill_enabled && matches!(current_tool, Tool::Curve | Tool::Shape)
        }
        ToolPanelItem::Brush => current_tool == Tool::Brush,
        ToolPanelItem::Shape => current_tool == Tool::Shape,
        ToolPanelItem::Text => current_tool == Tool::Text,
        ToolPanelItem::Eraser => current_tool == Tool::Eraser,
        // 描边设置无选中态（占位入口）
        ToolPanelItem::StrokeSettings => false,
    }
}

/// 面板条目是否禁用（填充桶在非曲线/形状工具下不可与当前工具共存）
fn panel_item_disabled(item: ToolPanelItem, current_tool: Tool) -> bool {
    match item {
        ToolPanelItem::FillBucket => !matches!(current_tool, Tool::Curve | Tool::Shape),
        _ => false,
    }
}

/// 构建面板中的单行（SVG 图标 + 文字标签 + 选中勾选）
fn panel_row<'a>(
    item: ToolPanelItem,
    ic: icon::Icon,
    label: &'static str,
    selected: bool,
    disabled: bool,
    theme: &'a Theme,
) -> Element<'a> {
    let palette = theme.extended_palette();

    let icon_el = icon::view_with_size_and_theme(ic, 20, 20, Some(theme));
    let label_el: Element<'a> = text(label)
        .size(13)
        .color(palette.background.weak.text)
        .into();

    let mut content = row![icon_el, space().width(10), label_el]
        .align_y(Alignment::Center)
        .padding([6.0, 10.0]);

    // 选中态在右侧显示勾选，强化"当前激活工具"反馈
    if selected {
        content = content
            .push(space().width(Length::Fill))
            .push(text("✓").size(13).color(palette.background.strong.text));
    }

    let on_press = if disabled {
        None
    } else {
        Some(Event::tool_panel_item_selected(item))
    };

    button(content)
        .width(Length::Fill)
        .height(Length::Fixed(32.0))
        .style(move |_theme: &Theme, status| row_button_style(status, selected, disabled))
        .on_press_maybe(on_press)
        .into()
}

/// 面板行按钮样式
///
/// - 禁用：透明背景、文字变暗；
/// - 选中：浅色高亮（与工具栏选中态呼应）；
/// - 常态：悬停/按下浅色高亮（参考溢出菜单按钮）。
fn row_button_style(
    status: button::Status,
    selected: bool,
    disabled: bool,
) -> button::Style {
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
