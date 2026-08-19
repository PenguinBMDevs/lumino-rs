//! 音轨选项卡右键上下文菜单
//!
//! 提供音轨列表中单个选项卡右键弹出的内嵌悬浮面板菜单。
//! 菜单以垂直图标栏形式显示，参考钢琴卷帘右键菜单的实现。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, tooltip};
use lumino_message::TrackContextMenuItem;

use crate::resources::icon::{self, Icon};
use crate::{Element, Message, Theme};

/// 图标按钮尺寸（宽高相同）
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 36;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// 面板宽度
const PANEL_WIDTH: f32 = BUTTON_SIZE + PANEL_PADDING * 2.0;
/// 面板与触发位置的垂直偏移
const MENU_OFFSET_Y: f32 = 8.0;

/// 深色菜单背景
const PANEL_BACKGROUND: Color = Color::from_rgba(0.06, 0.06, 0.08, 0.96);
/// Tooltip 深色背景
const TOOLTIP_BACKGROUND: Color = Color::from_rgba(0.08, 0.08, 0.10, 0.96);
/// 悬停背景
const HOVER_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
/// 按下背景
const PRESSED_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
/// 浅色文字
const TOOLTIP_TEXT_COLOR: Color = Color::from_rgba(0.95, 0.95, 0.95, 1.0);

/// 构建可直接嵌入 Stack 覆盖层的菜单面板内容
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

    let total_height = buttons.len() as f32 * BUTTON_SIZE
        + (buttons.len().saturating_sub(1)) as f32 * BUTTON_SPACING
        + PANEL_PADDING * 2.0;

    let panel = container(
        column(buttons)
            .spacing(BUTTON_SPACING)
            .align_x(Alignment::Center),
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
    // 避免触发下层的关闭覆盖层
    mouse_area(panel).on_press(Message::Null).into()
}

/// 构建单个菜单按钮（图标 + tooltip）
fn menu_button(track_id: usize, item: TrackContextMenuItem) -> Element<'static> {
    let icon =
        icon::view_with_size_and_theme(item_icon(item), ICON_SIZE, ICON_SIZE, Some(&Theme::Dark));

    let tooltip_text = item_label(item);

    let btn = button(icon)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press(
            lumino_ui_core::sidebar_event::Event::track_context_menu_item_clicked(track_id, item),
        )
        .style(|_theme: &Theme, status| button_style(status));

    tooltip::Tooltip::new(btn, tooltip_text, tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 菜单项对应的图标
const fn item_icon(item: TrackContextMenuItem) -> Icon {
    match item {
        TrackContextMenuItem::Delete => Icon::ContextMenuDelete,
        TrackContextMenuItem::Rename => Icon::PencilOutline,
        TrackContextMenuItem::SetColor => Icon::ContextMenuColorPalette,
        TrackContextMenuItem::SetChannel => Icon::ContextMenuChannel,
    }
}

/// 菜单项显示文本
fn item_label(item: TrackContextMenuItem) -> &'static str {
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

/// Tooltip 样式：深色背景 + 浅色文字
fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced_core::Background::Color(TOOLTIP_BACKGROUND)),
        border: iced_core::Border::default().rounded(4),
        text_color: Some(TOOLTIP_TEXT_COLOR),
        ..Default::default()
    }
}

/// 关闭背景：点击菜单外部区域关闭
///
/// 作为 Stack 的底层，覆盖整个父区域，点击时关闭菜单。
pub fn background_close_overlay<'a>() -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(lumino_ui_core::sidebar_event::Event::track_context_menu_closed())
        .into()
}

/// 将菜单面板定位在面板右侧
pub fn positioned_menu<'a>(track_id: usize, top_y: f32) -> Element<'a> {
    container(panel(track_id))
        .padding(Padding {
            top: top_y + MENU_OFFSET_Y,
            right: 8.0,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced_core::alignment::Horizontal::Right)
        .into()
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
    fn test_item_icon_mapping() {
        assert_eq!(
            item_icon(TrackContextMenuItem::Delete),
            Icon::ContextMenuDelete
        );
        assert_eq!(item_icon(TrackContextMenuItem::Rename), Icon::PencilOutline);
        assert_eq!(
            item_icon(TrackContextMenuItem::SetColor),
            Icon::ContextMenuColorPalette
        );
        assert_eq!(
            item_icon(TrackContextMenuItem::SetChannel),
            Icon::ContextMenuChannel
        );
    }

    #[test]
    fn test_menu_panel_returns_element() {
        let _element = panel(0);
    }

    #[test]
    fn test_background_close_overlay() {
        let _element = background_close_overlay();
    }

    #[test]
    fn test_positioned_menu() {
        let _element = positioned_menu(0, 100.0);
    }
}
