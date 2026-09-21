//! 设置页面 - 音频设置

mod lgs;
mod outputs;
mod types;
mod xsynth;

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};
use lumino_ui_core::{Element, Message};

use self::lgs::render_lgs_options;
use self::outputs::{
    render_audio_output_selector, render_midi_device_selector, render_winmm_output_selector,
};
use self::types::{LocalizedBuiltinEngine, LocalizedOutputType};
use self::xsynth::render_xsynth_options;
use super::super::components::constants::*;
use super::super::components::styles::create_content_text_style;
use super::super::components::with_setting_tooltip_inline;
use crate::SettingsPanel;
use lumino_core::storage::config::SynthBackend;
use lumino_extras::i18n::settings_translations;
use lumino_ui_core::settings_event::OutputType;

/// 渲染音频设置页面
pub fn view<'a>(settings: &'a SettingsPanel) -> Element<'a> {
    let t = settings_translations(settings.display.language);

    // 顶层：MIDI 输出类型（内置合成器 / KDMAPI / 系统 MIDI）
    let current_output_type = if matches!(
        settings.synth.backend,
        SynthBackend::XSynth | SynthBackend::Lgs
    ) {
        LocalizedOutputType::new(OutputType::Builtin, settings.display.language)
    } else {
        LocalizedOutputType::new(
            match settings.synth.backend {
                SynthBackend::Kdmapi => OutputType::Kdmapi,
                SynthBackend::System => OutputType::System,
                _ => OutputType::Builtin,
            },
            settings.display.language,
        )
    };
    let output_type_options = vec![
        LocalizedOutputType::new(OutputType::Builtin, settings.display.language),
        LocalizedOutputType::new(OutputType::Kdmapi, settings.display.language),
        LocalizedOutputType::new(OutputType::System, settings.display.language),
    ];

    // 内置合成器引擎子下拉（与 xsynth-realtime 共用同一列表）
    let show_builtin_engine = matches!(
        settings.synth.backend,
        SynthBackend::XSynth | SynthBackend::Lgs
    );
    let builtin_engine_options = vec![
        LocalizedBuiltinEngine::new(SynthBackend::XSynth, settings.display.language),
        LocalizedBuiltinEngine::new(SynthBackend::Lgs, settings.display.language),
    ];
    let current_builtin_engine = LocalizedBuiltinEngine::new(
        if show_builtin_engine {
            settings.synth.backend
        } else {
            SynthBackend::XSynth
        },
        settings.display.language,
    );

    let mut col = column![
        text(t.audio_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 输出类型选择（KDMAPI 说明挂在「合成器」标签文字上，控件本身不触发提示）
        row![
            with_setting_tooltip_inline(
                text(t.synthesizer)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                t.kdmapi_hint,
            ),
            iced_widget::space().width(SPACING_MAIN),
            pick_list(output_type_options, Some(current_output_type), |ot| {
                Message::Settings(crate::Event::OutputTypeChanged(ot.inner))
            })
            .width(200.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
    ];

    // 内置合成器引擎子下拉（仅内置类型显示）
    if show_builtin_engine {
        col = col.push(
            row![
                text(t.builtin_engine)
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                iced_widget::space().width(SPACING_MAIN),
                pick_list(builtin_engine_options, Some(current_builtin_engine), |be| {
                    Message::Settings(crate::Event::SynthBackendChanged(be.inner))
                })
                .width(200.0),
            ]
            .spacing(SPACING_ICON_LABEL)
            .align_y(Alignment::Center),
        );
        col = col.push(iced_widget::space().height(SPACING_CONTENT));
    }

    // 音频播放输出设备（CPAL 音频设备）选择器：仅对软件合成器（内置引擎）生效
    if show_builtin_engine {
        col = col.push(render_audio_output_selector(settings, t));
        col = col.push(iced_widget::space().height(SPACING_CONTENT));
    }

    // MIDI 输入设备选择
    col = col.push(render_midi_device_selector(settings, t));
    col = col.push(iced_widget::space().height(SPACING_CONTENT));

    // 只在对应模式下显示音色库选择 / 提示；
    // KDMAPI 模式使用系统驱动、无需音色库，其说明已挂在上方「合成器」输出类型选择器上
    if settings.synth.backend == SynthBackend::XSynth {
        col = col.push(render_xsynth_options(settings, t));
    } else if settings.synth.backend == SynthBackend::Lgs {
        col = col.push(render_lgs_options(settings, t));
    } else if settings.synth.backend == SynthBackend::System {
        col = col.push(render_winmm_output_selector(settings, t));
    }

    col.spacing(SPACING_CONTENT).padding(PADDING_CONTENT).into()
}
