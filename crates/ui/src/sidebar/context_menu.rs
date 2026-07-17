//! 音轨选项卡右键上下文菜单
//!
//! 提供音轨列表中单个选项卡右键弹出的悬浮面板菜单。
//! 菜单以垂直文本按钮形式显示，参考钢琴卷帘右键菜单的实现。

use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, mouse_area, text};
use lumino_message::TrackContextMenuItem;

use crate::{Element, Message, Theme};

/// 按钮高度
const BUTTON_HEIGHT: f32 = 32.0;
/// 面板宽度
const PANEL_WIDTH: f32 = 120.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// 按钮间距
const BUTTON_SPACING: f32 = 4.0;
/// 深色菜单背景
const PANEL_BACKGROUND: Color = Color::from_rgba(0.06, 0.06, 0.08, 0.96);
/// 悬停背景
const HOVER_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
/// 按下背景
const PRESSED_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
/// 浅色文字
const TEXT_COLOR: Color = Color::from_rgba(0.95, 0.95, 0.95, 1.0);

/// 构建可直接嵌入音轨行下方的菜单面板内容
pub fn panel(track_id: usize) -> Element<'static> {
    let buttons = [
        TrackContextMenuItem::Delete,
        TrackContextMenuItem::Rename,
        TrackContextMenuItem::SetColor,
        TrackContextMenuItem::SetChannel,
    ]
    .into_iter()
    .map(|item| menu_button(track_id, item))
    .collect::<Vec<_>>();

    let total_height = buttons.len() as f32 * BUTTON_HEIGHT
        + (buttons.len().saturating_sub(1)) as f32 * BUTTON_SPACING
        + PANEL_PADDING * 2.0;

    let panel = container(
        column(buttons)
            .spacing(BUTTON_SPACING)
            .align_x(Alignment::Start),
    )
    .padding(PANEL_PADDING)
    .width(Length::Fixed(PANEL_WIDTH))
    .height(Length::Fixed(total_height))
    .style(|_theme: &Theme| container::Style {
        background: Some(iced_core::Background::Color(PANEL_BACKGROUND)),
        border: iced_core::Border::default().rounded(8),
        ..Default::default()
    });

    // 用 mouse_area 包裹面板，吞掉菜单背景上的点击事件，
    // 避免触发下层的其他交互。
    mouse_area(panel).on_press(Message::Null).into()
}

/// 构建单个菜单按钮
fn menu_button(track_id: usize, item: TrackContextMenuItem) -> Element<'static> {
    let label = item_label(item);

    button(text(label).size(14).style(|_theme: &Theme| text::Style {
        color: Some(TEXT_COLOR),
    }))
    .width(Length::Fill)
    .height(Length::Fixed(BUTTON_HEIGHT))
    .on_press(lumino_ui_core::sidebar_event::Event::track_context_menu_item_clicked(track_id, item))
    .style(|_theme: &Theme, status| button_style(status))
    .into()
}

/// 菜单项显示文本
const fn item_label(item: TrackContextMenuItem) -> &'static str {
    match item {
        TrackContextMenuItem::Delete => "删除",
        TrackContextMenuItem::Rename => "重命名",
        TrackContextMenuItem::SetColor => "设置颜色",
        TrackContextMenuItem::SetChannel => "设置通道",
    }
}

/// 按钮样式
fn button_style(status: button::Status) -> button::Style {
    use button::Status;

    let background = match status {
        Status::Hovered => HOVER_BACKGROUND,
        Status::Pressed => PRESSED_BACKGROUND,
        _ => Color::TRANSPARENT,
    };

    button::Style {
        border: iced_core::Border::default().rounded(6),
        ..Default::default()
    }
    .with_background(background)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_label_mapping() {
        assert_eq!(item_label(TrackContextMenuItem::Delete), "删除");
        assert_eq!(item_label(TrackContextMenuItem::Rename), "重命名");
        assert_eq!(item_label(TrackContextMenuItem::SetColor), "设置颜色");
        assert_eq!(item_label(TrackContextMenuItem::SetChannel), "设置通道");
    }

    #[test]
    fn test_menu_panel_returns_element() {
        let _element = panel(0);
    }
}
