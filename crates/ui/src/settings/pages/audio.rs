//! 设置页面 - 音频设置

use crate::{Element, Message, Theme};
use iced_core::{Alignment, Length};
use iced_widget::{column, pick_list, row, text, text_input};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::settings::SettingsPanel;
use lumino_core::i18n::settings_translations;
use lumino_core::storage::config::SynthBackend;

/// 渲染音频设置页面
pub fn view<'a>(settings: &'a SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.language);
    let synth_options = [
        SynthBackend::XSynth,
        SynthBackend::Kdmapi,
        SynthBackend::System,
    ];

    let mut col = column![
        text(t.audio_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 合成器后端选择
        row![
            text(t.synthesizer)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(synth_options, Some(settings.synth_backend), |backend| {
                Message::Settings(crate::settings::Event::SynthBackendChanged(backend))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // MIDI 输入设备选择
        render_midi_device_selector(settings, t),
        iced_widget::space().height(SPACING_CONTENT),
    ];

    // 只在 XSynth 模式下显示音色库选择
    if settings.synth_backend == SynthBackend::XSynth {
        col = col.push(render_xsynth_options(settings, t));
    } else if settings.synth_backend == SynthBackend::Kdmapi {
        col = col.push(
            text(t.kdmapi_hint)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        );
    } else if settings.synth_backend == SynthBackend::System {
        col = col.push(
            text(t.system_hint)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        );
    }

    col.spacing(SPACING_CONTENT).padding(PADDING_CONTENT).into()
}

/// 渲染 XSynth 选项
fn render_xsynth_options<'a>(
    settings: &SettingsPanel,
    t: &lumino_core::i18n::SettingsTranslations,
) -> iced_widget::Column<'a, Message, Theme, crate::Renderer> {
    let mut col = column![];

    // 音色库选择
    col = col.push(
        row![
            text(t.soundfont)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(t.soundfont_placeholder, &settings.soundfont_path)
                .width(Length::Fill)
                .on_input(|s| Message::Settings(crate::settings::Event::SoundfontPathChanged(s))),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        iced_widget::button(t.browse)
            .on_press(Message::Settings(crate::settings::Event::BrowseSoundfont)),
    );
    col = col.push(iced_widget::space().height(20));

    // 缓冲区大小
    col = col.push(
        row![
            text(format!(
                "{}: {:.1} ms",
                t.buffer_latency, settings.xsynth_buffer_ms
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(160.0),
            iced_widget::slider(5.0..=100.0, settings.xsynth_buffer_ms, |ms| {
                Message::Settings(crate::settings::Event::XSynthBufferChanged(ms))
            })
            .step(1.0)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 音符释放淡出
    col = col.push(
        iced_widget::Checkbox::new(settings.xsynth_fade_out)
            .label(t.fade_out_label)
            .on_toggle(|f| Message::Settings(crate::settings::Event::XSynthFadeOutChanged(f))),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 每键最大同音数
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct VoiceOption(Option<usize>, &'static str);
    impl std::fmt::Display for VoiceOption {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.1)
        }
    }
    let voice_options = [
        VoiceOption(Some(1), "1 (极保守)"),
        VoiceOption(Some(2), "2"),
        VoiceOption(Some(4), "4 (默认)"),
        VoiceOption(Some(8), "8"),
        VoiceOption(Some(16), "16 (推荐)"),
        VoiceOption(Some(32), "32"),
        VoiceOption(Some(64), "64 (密集)"),
        VoiceOption(None, "不限制"),
    ];
    let current_voice = voice_options
        .iter()
        .find(|o| o.0 == settings.xsynth_max_voices_per_key)
        .copied()
        .or(Some(voice_options[3]));

    col = col.push(
        row![
            text(t.max_voices)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(voice_options, current_voice, |opt| {
                Message::Settings(crate::settings::Event::XSynthMaxVoicesChanged(opt.0))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        text(t.max_voices_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(iced_widget::space().height(20));

    // 力度过滤
    col = col.push(
        row![
            text(format!(
                "{}: {}",
                t.velocity_filter, settings.velocity_filter_threshold
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(180.0),
            iced_widget::slider(0..=127, settings.velocity_filter_threshold, |v| {
                Message::Settings(crate::settings::Event::VelocityFilterThresholdChanged(
                    v.to_string(),
                ))
            })
            .step(1)
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        text(t.velocity_filter_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(iced_widget::space().height(20));

    // 帮助文本
    col = col.push(
        text(t.xsynth_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(
        text(t.kdmapi_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );
    col = col.push(
        text(t.system_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
    );

    col
}

/// 渲染 MIDI 输入设备选择器
fn render_midi_device_selector<'a>(
    settings: &'a SettingsPanel,
    t: &lumino_core::i18n::SettingsTranslations,
) -> Element<'a> {
    let device_count = settings.midi_devices.len();
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
        .midi_devices
        .iter()
        .map(|(_, name)| name.as_str())
        .collect();
    let selected_idx = settings
        .selected_midi_device
        .and_then(|id| settings.midi_devices.iter().position(|(did, _)| *did == id));
    let selected = selected_idx.map(|i| device_options[i]);

    row![
        text(t.midi_input_device)
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style()),
        iced_widget::space().width(SPACING_MAIN),
        pick_list(device_options, selected, move |name| {
            if let Some((id, _)) = settings
                .midi_devices
                .iter()
                .find(|(_, n)| n.as_str() == name)
            {
                Message::Settings(crate::settings::Event::DeviceSelected(*id))
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
