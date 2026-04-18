//! 设置面板主视图

use iced_core::{Border, Length};
use iced_widget::{column, container, row, text};

use super::{SettingsPanel, components::*, menu, pages::*};
use crate::{Element, Message, Theme, window};
use lumino_core::font_scanner::FontInfo;

/// 渲染设置面板主视图
pub fn view<'a>(
    settings: &SettingsPanel,
    window: &window::Window,
    system_fonts: &[FontInfo],
) -> Element<'a> {
    let menu_items = menu::create_menu_items();

    let menu_list = menu::render_menu_list(settings, window, &menu_items);
    let content_area = render_content_area(settings, window, system_fonts);

    let main_content = row![
        menu_list,
        iced_widget::space().width(SPACING_MAIN),
        content_area,
    ]
    .spacing(SPACING_MENU_CONTENT)
    .padding(PADDING_CONTENT);

    container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(create_main_container_style())
        .into()
}

fn render_content_area<'a>(
    settings: &SettingsPanel,
    window: &window::Window,
    system_fonts: &[FontInfo],
) -> iced_widget::Container<'a, Message, Theme, crate::Renderer> {
    let content = match settings.selected_menu_index {
        0 => general_view(settings),
        1 => audio_view(settings),
        2 => ui_settings_view(settings, window, system_fonts),
        3 => shortcuts_view(),
        4 => about_view(),
        _ => render_placeholder("设置内容区域").into(),
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(create_content_container_style())
}

fn create_content_container_style() -> impl Fn(&Theme) -> container::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(palette.background.base.color)),
            border: Border::default()
                .rounded(BORDER_RADIUS_CONTENT)
                .width(BORDER_WIDTH)
                .color(palette.background.strong.color),
            shadow: iced_core::Shadow {
                color: iced_core::Color::from_rgba(
                    SHADOW_COLOR_CONTENT[0],
                    SHADOW_COLOR_CONTENT[1],
                    SHADOW_COLOR_CONTENT[2],
                    SHADOW_COLOR_CONTENT[3],
                ),
                offset: iced_core::Vector::new(SHADOW_OFFSET_CONTENT.0, SHADOW_OFFSET_CONTENT.1),
                blur_radius: SHADOW_BLUR_CONTENT,
            },
            text_color: Some(palette.background.base.text),
            snap: false,
        }
    }
}

fn create_main_container_style() -> impl Fn(&Theme) -> container::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(
                palette.background.weakest.color,
            )),
            text_color: Some(palette.background.base.text),
            snap: false,
            ..Default::default()
        }
    }
}

fn render_placeholder<'a>(
    content: &'a str,
) -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    column![
        text("设置")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text(content)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
}
