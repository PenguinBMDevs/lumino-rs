//! 设置页面 - 关于

use crate::SettingsPanel;
use iced_widget::{column, text};
use lumino_core::i18n::settings_translations;
use lumino_ui_core::Element;

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};

/// 渲染关于页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.language);

    column![
        text(t.about_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text(t.app_name)
            .size(16.0)
            .style(create_content_text_style()),
        text(t.version)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(10),
        text(t.app_description)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
