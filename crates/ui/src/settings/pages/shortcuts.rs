//! 设置页面 - 快捷键设置

use crate::Element;
use iced_widget::{column, text};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};

/// 渲染快捷键设置页面
pub fn view<'a>() -> Element<'a> {
    column![
        text("快捷键")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text("快捷键设置内容")
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
