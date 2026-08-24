//! 绘制工具选择面板渲染
//!
//! 点击工具栏「颜料桶」右侧的小三角（`ToolPanelCaret`）展开，以垂直列表形式
//! 列出绘制类工具/设置入口。
//!
//! 视觉与交互参考工具栏「更多工具」溢出菜单（overflow）：深色背景、圆角 8、
//! 点击面板外部区域关闭（由 `right_content.rs` 的遮罩层驱动）。

use iced_core::{Alignment, Background, Color, Length};
use iced_widget::{button, column, container, mouse_area, row, space, text};

use crate::resources::icon;
use crate::toolbar::{Event, ToolPanelItem};
use crate::{Element, Message, Theme};
use lumino_extras::i18n::Language;

/// 渲染「绘制工具选择面板」
///
/// - `language`：当前语言（用于面板条目标签）
/// - `panel_background`：面板背景色（由调用方据工具栏背景计算，贴近工具栏配色）
/// - `theme`：当前主题（用于图标反色与文字配色）
pub fn render_tool_panel<'a>(
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
        .map(|(item, ic, label)| panel_row(*item, *ic, label, theme))
        .collect::<Vec<_>>();

    let panel = container(column(rows).spacing(4).align_x(Alignment::Start))
        .padding(8)
        .width(Length::Fixed(220.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(panel_background)),
            border: iced_core::Border::default().rounded(8),
            ..Default::default()
        });

    // 用 mouse_area 包裹面板，吞掉面板内部的点击事件，
    // 避免触发下层的「关闭遮罩」导致面板立刻关闭
    mouse_area(panel).on_press(Message::Null).into()
}

/// 构建面板中的单行（SVG 图标 + 文字标签）
fn panel_row<'a>(
    item: ToolPanelItem,
    ic: icon::Icon,
    label: &'static str,
    theme: &'a Theme,
) -> Element<'a> {
    let palette = theme.extended_palette();

    let icon_el = icon::view_with_size_and_theme(ic, 20, 20, Some(theme));
    let label_el: Element<'a> = text(label)
        .size(13)
        .color(palette.background.weak.text)
        .into();

    let content = row![icon_el, space().width(10), label_el]
        .align_y(Alignment::Center)
        .padding([6.0, 10.0]);

    button(content)
        .width(Length::Fill)
        .style(move |_theme: &Theme, status| row_button_style(status))
        .on_press(Event::tool_panel_item_selected(item))
        .into()
}

/// 面板行按钮样式（无选中时透明，悬停/按下浅色高亮，参考溢出菜单按钮）
fn row_button_style(status: button::Status) -> button::Style {
    use button::Status;

    let background = match status {
        Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.12),
        Status::Pressed => Color::from_rgba(1.0, 1.0, 1.0, 0.22),
        _ => Color::TRANSPARENT,
    };

    button::Style {
        border: iced_core::Border::default().rounded(6),
        ..Default::default()
    }
    .with_background(background)
}
