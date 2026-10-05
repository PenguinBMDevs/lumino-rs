//! 音轨端口选择器悬浮面板
//!
//! 与颜色选择器同构：Stack 覆盖层 + 点击外部关闭。端口显示 `1..=16`
//! （内部 `0..=15`），对齐 Domino 与各类 DAW 的端口编号惯例。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, row, text};

use super::core::Sidebar;
use crate::{Element, Message, Theme};

/// 单个端口按钮尺寸
const ITEM_SIZE: f32 = 30.0;
/// 按钮间距
const ITEM_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// 每行端口按钮数（16 项 → 4×4）
const ITEMS_PER_ROW: usize = 4;
/// 面板与触发行的垂直偏移
const PANEL_OFFSET_Y: f32 = 8.0;
/// 面板与右侧边界的水平间距
const PANEL_RIGHT_MARGIN: f32 = 8.0;
/// 深色菜单背景
const PANEL_BACKGROUND: Color = Color::from_rgba(0.06, 0.06, 0.08, 0.96);
/// 悬停背景
const HOVER_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
/// 按下背景
const PRESSED_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
/// 浅色文字
const TEXT_COLOR: Color = Color::from_rgba(0.95, 0.95, 0.95, 1.0);

/// 构建端口选择器面板内容（当前端口高亮）。
pub fn panel(track_id: usize, current_port: u8) -> Element<'static> {
    let mut rows: Vec<Element<'static>> = Vec::new();
    let mut current_row: Vec<Element<'static>> = Vec::new();

    for port in 0..Sidebar::PORT_CHOICES {
        current_row.push(port_button(track_id, port, port == current_port));
        if (usize::from(port) + 1) % ITEMS_PER_ROW == 0 {
            rows.push(row(current_row).spacing(ITEM_SPACING).into());
            current_row = Vec::new();
        }
    }
    if !current_row.is_empty() {
        rows.push(row(current_row).spacing(ITEM_SPACING).into());
    }

    let content = column(rows)
        .spacing(ITEM_SPACING)
        .align_x(Alignment::Center);

    let panel = container(content)
        .padding(PANEL_PADDING)
        .style(|_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(PANEL_BACKGROUND)),
            border: iced_core::Border::default().rounded(8),
            ..Default::default()
        });

    // 吞掉面板上的点击，避免触发下层的关闭覆盖层
    mouse_area(panel).on_press(Message::Null).into()
}

/// 构建定位在触发音轨右侧的端口选择器覆盖层。
pub fn positioned_panel<'a>(track_id: usize, current_port: u8, top_y: f32) -> Element<'a> {
    container(panel(track_id, current_port))
        .padding(Padding {
            top: top_y + PANEL_OFFSET_Y,
            right: PANEL_RIGHT_MARGIN,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced_core::alignment::Horizontal::Right)
        .into()
}

/// 点击外部区域关闭端口选择器。
pub fn background_close_overlay<'a>(track_id: usize) -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(lumino_ui_core::sidebar_event::Event::track_port_picker_closed(track_id))
        .into()
}

/// 单个端口按钮：显示号 = 内部值 + 1（1..=16）。
fn port_button(track_id: usize, port: u8, selected: bool) -> Element<'static> {
    button(
        text(Sidebar::display_port_number(port).to_string())
            .size(13)
            .style(|_theme: &Theme| text::Style {
                color: Some(TEXT_COLOR),
            }),
    )
    .width(Length::Fixed(ITEM_SIZE))
    .height(Length::Fixed(ITEM_SIZE))
    .on_press(lumino_ui_core::sidebar_event::Event::track_port_selected(
        track_id, port,
    ))
    .style(move |theme: &Theme, status| {
        use button::Status;
        let palette = theme.extended_palette();
        let background = if selected {
            Some(iced_core::Background::Color(palette.primary.strong.color))
        } else {
            match status {
                Status::Hovered => Some(iced_core::Background::Color(HOVER_BACKGROUND)),
                Status::Pressed => Some(iced_core::Background::Color(PRESSED_BACKGROUND)),
                _ => None,
            }
        };
        button::Style {
            background,
            border: iced_core::Border::default().rounded(6),
            ..Default::default()
        }
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 内部 0..=15 → 显示 1..=16。
    #[test]
    fn display_port_number_maps_zero_based_to_one_based() {
        assert_eq!(Sidebar::display_port_number(0), 1);
        assert_eq!(Sidebar::display_port_number(15), 16);
    }

    /// 可选端口数覆盖产品上限且能整行排布。
    #[test]
    fn port_choices_covers_product_limit() {
        assert_eq!(Sidebar::PORT_CHOICES, 16, "产品上限 16 端口（0..=15）");
        assert_eq!(usize::from(Sidebar::PORT_CHOICES) % ITEMS_PER_ROW, 0, "4×4");
    }

    #[test]
    fn test_panel_returns_element() {
        let _element = panel(0, 0);
    }

    #[test]
    fn test_positioned_panel_returns_element() {
        let _element = positioned_panel(0, 3, 100.0);
    }

    #[test]
    fn test_background_close_overlay_returns_element() {
        let _element = background_close_overlay(0);
    }
}
