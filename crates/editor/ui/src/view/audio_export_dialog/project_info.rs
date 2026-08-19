//! 音频导出对话框 - 工程信息区域
//!
//! 包括：工程名称、MIDI 路径、音色库路径三条输入行。

use iced_widget::{button, column, container, row, space, text, text_input};

use crate::Message;
use crate::message::AudioExportAction;
use crate::state::root_state::AudioExportDialogState;

use crate::view::widgets;

/// 工程信息区域（工程名、MIDI路径、音色库路径）
pub fn project_info_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        project_name_row(state, palette),
        space().height(12),
        midi_path_row(state, palette),
        space().height(12),
        soundfont_path_row(state, palette),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 工程名称输入行
fn project_name_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("工程名称")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        container(
            text_input("工程名称", &state.project_name)
                .on_input(|v| Message::AudioExport(AudioExportAction::ProjectNameChanged(v)))
                .padding([6, 10])
                .width(iced_core::Length::Fill),
        )
        .width(iced_core::Length::Fill)
        .style(widgets::dialog_input_style(palette)),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// MIDI 路径选择行
fn midi_path_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("MIDI 路径")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        row![
            container(
                text(&state.midi_path)
                    .size(12)
                    .style(widgets::dialog_muted_text_style(palette))
                    .width(iced_core::Length::Fill),
            )
            .width(iced_core::Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseMidi))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 音色库路径选择行
fn soundfont_path_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("音色库 (SF2)")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        row![
            container(
                text(&state.soundfont_path)
                    .size(12)
                    .style(widgets::dialog_muted_text_style(palette))
                    .width(iced_core::Length::Fill),
            )
            .width(iced_core::Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::AudioExport(AudioExportAction::BrowseSoundfont))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
    ]
    .width(iced_core::Length::Fill)
    .into()
}
