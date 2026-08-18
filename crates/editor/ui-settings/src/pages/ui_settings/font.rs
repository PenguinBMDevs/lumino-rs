//! 界面设置 - 字体设置部分

use iced_core::Alignment;
use iced_widget::{button, column, pick_list, row, text, text_input};
use lumino_ui_core::{Element, Message};

use super::super::super::components::constants::*;
use super::super::super::components::styles::{
    create_content_text_style, create_placeholder_text_style,
};
use crate::SettingsPanel;
use lumino_extras::i18n::SettingsTranslations;

/// 构建字体设置部分（系统字体下拉、自定义路径、浏览按钮、说明）
pub(crate) fn build_font_section<'a>(
    settings: &'a SettingsPanel,
    t: &'static SettingsTranslations,
    system_fonts: &[lumino_note_core::font_scanner::FontInfo],
) -> Element<'a> {
    // 字体选项 - 从系统扫描的字体列表构建
    let font_options: Vec<String> = system_fonts.iter().map(|f| f.name.clone()).collect();

    // 字体选择下拉菜单
    let font_dropdown = pick_list(
        font_options,
        Some(settings.editing.program_font_name.clone()).filter(|s| !s.is_empty()),
        |font_name| Message::Settings(crate::Event::ProgramFontNameChanged(font_name)),
    )
    .width(200.0)
    .placeholder("选择系统字体...");

    // 自定义字体路径输入
    let font_path_input = text_input(t.font_path_placeholder, &settings.editing.program_font_path)
        .on_input(|path| Message::Settings(crate::Event::ProgramFontPathChanged(path)));

    // 浏览字体文件按钮
    let browse_font_button =
        button(t.browse).on_press(Message::Settings(crate::Event::BrowseProgramFont));

    column![
        // 系统字体下拉菜单
        row![
            text(t.program_font)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            font_dropdown,
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.or).size(12.0).style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_ICON_LABEL),
        // 自定义路径输入
        row![
            font_path_input.width(250.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            browse_font_button,
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 字体设置说明
        text(t.font_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .into()
}
