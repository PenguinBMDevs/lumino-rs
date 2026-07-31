//! 音频导出对话框 - 输出路径区域

use iced_widget::{button, column, container, row, space, text, text_input};

use crate::message::{AudioExportAction, Message};
use crate::state::root_state::AudioExportDialogState;

use super::title::section_title;
use crate::view::widgets;

/// 输出路径区域
pub fn output_path_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        section_title("输出路径", palette),
        space().height(8),
        output_path_row(state, palette),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 输出路径输入行（文本输入 + 浏览按钮）
fn output_path_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        container(
            text_input("选择输出路径...", &state.output_path)
                .on_input(|v| Message::AudioExport(AudioExportAction::OutputPathChanged(v)))
                .padding([6, 10])
                .width(iced_core::Length::Fill),
        )
        .width(iced_core::Length::Fill)
        .style(widgets::dialog_input_style(palette)),
        space().width(8),
        button(text("浏览...").size(14))
            .on_press(Message::AudioExport(AudioExportAction::BrowseOutput))
            .padding([6, 16]),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}
