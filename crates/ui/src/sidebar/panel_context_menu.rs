//! 音轨列表面板空白区域右键上下文菜单
//!
//! 与 `context_menu.rs`（音轨选项卡右键菜单）对应：本模块处理音轨列表
//! 空白区域的右键浮动菜单，提供工程级音轨管理动作（如找回删除音轨）。
//!
//! 菜单以垂直图标栏形式显示，参考 `context_menu.rs` 的视觉风格。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, tooltip};
use lumino_message::PanelContextMenuItem;

use crate::resources::icon::{self, Icon};
use crate::{Element, Message, Theme};

/// 图标按钮尺寸（宽高相同，与音轨选项卡右键菜单一致）
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 36;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// 面板宽度
const PANEL_WIDTH: f32 = BUTTON_SIZE + PANEL_PADDING * 2.0;
/// 面板与触发位置的偏移
const MENU_OFFSET_X: f32 = 4.0;
const MENU_OFFSET_Y: f32 = 4.0;

/// 深色菜单背景（与音轨选项卡右键菜单一致）
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
fn panel() -> Element<'static> {
    let buttons = [PanelContextMenuItem::RecoverDeletedTrack]
        .into_iter()
        .map(menu_button)
        .collect::<Vec<_>>();

    let total_height = buttons.len() as f32 * BUTTON_SIZE + PANEL_PADDING * 2.0;

    let panel = container(column(buttons).spacing(0).align_x(Alignment::Center))
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
fn menu_button(item: PanelContextMenuItem) -> Element<'static> {
    let icon_view =
        icon::view_with_size_and_theme(item_icon(item), ICON_SIZE, ICON_SIZE, Some(&Theme::Dark));

    let tooltip_text = item_label(item);

    let btn = button(icon_view)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press(lumino_ui_core::sidebar_event::Event::panel_context_menu_item_clicked(item))
        .style(|_theme: &Theme, status| button_style(status));

    tooltip::Tooltip::new(btn, tooltip_text, tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 菜单项对应的图标
const fn item_icon(item: PanelContextMenuItem) -> Icon {
    match item {
        PanelContextMenuItem::RecoverDeletedTrack => Icon::ContextMenuRecoverTrack,
    }
}

/// 菜单项显示文本
fn item_label(item: PanelContextMenuItem) -> &'static str {
    match item {
        PanelContextMenuItem::RecoverDeletedTrack => "找回删除音轨",
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
        .on_press(lumino_ui_core::sidebar_event::Event::panel_context_menu_closed())
        .into()
}

/// 将菜单面板定位在鼠标位置附近
///
/// `pos` 为鼠标在窗口中的逻辑坐标（由 `window_ctx.cursor_position` 提供）。
/// 菜单以鼠标位置的 Y 坐标为基准，向右上角偏移，显示在面板右侧区域。
/// 若无位置信息，则默认显示在面板右上角。
pub fn positioned_menu<'a>(pos: Option<(f32, f32)>) -> Element<'a> {
    let top = pos.map_or(MENU_OFFSET_Y, |(_, y)| y + MENU_OFFSET_Y);

    container(panel())
        .padding(Padding {
            top,
            right: MENU_OFFSET_X,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced_core::alignment::Horizontal::Right)
        .align_y(iced_core::alignment::Vertical::Top)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_label_mapping() {
        assert_eq!(
            item_label(PanelContextMenuItem::RecoverDeletedTrack),
            "找回删除音轨"
        );
    }

    #[test]
    fn test_item_icon_mapping() {
        assert_eq!(
            item_icon(PanelContextMenuItem::RecoverDeletedTrack),
            Icon::ContextMenuRecoverTrack
        );
    }

    #[test]
    fn test_background_close_overlay() {
        let _element = background_close_overlay();
    }

    #[test]
    fn test_positioned_menu_without_pos() {
        let _element = positioned_menu(None);
    }

    #[test]
    fn test_positioned_menu_with_pos() {
        let _element = positioned_menu(Some((100.0, 200.0)));
    }
}
