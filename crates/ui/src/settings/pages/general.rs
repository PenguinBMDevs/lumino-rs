//! 设置页面 - 常规设置

use crate::{Element, Message};
use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::settings::SettingsPanel;
use lumino_core::storage::config::EraserBehavior;

/// 渲染常规设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    // 橡皮擦行为选项
    let eraser_options = vec![EraserBehavior::Default, EraserBehavior::DirectSelect];
    let current_eraser = settings.eraser_behavior;

    column![
        text("常规")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 橡皮擦行为选择
        row![
            text("橡皮擦行为:")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(eraser_options, Some(current_eraser), |behavior| {
                Message::Settings(crate::settings::Event::EraserBehaviorChanged(behavior))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        text("默认: Shift+拖动框选删除，点击删除单个\n直接框选: 拖动框选删除，Shift+点击删除单个")
            .size(12.0)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
