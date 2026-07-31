//! 音频导出对话框 - 音频设置区域
//!
//! 包括：输出格式、比特率、采样率、通道数、层数限制、通道多线程、
//! 按键多线程、插值算法、以及三个复选框。

use iced_widget::{checkbox, column, pick_list, row, space, text, text_input};

use crate::message::{AudioExportAction, Message};
use crate::state::root_state::{
    AudioChannels, AudioExportDialogState, AudioFormat, Interpolation, ThreadingOption,
};

use super::title::section_title;
use crate::view::widgets;

/// 音频设置区域
pub fn audio_settings_section<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        section_title("音频设置", palette),
        space().height(12),
        format_row(state, palette),
        space().height(8),
        bitrate_row(state, palette),
        space().height(8),
        sample_rate_row(state, palette),
        space().height(8),
        channels_row(state, palette),
        space().height(8),
        layers_row(state, palette),
        space().height(8),
        channel_threading_row(state, palette),
        space().height(8),
        key_threading_row(state, palette),
        space().height(8),
        interpolation_row(state, palette),
        space().height(12),
        apply_limiter_checkbox(state, palette),
        space().height(4),
        disable_fade_out_checkbox(state, palette),
        space().height(4),
        linear_envelope_checkbox(state, palette),
    ]
    .width(iced_core::Length::Fill)
    .into()
}

/// 输出格式选择行
fn format_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("输出格式:", palette).width(120),
        pick_list(
            [
                AudioFormat::WAV,
                AudioFormat::FLAC,
                AudioFormat::MP3,
                AudioFormat::Ogg,
                AudioFormat::WavPack,
            ],
            Some(state.format),
            |v| Message::AudioExport(AudioExportAction::FormatChanged(v)),
        ),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 比特率输入行
fn bitrate_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("比特率 (kbps):", palette).width(120),
        text_input("320", &state.audio_bitrate.to_string())
            .on_input(|v| Message::AudioExport(AudioExportAction::BitrateChanged(v)))
            .padding([6, 10])
            .width(200),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 采样率选择行
fn sample_rate_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("采样率:", palette).width(120),
        pick_list(
            [22050u32, 44100, 48000, 96000],
            Some(state.sample_rate),
            |v| Message::AudioExport(AudioExportAction::SampleRateChanged(v)),
        ),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 通道数选择行
fn channels_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("通道数:", palette).width(120),
        pick_list(
            [AudioChannels::Mono, AudioChannels::Stereo],
            Some(state.channels),
            |v| Message::AudioExport(AudioExportAction::ChannelsChanged(v)),
        ),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 层数限制输入行
fn layers_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("层数限制:", palette).width(120),
        text_input("32", &state.layers.to_string())
            .on_input(|v| Message::AudioExport(AudioExportAction::LayersChanged(v)))
            .padding([6, 10])
            .width(200),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 通道多线程选择行
fn channel_threading_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("通道多线程:", palette).width(120),
        pick_list(
            [
                ThreadingOption::None,
                ThreadingOption::Auto,
                ThreadingOption::Manual(2),
                ThreadingOption::Manual(4),
                ThreadingOption::Manual(8),
            ],
            Some(state.channel_threading),
            |v| Message::AudioExport(AudioExportAction::ChannelThreadingChanged(v)),
        ),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 按键多线程选择行
fn key_threading_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("按键多线程:", palette).width(120),
        pick_list(
            [
                ThreadingOption::None,
                ThreadingOption::Auto,
                ThreadingOption::Manual(2),
                ThreadingOption::Manual(4),
                ThreadingOption::Manual(8),
            ],
            Some(state.key_threading),
            |v| Message::AudioExport(AudioExportAction::KeyThreadingChanged(v)),
        ),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 插值算法选择行
fn interpolation_row<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    row![
        label("插值算法:", palette).width(120),
        pick_list(
            [Interpolation::None, Interpolation::Linear],
            Some(state.interpolation),
            |v| Message::AudioExport(AudioExportAction::InterpolationChanged(v)),
        ),
    ]
    .spacing(8)
    .align_y(iced_core::Alignment::Center)
    .into()
}

/// 应用限制器复选框
fn apply_limiter_checkbox<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    checkbox(state.apply_limiter)
        .label("应用限制器 (防止削波)")
        .on_toggle(|v| Message::AudioExport(AudioExportAction::ApplyLimiterChanged(v)))
        .style(widgets::dialog_checkbox_style(palette))
        .into()
}

/// 禁用淡出复选框
fn disable_fade_out_checkbox<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    checkbox(state.disable_fade_out)
        .label("禁用淡出 (可能爆音)")
        .on_toggle(|v| Message::AudioExport(AudioExportAction::DisableFadeOutChanged(v)))
        .style(widgets::dialog_checkbox_style(palette))
        .into()
}

/// 线性包络复选框
fn linear_envelope_checkbox<'a>(
    state: &'a AudioExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    checkbox(state.linear_envelope)
        .label("线性包络")
        .on_toggle(|v| Message::AudioExport(AudioExportAction::LinearEnvelopeChanged(v)))
        .style(widgets::dialog_checkbox_style(palette))
        .into()
}

// ---------------------------------------------------------------------------
// 内部辅助：统一的标签文字，避免每次重复 widget + style
// ---------------------------------------------------------------------------

/// 设置行左侧标签文字
fn label<'a>(
    text_str: &'a str,
    palette: &'a iced_core::theme::palette::Extended,
) -> iced_widget::Text<'a> {
    text(text_str)
        .size(14)
        .style(widgets::dialog_label_style(palette))
}
