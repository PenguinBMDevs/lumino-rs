//! 设置页面 - 快捷键设置

use crate::Element;
use crate::settings::SettingsPanel;
use iced_widget::{column, text};
use lumino_core::i18n::settings_translations;

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};

/// 渲染快捷键设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.language);

    column![
        text(t.shortcuts_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text(t.shortcuts_placeholder)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
