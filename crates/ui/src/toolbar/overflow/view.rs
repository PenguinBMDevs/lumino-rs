//! 溢出菜单视图渲染
//!
//! 包括溢出面板的网格布局、按钮样式、tooltip 样式以及定位函数。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, row, tooltip};

use crate::resources::icon;
use crate::toolbar::overflow::state::{OverflowMenuItem, ToolbarGroup};
use crate::toolbar::{Event, Toolbar};
use crate::{Element, Message, Theme};

/// 图标按钮尺寸（宽高相同）
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 20;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// Tooltip 深色背景
const TOOLTIP_BACKGROUND: Color = Color::from_rgba(0.08, 0.08, 0.10, 0.96);
/// 浅色悬停/按下颜色，用于深色按钮背景
const HOVER_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
const PRESSED_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
/// 浅色文字，用于深色 Tooltip 背景
const TOOLTIP_TEXT_COLOR: Color = Color::from_rgba(0.95, 0.95, 0.95, 1.0);

impl Toolbar {
    /// 渲染溢出菜单面板
    pub fn render_overflow_menu<'a>(
        &'a self,
        hidden_groups: &[ToolbarGroup],
        has_selection: bool,
        language: lumino_core::i18n::Language,
        panel_background: Color,
        theme: &'a Theme,
        arrangement_mode: bool,
    ) -> Element<'a> {
        let buttons = self.build_overflow_buttons(
            hidden_groups, has_selection, language, theme, arrangement_mode,
        );
        let (grid, panel_width, panel_height) = Self::build_overflow_panel(buttons);

        let panel = container(grid)
            .padding(PANEL_PADDING)
            .width(Length::Fixed(panel_width))
            .height(Length::Fixed(panel_height))
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(panel_background)),
                border: iced_core::Border::default().rounded(8),
                ..Default::default()
            });

        mouse_area(panel).on_press(Message::Null).into()
    }

    /// 收集隐藏分组的按钮列表
    fn build_overflow_buttons<'a>(
        &'a self,
        hidden_groups: &[ToolbarGroup],
        has_selection: bool,
        language: lumino_core::i18n::Language,
        theme: &'a Theme,
        arrangement_mode: bool,
    ) -> Vec<Element<'a>> {
        hidden_groups
            .iter()
            .flat_map(|g| self.group_overflow_items(*g, has_selection, language, arrangement_mode))
            .map(|item| overflow_menu_button(item, theme))
            .collect()
    }

    /// 构建溢出面板的网格布局，返回 (网格元素, 面板宽度, 面板高度)
    fn build_overflow_panel(buttons: Vec<Element>) -> (Element, f32, f32) {
        let button_count = buttons.len().max(1);
        let cols = ((button_count as f32).sqrt().ceil() as usize).max(1);
        let rows = button_count.div_ceil(cols);

        let mut btn_iter = buttons.into_iter();
        let mut columns: Vec<Element> = Vec::with_capacity(cols);
        for _ in 0..cols {
            let col_buttons: Vec<Element> = btn_iter.by_ref().take(rows).collect();
            if col_buttons.is_empty() {
                break;
            }
            columns.push(
                column(col_buttons)
                    .spacing(BUTTON_SPACING)
                    .align_x(Alignment::Center)
                    .into(),
            );
        }

        let grid: Element = row(columns)
            .spacing(BUTTON_SPACING)
            .align_y(Alignment::Start)
            .into();

        let panel_width = cols as f32 * BUTTON_SIZE
            + (cols.saturating_sub(1)) as f32 * BUTTON_SPACING
            + PANEL_PADDING * 2.0;
        let panel_height = rows as f32 * BUTTON_SIZE
            + (rows.saturating_sub(1)) as f32 * BUTTON_SPACING
            + PANEL_PADDING * 2.0;

        (grid, panel_width, panel_height)
    }
}

/// 构建单个溢出菜单按钮
fn overflow_menu_button(item: OverflowMenuItem, theme: &Theme) -> Element<'static> {
    let icon = icon::view_with_size_and_theme(item.icon, ICON_SIZE, ICON_SIZE, Some(theme));
    let btn = button(icon)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .style(move |_theme: &Theme, status| button_style(status, item.enabled));

    let btn = if item.enabled {
        btn.on_press(item.on_press)
    } else {
        btn
    };

    tooltip::Tooltip::new(btn, item.tooltip, tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 按钮样式（无选中/禁用时背景透明）
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
pub fn background_close_overlay<'a>() -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Event::close_overflow_menu())
        .into()
}

/// 将菜单面板定位在容器右上角
pub fn positioned_overflow_menu<'a>(menu: Element<'a>, toolbar_height: f32) -> Element<'a> {
    container(menu)
        .padding(Padding {
            top: toolbar_height,
            right: 4.0,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced_core::alignment::Horizontal::Right)
        .align_y(iced_core::alignment::Vertical::Top)
        .into()
}
