//! 钢琴卷帘右键上下文菜单
//!
//! 提供钢琴卷帘区域内右键弹出的内嵌悬浮面板菜单。
//! 菜单以垂直图标栏形式显示在右键位置附近，
//! 模仿 ../yinhe 的选中后悬浮面板风格。

use iced_core::{Alignment, Color, Length, Padding, Point};
use iced_widget::{Space, button, column, container, mouse_area, tooltip};
use lumino_message::{PianoRollContextMenuAction, PianoRollContextMenuItem};

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
/// 面板与鼠标位置的水平偏移（避免遮挡光标）
const MENU_OFFSET_X: f32 = 16.0;
/// 面板与鼠标位置的垂直偏移
const MENU_OFFSET_Y: f32 = 16.0;

/// 深色菜单背景，保证在浅色主题下也能明显区分
const PANEL_BACKGROUND: Color = Color::from_rgba(0.06, 0.06, 0.08, 0.96);
/// Tooltip 深色背景
const TOOLTIP_BACKGROUND: Color = Color::from_rgba(0.08, 0.08, 0.10, 0.96);
/// 浅色悬停/按下颜色，用于深色按钮背景
const HOVER_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
const PRESSED_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
/// 浅色文字，用于深色 Tooltip 背景
const TOOLTIP_TEXT_COLOR: Color = Color::from_rgba(0.95, 0.95, 0.95, 1.0);

/// 钢琴卷帘上下文菜单状态
#[derive(Debug, Clone, Default)]
pub struct PianoRollContextMenuState {
    /// 菜单是否打开
    pub open: bool,
    /// 菜单打开位置（canvas 局部坐标）
    pub position: Option<Point>,
}

impl PianoRollContextMenuState {
    /// 打开菜单
    pub fn open(&mut self, position: Point) {
        self.open = true;
        self.position = Some(position);
    }

    /// 关闭菜单
    pub fn close(&mut self) {
        self.open = false;
        self.position = None;
    }

    /// 切换菜单状态
    #[allow(dead_code)]
    pub fn toggle(&mut self, position: Point) {
        if self.open {
            self.close();
        } else {
            self.open(position);
        }
    }
}

/// 渲染上下文菜单覆盖层
pub fn view(position: Point) -> Element<'static> {
    let adjusted_position = Point::new(position.x + MENU_OFFSET_X, position.y + MENU_OFFSET_Y);

    let menu_panel = menu_panel();

    container(menu_panel)
        .padding(Padding {
            top: adjusted_position.y,
            right: 0.0,
            bottom: 0.0,
            left: adjusted_position.x,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 关闭背景：点击菜单外部区域关闭
///
/// 作为 Stack 的底层，覆盖整个编辑器区域，点击时关闭菜单。
/// 菜单面板应放在此背景之上，使菜单内部的点击由按钮处理。
pub fn background_close_overlay<'a>() -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::PianoRollContextMenu(
            PianoRollContextMenuAction::Close,
        ))
        .into()
}

/// 构建菜单面板内容
fn menu_panel() -> Element<'static> {
    let buttons = [
        PianoRollContextMenuItem::Cut,
        PianoRollContextMenuItem::Copy,
        PianoRollContextMenuItem::Paste,
        PianoRollContextMenuItem::Delete,
        PianoRollContextMenuItem::SelectAll,
        PianoRollContextMenuItem::BatchEdit,
    ]
    .into_iter()
    .map(menu_button)
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
    // 避免触发下层的关闭覆盖层。按钮作为子元素仍会优先响应点击。
    mouse_area(panel).on_press(Message::Null).into()
}

/// 构建单个菜单按钮
fn menu_button(item: PianoRollContextMenuItem) -> Element<'static> {
    // 面板固定为深色，图标始终按暗色主题反色渲染，保证亮/暗主题下均为浅色可见
    let icon = lumino_ui_core::resources::icon::view_with_size_and_theme(
        item_icon(item),
        ICON_SIZE,
        ICON_SIZE,
        Some(&Theme::Dark),
    );

    let tooltip_text = item_label(item);

    let btn = button(icon)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .on_press(Message::PianoRollContextMenu(
            PianoRollContextMenuAction::ItemClicked(item),
        ))
        .style(|_theme: &Theme, status| button_style(status));

    tooltip::Tooltip::new(btn, tooltip_text, tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 菜单项对应的图标
const fn item_icon(item: PianoRollContextMenuItem) -> lumino_ui_core::resources::icon::Icon {
    use lumino_ui_core::resources::icon::Icon;
    match item {
        PianoRollContextMenuItem::Cut => Icon::ContextMenuCut,
        PianoRollContextMenuItem::Copy => Icon::ContextMenuCopy,
        PianoRollContextMenuItem::Paste => Icon::ContextMenuPaste,
        PianoRollContextMenuItem::Delete => Icon::ContextMenuDelete,
        PianoRollContextMenuItem::SelectAll => Icon::ContextMenuSelectAll,
        PianoRollContextMenuItem::BatchEdit => Icon::Gear,
    }
}

/// 菜单项显示文本
fn item_label(item: PianoRollContextMenuItem) -> &'static str {
    match item {
        PianoRollContextMenuItem::Cut => "剪切",
        PianoRollContextMenuItem::Copy => "复制",
        PianoRollContextMenuItem::Paste => "粘贴",
        PianoRollContextMenuItem::Delete => "删除",
        PianoRollContextMenuItem::SelectAll => "全选",
        PianoRollContextMenuItem::BatchEdit => "批量编辑",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_open_close() {
        let mut state = PianoRollContextMenuState::default();
        assert!(!state.open);
        assert!(state.position.is_none());

        state.open(Point::new(100.0, 200.0));
        assert!(state.open);
        assert_eq!(state.position, Some(Point::new(100.0, 200.0)));

        state.close();
        assert!(!state.open);
        assert!(state.position.is_none());
    }

    #[test]
    fn test_item_icon_mapping() {
        assert_eq!(
            item_icon(PianoRollContextMenuItem::Cut),
            lumino_ui_core::resources::icon::Icon::ContextMenuCut
        );
        assert_eq!(
            item_icon(PianoRollContextMenuItem::Copy),
            lumino_ui_core::resources::icon::Icon::ContextMenuCopy
        );
        assert_eq!(
            item_icon(PianoRollContextMenuItem::Paste),
            lumino_ui_core::resources::icon::Icon::ContextMenuPaste
        );
        assert_eq!(
            item_icon(PianoRollContextMenuItem::Delete),
            lumino_ui_core::resources::icon::Icon::ContextMenuDelete
        );
        assert_eq!(
            item_icon(PianoRollContextMenuItem::SelectAll),
            lumino_ui_core::resources::icon::Icon::ContextMenuSelectAll
        );
        assert_eq!(
            item_icon(PianoRollContextMenuItem::BatchEdit),
            lumino_ui_core::resources::icon::Icon::Gear
        );
    }

    #[test]
    fn test_menu_panel_size() {
        let _element = menu_panel();
    }

    #[test]
    fn test_view_returns_element() {
        let _element = view(Point::new(50.0, 60.0));
    }
}
