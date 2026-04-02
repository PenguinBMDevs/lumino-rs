//! 设置页面 - 界面设置

use crate::{Element, Message, Theme};
use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::settings::SettingsPanel;
use crate::window;

/// 渲染界面设置页面
pub fn view<'a>(settings: &SettingsPanel, window: &crate::window::Window) -> Element<'a> {
    // 创建复选框
    let native_titlebar_checkbox = iced_widget::Checkbox::new(settings.use_native_titlebar)
        .label("使用经典系统标题栏")
        .on_toggle(|enabled| {
            Message::Settings(crate::settings::Event::NativeTitlebarChanged(enabled))
        });

    // 主题选项
    let theme_options: Vec<String> = Theme::ALL.iter().map(|t| t.to_string()).collect();
    let current_theme = window.theme.to_string();

    column![
        text("界面")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 主题选择
        row![
            text("主题:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(theme_options, Some(current_theme), |theme| {
                Message::Window(window::Event::Theme(theme))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 使用经典系统标题栏选项
        row![native_titlebar_checkbox,]
            .spacing(SPACING_ICON_LABEL)
            .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("启用后，将使用系统原生标题栏，隐藏 Logo 和自定义窗口控制按钮")
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
