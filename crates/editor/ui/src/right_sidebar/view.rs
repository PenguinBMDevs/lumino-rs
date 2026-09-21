//! 右侧栏视图渲染

mod i2m_config;
mod i2m_panel;
mod sidebar_button;

use i2m_panel::i2m_panel;
use sidebar_button::sidebar_button;

use iced_core::{Alignment, Color, Length};
use iced_widget::{Column, Row, Space, button, container, mouse_area, tooltip};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::{I2mConfigField, RightSidebarAction};

use crate::resources::icon::{self, Icon};
use crate::right_sidebar::core::{
    PALETTE_ALGORITHMS, RESIZE_HANDLE_WIDTH, ROUTE_BAR_WIDTH, RightSidebar, RightSidebarPanel,
};
use crate::widget;
use crate::{Element, Message, Theme, window};

/// 渲染右侧栏视图（图标按钮列 + 向左展开的面板）
pub fn view<'a>(
    right_sidebar: &'a RightSidebar,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let t = main_translations(language);

    // 图标列（垂直排列按钮）
    let col = Column::new()
        .spacing(2)
        .width(ROUTE_BAR_WIDTH)
        .height(Length::Fill)
        // 图片转 MIDI 按钮：始终可见，点击自动展开/收起面板并亮灯
        .push(sidebar_button(
            Icon::ImageToMidi,
            t.tool_image_to_midi,
            Message::RightSidebar(RightSidebarAction::ImageToMidiClicked),
            right_sidebar.is_panel_active(RightSidebarPanel::ImageToMidi),
            window,
        ))
        // 素材库按钮：点击切换到素材库面板并亮灯
        .push(sidebar_button(
            Icon::MaterialLibrary,
            t.material_library,
            Message::RightSidebar(RightSidebarAction::MaterialLibraryClicked),
            right_sidebar.is_panel_active(RightSidebarPanel::Materials),
            window,
        ));
    let col = col.push(sidebar_button(
        Icon::PianoWaterfall,
        t.piano_waterfall,
        Message::RightSidebar(RightSidebarAction::PianoWaterfallClicked),
        right_sidebar.is_panel_active(RightSidebarPanel::PianoWaterfall),
        window,
    ));
    let col = col.push(Space::new().height(Length::Fill));

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
        // 面板内容按路由分发：素材库面板 / 图片转 MIDI 面板
        let panel_content: Element<'a> = match right_sidebar.active_panel {
            RightSidebarPanel::Materials => {
                crate::right_sidebar::materials_view::panel(right_sidebar, language, window)
            }
            RightSidebarPanel::PianoWaterfall => {
                crate::right_sidebar::piano_waterfall::panel(right_sidebar, language, window)
            }
            RightSidebarPanel::ImageToMidi => i2m_panel(right_sidebar, window, language),
        };

        // 调整大小手柄（放在面板左侧边缘，紧贴主内容区——面板向右栏图标列方向
        // 展开，左侧边界才是用户肉眼可见的可拖拽边缘）
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

        // 调整手柄 + 面板内容（手柄在面板左侧）
        let panel_with_handle = Row::new().push(resize_handle).push(panel_content);
        let panel_container = container(panel_with_handle)
            .width(Length::Fixed(right_sidebar.panel_width))
            .height(Length::Fill);

        // 顺序：面板内容（向左展开）→ 图标列（固定在右侧）
        Row::new()
            .push(panel_container)
            .push(route_bar)
            .height(Length::Fill)
            .into()
    } else {
        // 面板不可见，只显示图标列
        route_bar.into()
    }
}
