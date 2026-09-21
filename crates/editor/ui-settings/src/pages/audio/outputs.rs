//! 设置页面 - 音频设置输出设备选择器渲染
//!
//! 由 `pages/audio.rs` 原样拆分，函数体逻辑零变更。

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};
use lumino_ui_core::{Element, Message};

use super::super::super::components::constants::{
    SPACING_CONTENT, SPACING_ICON_LABEL, SPACING_MAIN, TEXT_SIZE_CONTENT,
};
use super::super::super::components::styles::{
    create_content_text_style, create_placeholder_text_style,
};
use super::super::super::components::with_setting_tooltip_inline;
use crate::SettingsPanel;

/// 渲染 WinMM (系统 MIDI) 输出设备（播表）选择器
///
/// 展示系统播表自动扫描结果，通过下拉菜单选择指定的 WINMM 播表；
/// 提供「刷新」按钮触发重新扫描。
pub(super) fn render_winmm_output_selector<'a>(
    settings: &'a SettingsPanel,
    t: &lumino_extras::i18n::SettingsTranslations,
) -> Element<'a> {
    // 整组说明（System 模式无需音色库）挂在组内首个标签文字上；控件本身不触发提示
    let label = with_setting_tooltip_inline(
        text(t.winmm_output_device)
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style()),
        t.system_hint,
    );

    let refresh_btn =
        iced_widget::button(t.refresh).on_press(Message::Settings(crate::Event::ScanWinmmOutputs));

    let body: Element<'a> = if settings.midi.winmm_outputs.is_empty() {
        row![
            label,
            iced_widget::space().width(SPACING_MAIN),
            text(t.winmm_no_device)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
            iced_widget::space().width(SPACING_ICON_LABEL),
            refresh_btn,
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center)
        .into()
    } else {
        let options: Vec<&str> = settings
            .midi
            .winmm_outputs
            .iter()
            .map(|(_, name)| name.as_str())
            .collect();
        let selected = settings.midi.selected_winmm_output.and_then(|id| {
            settings
                .midi
                .winmm_outputs
                .iter()
                .find(|(oid, _)| *oid == id)
                .map(|(_, name)| name.as_str())
        });

        row![
            label,
            iced_widget::space().width(SPACING_MAIN),
            pick_list(options, selected, move |name| {
                if let Some((id, _)) = settings
                    .midi
                    .winmm_outputs
                    .iter()
                    .find(|(_, n)| n.as_str() == name)
                {
                    Message::Settings(crate::Event::WinmmOutputSelected(*id))
                } else {
                    Message::Null
                }
            })
            .placeholder(t.select_device_placeholder)
            .padding([4, 8])
            .width(200.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            refresh_btn,
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center)
        .into()
    };

    column![body].spacing(SPACING_CONTENT).into()
}

/// 渲染音频播放输出设备（CPAL 音频设备）选择器
///
/// 展示 CPAL 音频输出设备扫描结果，通过下拉菜单选择指定的播放输出设备；
/// 默认项为「系统默认输出设备」，提供「刷新」按钮触发重新扫描。
/// 该设置仅对软件合成器（XSynth / LGS）生效。
pub(super) fn render_audio_output_selector<'a>(
    settings: &'a SettingsPanel,
    t: &lumino_extras::i18n::SettingsTranslations,
) -> Element<'a> {
    let default_label = t.audio_output_default;
    let refresh_btn =
        iced_widget::button(t.refresh).on_press(Message::Settings(crate::Event::ScanAudioOutputs));

    // 整组说明挂在组内首个标签文字上；控件本身不触发提示
    let device_label = with_setting_tooltip_inline(
        text(t.audio_output_device)
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style()),
        t.audio_output_hint,
    );

    let body: Element<'a> = if settings.synth.audio_output_devices.is_empty() {
        row![
            device_label,
            iced_widget::space().width(SPACING_MAIN),
            text(t.audio_output_no_device)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
            iced_widget::space().width(SPACING_ICON_LABEL),
            refresh_btn,
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center)
        .into()
    } else {
        // 选项：默认项 + 所有扫描到的音频输出设备名
        let mut options: Vec<String> = vec![default_label.to_string()];
        options.extend(settings.synth.audio_output_devices.iter().cloned());
        // 选中态：未选择（None）时显示默认项
        let selected: Option<String> = settings
            .synth
            .selected_audio_output_device
            .clone()
            .or_else(|| Some(default_label.to_string()));

        row![
            device_label,
            iced_widget::space().width(SPACING_MAIN),
            pick_list(options, selected, move |name| {
                // 选择默认项 → 清空（使用系统默认）；否则记录设备名
                if name == default_label {
                    Message::Settings(crate::Event::AudioOutputSelected(String::new()))
                } else {
                    Message::Settings(crate::Event::AudioOutputSelected(name))
                }
            })
            .placeholder(t.select_device_placeholder)
            .padding([4, 8])
            .width(200.0),
            iced_widget::space().width(SPACING_ICON_LABEL),
            refresh_btn,
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center)
        .into()
    };

    column![body].spacing(SPACING_CONTENT).into()
}

/// 渲染 MIDI 输入设备选择器
pub(super) fn render_midi_device_selector<'a>(
    settings: &'a SettingsPanel,
    t: &lumino_extras::i18n::SettingsTranslations,
) -> Element<'a> {
    let device_count = settings.midi.devices.len();
    if device_count == 0 {
        return row![
            text(t.midi_input_device)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text(t.no_device)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center)
        .into();
    }

    let device_options: Vec<&str> = settings
        .midi
        .devices
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    let selected_idx = settings
        .midi
        .selected_device
        .and_then(|id| settings.midi.devices.iter().position(|(did, _)| *did == id));
    let selected = selected_idx.map(|i| device_options[i]);

    row![
        text(t.midi_input_device)
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style()),
        iced_widget::space().width(SPACING_MAIN),
        pick_list(device_options, selected, move |name| {
            if let Some((id, _)) = settings
                .midi
                .devices
                .iter()
                .find(|(_, n)| n.as_str() == name)
            {
                Message::Settings(crate::Event::DeviceSelected(*id))
            } else {
                Message::Null
            }
        })
        .placeholder(t.select_device_placeholder)
        .padding([4, 8])
        .width(200.0),
    ]
    .spacing(SPACING_ICON_LABEL)
    .align_y(Alignment::Center)
    .into()
}
