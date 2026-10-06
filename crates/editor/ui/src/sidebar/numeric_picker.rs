//! 音轨数值选择器（端口/通道共用）
//!
//! 端口与通道都是「内部 0 基、UI 显示 1 基」的 16 选 1 网格（Domino/DAW 惯例），
//! 面板布局/定位/关闭覆盖层完全一致。本模块收敛公共实现，具体事件由调用方
//! 以构造函数注入（`selected(track_id, value)`），避免第二份复制粘贴。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, row, text};

use super::core::Sidebar;
use crate::{Element, Message, Theme};

/// 单个选项按钮尺寸
const ITEM_SIZE: f32 = 30.0;
/// 按钮间距
const ITEM_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// 每行按钮数（16 项 → 4×4）
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

/// 构建数值选择器面板内容（当前值高亮；显示值 = 内部值 + 1）。
pub fn panel(
    track_id: usize,
    current: u8,
    choices: u8,
    selected: fn(usize, u8) -> Message,
) -> Element<'static> {
    let mut rows: Vec<Element<'static>> = Vec::new();
    let mut current_row: Vec<Element<'static>> = Vec::new();

    for value in 0..choices {
        current_row.push(option_button(track_id, value, value == current, selected));
        if (usize::from(value) + 1) % ITEMS_PER_ROW == 0 {
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

    // 吞掉面板上的左右键点击，避免触发下层的关闭覆盖层/面板空白菜单
    mouse_area(panel)
        .on_press(Message::Null)
        .on_right_press(Message::Null)
        .into()
}

/// 将选择器面板定位到触发音轨右侧（`top_y` 为音轨行顶部）。
pub fn position<'a>(content: Element<'static>, top_y: f32) -> Element<'a> {
    container(content)
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

/// 点击外部区域关闭（`close` 由调用方注入对应关闭事件）。
///
/// 左右键均处理：右键若不吞掉会穿透到面板空白层，置位「找回删除音轨」菜单，
/// 待选择器关闭后残留浮层。
pub fn background_close_overlay<'a>(close: Message) -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(close.clone())
        .on_right_press(close)
        .into()
}

/// 单个数值按钮：显示号 = 内部值 + 1（1..=choices），当前值高亮。
fn option_button(
    track_id: usize,
    value: u8,
    selected: bool,
    chosen: fn(usize, u8) -> Message,
) -> Element<'static> {
    button(
        text(Sidebar::display_number(value).to_string())
            .size(13)
            .style(|_theme: &Theme| text::Style {
                color: Some(TEXT_COLOR),
            }),
    )
    .width(Length::Fixed(ITEM_SIZE))
    .height(Length::Fixed(ITEM_SIZE))
    .on_press(chosen(track_id, value))
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

    /// 内部 0 基 → 显示 1 基（端口/通道共用映射）。
    #[test]
    fn display_number_maps_zero_based_to_one_based() {
        assert_eq!(Sidebar::display_number(0), 1);
        assert_eq!(Sidebar::display_number(15), 16);
    }

    /// 16 项能整行排布（4×4）。
    #[test]
    fn sixteen_choices_fit_rows() {
        assert_eq!(16 % ITEMS_PER_ROW, 0);
    }

    #[test]
    fn panel_builders_return_elements() {
        let selected = lumino_ui_core::sidebar_event::Event::track_port_selected;
        let _ = panel(1, 0, Sidebar::PORT_CHOICES, selected);
        let _ = position(panel(1, 3, Sidebar::PORT_CHOICES, selected), 100.0);
        let _ = background_close_overlay(
            lumino_ui_core::sidebar_event::Event::track_port_picker_closed(1),
        );
    }
}
