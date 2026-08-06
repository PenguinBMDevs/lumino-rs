//! 右侧栏视图渲染

use iced_core::{Alignment, Color, Length};
use iced_widget::{Column, Row, Space, button, container, mouse_area, tooltip};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::RightSidebarAction;

use crate::resources::icon::{self, Icon};
use crate::right_sidebar::core::{RESIZE_HANDLE_WIDTH, ROUTE_BAR_WIDTH, RightSidebar};
use crate::{Element, Message, Theme, window};

/// 渲染右侧栏视图（图标按钮列 + 展开面板）
pub fn view<'a>(
    right_sidebar: &'a RightSidebar,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let t = main_translations(language);

    // 图标列（垂直排列按钮）
    let mut col = Column::new()
        .spacing(2)
        .width(ROUTE_BAR_WIDTH)
        .height(Length::Fill);

    // 切换面板按钮
    let toggle_btn = if right_sidebar.panel_visible {
        tool_button(
            Icon::AngleRight,
            t.right_sidebar_hide,
            Message::RightSidebar(RightSidebarAction::TogglePanel),
            window,
        )
    } else {
        tool_button(
            Icon::AngleRight,
            t.right_sidebar_show,
            Message::RightSidebar(RightSidebarAction::TogglePanel),
            window,
        )
    };
    col = col.push(toggle_btn);

    // 图片转 MIDI 按钮（仅在面板展开后可见）
    if right_sidebar.panel_visible {
        col = col.push(tool_button(
            Icon::ImageToMidi,
            t.tool_image_to_midi,
            Message::RightSidebar(RightSidebarAction::ImageToMidiClicked),
            window,
        ));
    }

    // 弹性空间占据剩余高度
    col = col.push(Space::new().height(Length::Fill));

    // 图标列容器
    let route_bar = container(col)
        .width(ROUTE_BAR_WIDTH)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weaker.color)
        });

    // 如果面板可见，渲染内容面板 + 调整手柄
    if right_sidebar.panel_visible {
        // 面板内容（当前只有占位空间，后续可以扩展更多功能）
        let content = container(Space::new().height(Length::Fill))
            .width(Length::Fixed(
                right_sidebar.panel_width - RESIZE_HANDLE_WIDTH,
            ))
            .height(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default().background(palette.background.weakest.color)
            });

        // 调整大小手柄（放在面板右侧，与左侧栏一致）
        let resize_handle = mouse_area(
            container(
                Space::new()
                    .width(Length::Fixed(RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill),
            )
            .style(move |_theme: &Theme| {
                let bg = if right_sidebar.is_resizing {
                    palette.primary.strong.color
                } else {
                    palette.background.weakest.color
                };
                container::Style::default().background(bg)
            }),
        )
        .interaction(iced_core::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::RightSidebar(RightSidebarAction::ResizeDragStarted))
        .on_release(Message::RightSidebar(RightSidebarAction::ResizeDragEnded));

        // 面板内容 + 调整手柄（调整手柄在右侧）
        let panel_with_handle = Row::new().push(content).push(resize_handle);
        let panel_container = container(panel_with_handle)
            .width(Length::Fixed(right_sidebar.panel_width))
            .height(Length::Fill);

        // 顺序：图标列 → 面板内容（包含右侧调整手柄）
        Row::new()
            .push(route_bar)
            .push(panel_container)
            .height(Length::Fill)
            .into()
    } else {
        // 面板不可见，只显示图标列
        route_bar.into()
    }
}

/// 右侧栏工具栏按钮（带提示）
fn tool_button<'a>(
    icon_enum: Icon,
    tooltip_text: &'a str,
    on_press: Message,
    window: &'a window::Window,
) -> Element<'a> {
    let btn = button(
        container(icon::view_with_size_and_theme(
            icon_enum,
            20,
            20,
            Some(&window.theme),
        ))
        .width(Length::Fill)
        .height(Length::Fixed(48.0))
        .align_y(Alignment::Center),
    )
    .width(ROUTE_BAR_WIDTH)
    .height(Length::Fixed(48.0))
    .padding(0)
    .style(move |theme: &Theme, status| {
        let p = theme.extended_palette();
        let text_color = match status {
            button::Status::Hovered | button::Status::Pressed => p.background.base.color,
            _ => p.background.weakest.color,
        };
        button::Style {
            text_color,
            border: iced_core::Border {
                radius: 0.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        }
        .with_background(Color::TRANSPARENT)
    })
    .on_press(on_press);

    crate::widget::with_tooltip(btn, tooltip_text, tooltip::Position::Left).into()
}
