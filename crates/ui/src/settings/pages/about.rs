//! 设置页面 - 关于

use crate::Element;
use iced_widget::{column, text};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};

/// 渲染关于页面
pub fn view<'a>() -> Element<'a> {
    column![
        text("关于")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text("Lumino").size(16.0).style(create_content_text_style()),
        text("版本 1.0.0")
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(10),
        text("一个高效的MIDI编辑工具")
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
