//! 音频导出对话框 - 大标题与段落小标题

use iced_widget::text;

use crate::message::{AudioExportAction, Message};

use crate::view::widgets;

/// 对话框大标题
pub fn title_section<'a>(palette: &'a iced_core::theme::palette::Extended) -> crate::Element<'a> {
    text("音频导出")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(widgets::dialog_label_style(palette))
        .into()
}

/// 段落小标题
pub fn section_title<'a>(
    text_str: &'a str,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    text(text_str)
        .size(16)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(widgets::dialog_label_style(palette))
        .into()
}
