//! 素材库右键上下文菜单
//!
//! 提供右侧栏素材库面板中单个素材项右键弹出的内嵌悬浮面板菜单。
//! 视觉风格参考音轨选项卡右键菜单（`sidebar/context_menu.rs`）：
//! 垂直图标栏 + tooltip，深色圆角背景。
//!
//! 菜单项：
//! - 重命名（仅用户素材可用，内置素材置灰）
//! - 删除（仅用户素材可用，内置素材置灰）
//! - 上传到云（全部素材可用）

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, tooltip};
use lumino_message::{MaterialContextMenuItem, RightSidebarAction};

use crate::resources::icon::{self, Icon};
use crate::{Element, Message, Theme};

/// 图标按钮尺寸（宽高相同，与音轨选项卡右键菜单一致）
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 36;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
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
///
/// `index` 为素材列表索引；`can_edit` 表示是否可重命名/删除（用户素材 = true）。
pub fn panel(index: usize, can_edit: bool) -> Element<'static> {
    let buttons = [
        menu_button(index, MaterialContextMenuItem::Rename, can_edit),
        menu_button(index, MaterialContextMenuItem::Delete, can_edit),
        menu_button(index, MaterialContextMenuItem::UploadToCloud, true),
    ];

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
///
/// `enabled = false` 时按钮不响应点击（内置素材的重命名/删除）。
fn menu_button(
    index: usize,
    item: MaterialContextMenuItem,
    enabled: bool,
) -> Element<'static> {
    let icon_view =
        icon::view_with_size_and_theme(item_icon(item), ICON_SIZE, ICON_SIZE, Some(&Theme::Dark));

    let tooltip_text = item_label(item);

    let btn = button(icon_view)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press_maybe(enabled.then_some(Message::RightSidebar(
            RightSidebarAction::MaterialContextMenuItemClicked(index, item),
        )))
        .style(move |_theme: &Theme, status| button_style(status, enabled));

    // 素材库面板位于屏幕右侧：tooltip 向左显示，避免超出屏幕右缘
    tooltip::Tooltip::new(btn, tooltip_text, tooltip::Position::Left)
        .style(tooltip_style)
        .into()
}

/// 菜单项对应的图标
const fn item_icon(item: MaterialContextMenuItem) -> Icon {
    match item {
        MaterialContextMenuItem::Rename => Icon::PencilOutline,
        MaterialContextMenuItem::Delete => Icon::ContextMenuDelete,
        MaterialContextMenuItem::UploadToCloud => Icon::ContextMenuUploadToCloud,
    }
}

/// 菜单项显示文本
fn item_label(item: MaterialContextMenuItem) -> &'static str {
    match item {
        MaterialContextMenuItem::Rename => "重命名",
        MaterialContextMenuItem::Delete => "删除",
        MaterialContextMenuItem::UploadToCloud => "上传到云",
    }
}

/// 按钮样式（禁用态无 hover/按下反馈）
fn button_style(status: button::Status, enabled: bool) -> button::Style {
    use button::Status;

    let background = if !enabled {
        Color::TRANSPARENT
    } else {
        match status {
            Status::Hovered => HOVER_BACKGROUND,
            Status::Pressed => PRESSED_BACKGROUND,
            _ => Color::TRANSPARENT,
        }
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
        .on_press(Message::RightSidebar(
            RightSidebarAction::MaterialContextMenuClosed,
        ))
        .into()
}

/// 将菜单面板定位在鼠标位置附近
///
/// `pos` 为鼠标在窗口中的逻辑坐标（由 Host 在消息处理时注入）。
/// 菜单以鼠标位置的 Y 坐标为基准，贴面板右缘显示（素材库面板位于屏幕右侧）。
/// 若无位置信息，则默认显示在面板右上角。
pub fn positioned_menu<'a>(
    index: usize,
    can_edit: bool,
    pos: Option<(f32, f32)>,
) -> Element<'a> {
    let top = pos.map_or(MENU_OFFSET_Y, |(_, y)| y + MENU_OFFSET_Y);

    container(panel(index, can_edit))
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
        assert_eq!(item_label(MaterialContextMenuItem::Rename), "重命名");
        assert_eq!(item_label(MaterialContextMenuItem::Delete), "删除");
        assert_eq!(
            item_label(MaterialContextMenuItem::UploadToCloud),
            "上传到云"
        );
    }

    #[test]
    fn test_item_icon_mapping() {
        assert_eq!(
            item_icon(MaterialContextMenuItem::Rename),
            Icon::PencilOutline
        );
        assert_eq!(
            item_icon(MaterialContextMenuItem::Delete),
            Icon::ContextMenuDelete
        );
        assert_eq!(
            item_icon(MaterialContextMenuItem::UploadToCloud),
            Icon::ContextMenuUploadToCloud
        );
    }

    #[test]
    fn test_menu_panel_returns_element() {
        let _element = panel(0, true);
        let _element = panel(0, false);
    }

    #[test]
    fn test_background_close_overlay() {
        let _element = background_close_overlay();
    }

    #[test]
    fn test_positioned_menu() {
        let _element = positioned_menu(0, true, None);
        let _element = positioned_menu(1, false, Some((100.0, 200.0)));
    }
}
