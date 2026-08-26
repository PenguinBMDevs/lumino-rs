//! 设置页面 - 音频设置

use iced_core::{Alignment, Length};
use iced_widget::{column, pick_list, row, text, text_input};
use lumino_ui_core::{Element, Message, Theme};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::SettingsPanel;
use lumino_core::storage::config::{AudioEngineKind, SynthBackend};
use lumino_extras::i18n::settings_translations;

/// 本地化合成器后端包装
#[derive(Debug, Clone, Copy)]
struct LocalizedSynth {
    inner: SynthBackend,
    name: &'static str,
}

impl PartialEq for LocalizedSynth {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedSynth {}

impl std::fmt::Display for LocalizedSynth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedSynth {
    fn new(backend: SynthBackend, lang: lumino_extras::i18n::Language) -> Self {
        Self {
            inner: backend,
            name: lumino_extras::i18n::synth_backend_name(backend, lang),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LocalizedAudioEngine {
    inner: AudioEngineKind,
    name: &'static str,
}
impl PartialEq for LocalizedAudioEngine {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl Eq for LocalizedAudioEngine {}
impl std::fmt::Display for LocalizedAudioEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
impl LocalizedAudioEngine {
    fn new(kind: AudioEngineKind) -> Self {
        let name = match kind {
            AudioEngineKind::Realtime => "Realtime (xsynth)",
        };
        Self { inner: kind, name }
    }
}

/// 渲染音频设置页面
pub fn view<'a>(settings: &'a SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.display.language);
    let synth_options = vec![
        LocalizedSynth::new(SynthBackend::XSynth, settings.display.language),
        LocalizedSynth::new(SynthBackend::Kdmapi, settings.display.language),
        LocalizedSynth::new(SynthBackend::System, settings.display.language),
    ];
    let current_synth = LocalizedSynth::new(settings.synth.backend, settings.display.language);
    let audio_engine_options = vec![LocalizedAudioEngine::new(AudioEngineKind::Realtime)];
    let current_engine = LocalizedAudioEngine::new(settings.synth.audio_engine);

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
            pick_list(synth_options, Some(current_synth), |ls| {
                Message::Settings(crate::Event::SynthBackendChanged(ls.inner))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 音频引擎选择（Realtime vs Core，仅 XSynth 时有效）
        row![
            text("音频引擎")
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(audio_engine_options, Some(current_engine), |ae| {
                Message::Settings(crate::Event::AudioEngineChanged(ae.inner))
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
    if settings.synth.backend == SynthBackend::XSynth {
        col = col.push(render_xsynth_options(settings, t));
    } else if settings.synth.backend == SynthBackend::Kdmapi {
        col = col.push(
            text(t.kdmapi_hint)
                .size(TEXT_SIZE_CONTENT)
                .style(create_placeholder_text_style()),
        );
    } else if settings.synth.backend == SynthBackend::System {
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
    t: &lumino_extras::i18n::SettingsTranslations,
) -> iced_widget::Column<'a, Message, Theme, lumino_ui_core::Renderer> {
    let mut col = column![];

    // 音色库选择
    col = col.push(
        row![
            text(t.soundfont)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text_input(t.soundfont_placeholder, &settings.synth.soundfont_path)
                .width(Length::Fill)
                .on_input(|s| Message::Settings(crate::Event::SoundfontPathChanged(s))),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));
    col = col.push(
        iced_widget::button(t.browse).on_press(Message::Settings(crate::Event::BrowseSoundfont)),
    );
    col = col.push(iced_widget::space().height(20));

    // 缓冲区大小（Realtime 引擎使用毫秒粒度）
    col = col.push(
        row![
            text(format!(
                "{}: {:.1} ms",
                t.buffer_latency, settings.synth.xsynth_buffer_ms
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(160.0),
            iced_widget::slider(5.0..=100.0, settings.synth.xsynth_buffer_ms, |ms| {
                Message::Settings(crate::Event::XSynthBufferChanged(ms))
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
        iced_widget::Checkbox::new(settings.synth.xsynth_fade_out)
            .label(t.fade_out_label)
            .on_toggle(|f| Message::Settings(crate::Event::XSynthFadeOutChanged(f))),
    );
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 每键最大同音数：0=不限制，1..64 拖拽 + 1..128 自定义输入
    let slider_val = match settings.synth.xsynth_max_voices_per_key {
        None => 0.0,
        Some(v) => (v as f32).clamp(0.0, 64.0),
    };
    let display_val = match settings.synth.xsynth_max_voices_per_key {
        None => "不限制".to_string(),
        Some(v) => v.to_string(),
    };
    col = col.push(
        row![
            text(format!("{}: {}", t.max_voices, display_val))
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style())
                .width(180.0),
            iced_widget::slider(0.0..=64.0, slider_val, |v| {
                let opt = if v < 0.5 { None } else { Some(v as usize) };
                Message::Settings(crate::Event::XSynthMaxVoicesChanged(opt))
            })
            .step(1.0)
            .width(160.0),
            text_input("0=不限制 1-128", &display_val)
                .width(80.0)
                .on_input(|s| { Message::Settings(crate::Event::XSynthMaxVoicesCustomInput(s)) }),
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
                t.velocity_filter, settings.midi.velocity_filter_threshold
            ))
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style())
            .width(180.0),
            iced_widget::slider(0..=127, settings.midi.velocity_filter_threshold, |v| {
                Message::Settings(crate::Event::VelocityFilterThresholdChanged(v.to_string()))
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
