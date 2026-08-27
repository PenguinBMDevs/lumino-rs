//! 形状工具下拉（ctrl+点击形状工具触发）
//!
//! 以横向图标栏形式列出三种基本图形（矩形/圆形/三角形），点击切换当前图形类型。
//! 选中态与工具栏按钮保持呼应：当前图形类型对应的图标高亮，点击后由 `Toolbar::update`
//! 写入 `current_shape` 状态变量并自动关闭下拉。
//!
//! 视觉与交互对齐「绘制工具选择面板」(`tool_panel.rs`)：图标独占按钮（`BUTTON_SIZE=40`、
//! `ICON_SIZE=36`），说明走 `tooltip` 悬浮显示，圆角与工具栏下拉同范式（8）。

use iced_core::{Alignment, Background, Border, Color};
use iced_widget::{button, container, row, text, tooltip};

use crate::resources::icon;
use crate::toolbar::{Event, ShapeType};
use crate::{Element, Theme};

/// 图标按钮尺寸（宽高相同），与绘制工具选择面板（tool_panel.rs）保持一致
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 36;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;

/// 渲染「形状工具下拉」
///
/// - `current_shape`：当前激活的图形类型，用于标记选中高亮；
/// - `panel_background`：面板背景色（由调用方据工具栏背景计算，贴近工具栏配色）；
/// - `theme`：当前主题（用于图标反色与文字配色）。
pub fn render_shape_dropdown<'a>(
    current_shape: ShapeType,
    panel_background: Color,
    theme: &'a Theme,
) -> Element<'a> {
    // 条目：(图形类型, 图标, 悬浮 tooltip 名称)
    let all_items: &[(ShapeType, icon::Icon, &'static str)] = &[
        (ShapeType::Rectangle, icon::ShapeRectangle, "矩形"),
        (ShapeType::Circle, icon::ShapeCircle, "圆形"),
        (ShapeType::Triangle, icon::ShapeTriangle, "三角形"),
    ];

    let buttons = all_items
        .iter()
        .map(|(shape, ic, desc)| {
            let selected = *shape == current_shape;
            shape_button(*shape, *ic, desc, selected, theme)
        })
        .collect::<Vec<_>>();

    let palette = theme.extended_palette();

    container(
        row(buttons)
            .spacing(BUTTON_SPACING)
            .align_y(Alignment::Center),
    )
    .width(iced_core::Length::Shrink)
    .height(iced_core::Length::Shrink)
    .padding(PANEL_PADDING)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(panel_background)),
        border: Border {
            width: 1.0,
            color: palette.background.strong.color,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// 构建下拉中的图形图标按钮
fn shape_button<'a>(
    shape: ShapeType,
    ic: icon::Icon,
    desc: &'static str,
    selected: bool,
    theme: &'a Theme,
) -> Element<'a> {
    let icon_el = icon::view_with_size_and_theme(ic, ICON_SIZE, ICON_SIZE, Some(theme));

    let btn = button(icon_el)
        .width(iced_core::Length::Fixed(BUTTON_SIZE))
        .height(iced_core::Length::Fixed(BUTTON_SIZE))
        .on_press(Event::shape_type_selected(shape))
        .style(move |_theme: &Theme, status| icon_button_style(status, selected));

    // 悬浮 tooltip 显示该按钮说明（与绘制工具选择面板一致，避免把文字塞进按钮撑宽）
    tooltip::Tooltip::new(btn, text(desc), tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 图标按钮样式（对齐绘制工具选择面板 button_style）
///
/// - 选中：浅色高亮（与工具栏选中态呼应）；
/// - 常态：悬停/按下浅色高亮。
fn icon_button_style(status: button::Status, selected: bool) -> button::Style {
    use button::Status;

    let background = if selected {
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

/// Tooltip 样式：深色背景 + 浅色文字（与绘制工具选择面板一致）
fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.08, 0.08, 0.10, 0.96))),
        border: iced_core::Border::default().rounded(4),
        text_color: Some(Color::from_rgba(0.95, 0.95, 0.95, 1.0)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shape_type_default_is_rectangle() {
        // 默认图形应为矩形（与工具栏 initial 状态一致）
        assert_eq!(ShapeType::default(), ShapeType::Rectangle);
    }

    #[test]
    fn test_shape_type_equality() {
        // 选中态判定依赖 PartialEq，确保三种类型互不混淆
        assert_ne!(ShapeType::Rectangle, ShapeType::Circle);
        assert_ne!(ShapeType::Circle, ShapeType::Triangle);
        assert_ne!(ShapeType::Rectangle, ShapeType::Triangle);
    }
}
