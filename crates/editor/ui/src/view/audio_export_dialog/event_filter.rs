//! 音频导出对话框 - 事件过滤区域
//!
//! 包括：忽略音色变化复选框、音符力度过滤与范围、音符键位过滤与范围、结束延迟。

use iced_widget::{checkbox, column, row, space, text, text_input};

use crate::Message;
use crate::message::AudioExportAction;
use crate::state::root_state::AudioExportDialogState;

use super::title::section_title;
use crate::view::widgets;

/// 事件过滤区域
pub fn event_filter_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        section_title("事件过滤", palette),
        space().height(8),
        ignore_program_changes_checkbox(state, palette),
        space().height(8),
        velocity_filter_section(state, palette),
        space().height(8),
        key_filter_section(state, palette),
        space().height(8),
        note_delay_row(state, palette),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 忽略音色变化复选框
fn ignore_program_changes_checkbox<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        checkbox(state.ignore_program_changes)
            .label("忽略音色变化事件（Program Change）")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::IgnoreProgramChangesChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        text("勾选后所有音色切换事件被丢弃，所有音符用默认音色渲染")
            .size(11)
            .style(widgets::dialog_label_style(palette)),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 音符力度过滤区域（启用复选框 + 范围输入）
fn velocity_filter_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("音符力度过滤（仅保留指定力度范围内的音符）")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        checkbox(state.filter_velocity)
            .label("启用力度过滤（超出范围的音符将被静音丢弃）")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::FilterVelocityChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        range_input_row(
            "力度范围 (0-127):",
            palette,
            state.velocity_low,
            state.velocity_high,
            |v| { Message::AudioExport(AudioExportAction::VelocityLowChanged(v)) },
            |v| { Message::AudioExport(AudioExportAction::VelocityHighChanged(v)) }
        ),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 音符键位过滤区域（启用复选框 + 范围输入）
fn key_filter_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("音符键位过滤（仅保留指定键位范围内的音符，0=C-1, 60=C4, 127=G9）")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        checkbox(state.filter_key)
            .label("启用键位过滤（超出范围的音符将被静音丢弃）")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::FilterKeyChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        range_input_row(
            "键位范围 (0-127):",
            palette,
            state.key_low,
            state.key_high,
            |v| { Message::AudioExport(AudioExportAction::KeyLowChanged(v)) },
            |v| { Message::AudioExport(AudioExportAction::KeyHighChanged(v)) }
        ),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 音符结束延迟输入行
fn note_delay_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        row![
            text("音符结束延迟 (ms):")
                .size(14)
                .style(widgets::dialog_label_style(palette))
                .width(140),
            text_input("0", &state.note_force_end_delay.to_string())
                .on_input(|v| Message::AudioExport(AudioExportAction::NoteForceEndDelayChanged(v)))
                .padding([6, 10])
                .width(180),
        ]
        .spacing(8)
        .align_y(iced_core::Alignment::Center),
        text("每个音符 NoteOff 后额外延长该时长再释放，>0 可避免极短音被吞（0=禁用）")
            .size(11)
            .style(widgets::dialog_label_style(palette)),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// 内部辅助：范围输入行（两端 text_input）
// ---------------------------------------------------------------------------

/// 通用的低-高范围输入行（用于力度范围、键位范围等）
fn range_input_row<'a>(
    label_str: &'a str,
    palette: &'a iced_core::theme::palette::Extended,
    low: u8,
    high: u8,
    on_low_input: impl Fn(String) -> Message + 'a,
    on_high_input: impl Fn(String) -> Message + 'a,
) -> crate::Element<'a> {
    let low_s = low.to_string();
    let high_s = high.to_string();
    row![
        text(label_str)
            .size(14)
            .style(widgets::dialog_label_style(palette))
            .width(120),
        text_input("0", &low_s)
            .on_input(on_low_input)
            .padding([6, 10])
            .width(80),
        text(" ~ ")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        text_input("127", &high_s)
            .on_input(on_high_input)
            .padding([6, 10])
            .width(80),
    ]
    .spacing(4)
    .align_y(iced_core::Alignment::Center)
    .into()
}
