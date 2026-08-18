//! 界面设置 - 自动滚动配置部分

use iced_core::Alignment;
use iced_widget::{column, row, text, text_input};
use lumino_ui_core::{Element, Message};

use super::super::super::components::constants::*;
use super::super::super::components::styles::{
    create_content_text_style, create_placeholder_text_style,
};
use crate::SettingsPanel;
use lumino_extras::i18n::SettingsTranslations;

/// 构建自动滚动配置部分
pub(crate) fn build_auto_scroll_section<'a>(
    settings: &'a SettingsPanel,
    t: &'static SettingsTranslations,
) -> Element<'a> {
    column![
        text(t.auto_scroll)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(12),
        // 模式1：指示线固定位置
        row![
            text(t.auto_scroll_fixed)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(t.pixel, &settings.auto_scroll.fixed_position.to_string())
                .on_input(|v| Message::Settings(crate::Event::AutoScrollFixedPositionChanged(v)))
                .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.from_left)
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 模式2：翻页触发位置
        row![
            text(t.auto_scroll_trigger)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(
                t.pixel,
                &settings.auto_scroll.page_trigger_offset.to_string()
            )
            .on_input(|v| Message::Settings(crate::Event::AutoScrollPageTriggerOffsetChanged(v)))
            .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.from_right)
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 模式2：翻页后回到的位置
        row![
            text(t.auto_scroll_return)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(
                t.pixel,
                &settings.auto_scroll.page_return_position.to_string()
            )
            .on_input(|v| Message::Settings(crate::Event::AutoScrollPageReturnPositionChanged(v)))
            .width(80.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            text(t.from_left)
                .size(12.0)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text(t.auto_scroll_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
    ]
    .spacing(SPACING_CONTENT)
    .into()
}
