//! 音频导出对话框 - 事件过滤区域
//!
//! 包括：忽略音色变化复选框、音符力度过滤与范围、音符键位过滤与范围、结束延迟。

use iced_widget::{checkbox, column, row, space, text, text_input};

use crate::message::{AudioExportAction, Message};
use crate::state::root_state::AudioExportDialogState;

use super::title::section_title;
use crate::view::widgets;

/// 事件过滤区域
pub fn event_filter_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    // 将临时 String 绑定到局部变量，避免 borrow 生命周期问题
    let velocity_low = state.velocity_low.to_string();
    let velocity_high = state.velocity_high.to_string();
    let key_low = state.key_low.to_string();
    let key_high = state.key_high.to_string();
    let note_force_end_delay = state.note_force_end_delay.to_string();

    column![
        section_title("事件过滤", palette),
        space().height(8),
        ignore_program_changes_checkbox(state, palette),
        space().height(8),
        velocity_filter_section(state, palette, &velocity_low, &velocity_high),
        space().height(8),
        key_filter_section(state, palette, &key_low, &key_high),
        space().height(8),
        note_delay_row(palette, &note_force_end_delay),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 忽略音色变化复选框
fn ignore_program_changes_checkbox<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    checkbox(state.ignore_program_changes)
        .label("忽略音色变化事件")
        .on_toggle(|v| Message::AudioExport(AudioExportAction::IgnoreProgramChangesChanged(v)))
        .style(widgets::dialog_checkbox_style(palette))
        .into()
}

/// 音符力度过滤区域（启用复选框 + 范围输入）
fn velocity_filter_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
    low: &'a str,
    high: &'a str,
) -> crate::Element<'a> {
    column![
        text("音符力度过滤")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        checkbox(state.filter_velocity)
            .label("启用力度过滤")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::FilterVelocityChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        range_input_row("力度范围:", palette, low, high, |v| {
            Message::AudioExport(AudioExportAction::VelocityLowChanged(v))
        }, |v| {
            Message::AudioExport(AudioExportAction::VelocityHighChanged(v))
        }),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 音符键位过滤区域（启用复选框 + 范围输入）
fn key_filter_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
    low: &'a str,
    high: &'a str,
) -> crate::Element<'a> {
    column![
        text("音符键位过滤")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        checkbox(state.filter_key)
            .label("启用键位过滤")
            .on_toggle(|v| Message::AudioExport(AudioExportAction::FilterKeyChanged(v)))
            .style(widgets::dialog_checkbox_style(palette)),
        space().height(4),
        range_input_row("键位范围:", palette, low, high, |v| {
            Message::AudioExport(AudioExportAction::KeyLowChanged(v))
        }, |v| {
            Message::AudioExport(AudioExportAction::KeyHighChanged(v))
        }),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 音符结束延迟输入行
fn note_delay_row<'a>(
    palette: &'a iced_core::theme::palette::Extended,
    delay: &'a str,
) -> crate::Element<'a> {
    row![
        text("音符结束延迟 (ms):")
            .size(14)
            .style(widgets::dialog_label_style(palette))
            .width(120),
        text_input("0", delay)
            .on_input(|v| Message::AudioExport(AudioExportAction::NoteForceEndDelayChanged(v)))
            .padding([6, 10])
            .width(200),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

// ---------------------------------------------------------------------------
// 内部辅助：范围输入行（两端 text_input）
// ---------------------------------------------------------------------------

/// 通用的低-高范围输入行（用于力度范围、键位范围等）
fn range_input_row<'a>(
    label_str: &'a str,
    palette: &'a iced_core::theme::palette::Extended,
    low: &'a str,
    high: &'a str,
    on_low_input: impl Fn(String) -> Message + 'a,
    on_high_input: impl Fn(String) -> Message + 'a,
) -> crate::Element<'a> {
    row![
        text(label_str)
            .size(14)
            .style(widgets::dialog_label_style(palette))
            .width(120),
        text_input("0", low)
            .on_input(on_low_input)
            .padding([6, 10])
            .width(80),
        text(" ~ ")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        text_input("127", high)
            .on_input(on_high_input)
            .padding([6, 10])
            .width(80),
    ]
    .spacing(4)
    .align_y(iced_core::Alignment::Center)
    .into()
}
