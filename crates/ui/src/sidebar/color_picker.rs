//! 音轨选项卡颜色选择器悬浮面板
//!
//! 参考右键菜单实现：使用 Stack 覆盖层，点击外部区域关闭，
//! 面板在触发音轨右侧弹出，色块为圆形，并包含斜杠恢复按钮。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, row, text};

use crate::{Element, Message, Theme};

/// 单个色块直径
const SWATCH_SIZE: f32 = 20.0;
/// 色块之间间距
const SWATCH_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// 每行色块数量
const SWATCHES_PER_ROW: usize = 4;
/// 面板与触发行的垂直偏移
const PANEL_OFFSET_Y: f32 = 8.0;
/// 面板与右侧边界的水平间距
const PANEL_RIGHT_MARGIN: f32 = 8.0;
/// 深色菜单背景
const PANEL_BACKGROUND: Color = Color::from_rgba(0.06, 0.06, 0.08, 0.96);
/// 斜杠按钮边框颜色
const SLASH_BORDER_COLOR: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// 预设音轨颜色
const TRACK_COLORS: [Color; 8] = [
    Color::from_rgb(0.85, 0.15, 0.15),
    Color::from_rgb(0.15, 0.75, 0.35),
    Color::from_rgb(0.15, 0.45, 0.85),
    Color::from_rgb(0.85, 0.75, 0.10),
    Color::from_rgb(0.75, 0.15, 0.75),
    Color::from_rgb(0.15, 0.75, 0.75),
    Color::from_rgb(0.95, 0.50, 0.15),
    Color::from_rgb(0.50, 0.50, 0.50),
];

/// 构建颜色选择器面板内容
pub fn panel(track_id: usize) -> Element<'static> {
    let mut rows: Vec<Element<'static>> = Vec::new();
    let mut current_row: Vec<Element<'static>> = Vec::new();

    for (idx, color) in TRACK_COLORS.iter().enumerate() {
        current_row.push(color_button(track_id, *color));
        if (idx + 1) % SWATCHES_PER_ROW == 0 {
            rows.push(row(current_row).spacing(SWATCH_SPACING).into());
            current_row = Vec::new();
        }
    }
    if !current_row.is_empty() {
        rows.push(row(current_row).spacing(SWATCH_SPACING).into());
    }

    rows.push(row![reset_button(track_id)].spacing(SWATCH_SPACING).into());

    let content = column(rows)
        .spacing(SWATCH_SPACING)
        .align_x(Alignment::Center);

    let panel = container(content)
        .padding(PANEL_PADDING)
        .style(|_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(PANEL_BACKGROUND)),
            border: iced_core::Border::default().rounded(8),
            ..Default::default()
        });

    // 用 mouse_area 包裹面板，吞掉面板上的点击事件，
    // 避免触发下层的关闭覆盖层
    mouse_area(panel).on_press(Message::Null).into()
}

/// 构建定位在右侧的颜色选择器覆盖层
pub fn positioned_panel<'a>(track_id: usize, top_y: f32) -> Element<'a> {
    container(panel(track_id))
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

/// 点击外部区域关闭颜色选择器
pub fn background_close_overlay<'a>(track_id: usize) -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(lumino_ui_core::sidebar_event::Event::track_color_picker_closed(track_id))
        .into()
}

/// 单个圆形颜色按钮
fn color_button(track_id: usize, color: Color) -> Element<'static> {
    button(
        Space::new()
            .width(Length::Fixed(SWATCH_SIZE))
            .height(Length::Fixed(SWATCH_SIZE)),
    )
    .width(Length::Fixed(SWATCH_SIZE))
    .height(Length::Fixed(SWATCH_SIZE))
    .on_press(lumino_ui_core::sidebar_event::Event::track_color_selected(
        track_id, color,
    ))
    .style(move |_theme: &Theme, _status| button::Style {
        background: Some(iced_core::Background::Color(color)),
        border: iced_core::Border {
            radius: (SWATCH_SIZE / 2.0).into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    })
    .into()
}

/// 恢复默认颜色按钮（斜杠图标）
fn reset_button(track_id: usize) -> Element<'static> {
    let slash = text("/")
        .size(12)
        .font(iced_core::Font {
            weight: iced_core::font::Weight::Bold,
            ..Default::default()
        })
        .style(|_theme: &Theme| text::Style {
            color: Some(Color::WHITE),
        });

    button(slash)
        .width(Length::Fixed(SWATCH_SIZE))
        .height(Length::Fixed(SWATCH_SIZE))
        .on_press(lumino_ui_core::sidebar_event::Event::track_color_reset(
            track_id,
        ))
        .style(|_theme: &Theme, _status| button::Style {
            background: Some(iced_core::Background::Color(Color::TRANSPARENT)),
            border: iced_core::Border {
                radius: (SWATCH_SIZE / 2.0).into(),
                width: 1.0,
                color: SLASH_BORDER_COLOR,
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_returns_element() {
        let _element = panel(0);
    }

    #[test]
    fn test_positioned_panel_returns_element() {
        let _element = positioned_panel(0, 100.0);
    }

    #[test]
    fn test_background_close_overlay_returns_element() {
        let _element = background_close_overlay(0);
    }

    #[test]
    fn test_track_colors_count_divisible_by_row() {
        assert_eq!(TRACK_COLORS.len() % SWATCHES_PER_ROW, 0);
    }
}
