//! 设置「音频」页：模型构建 + 字段动作解析。
//!
//! 页面随合成器后端变化（XSynth / LGS / 系统 MIDI）；所有文案与数值文本在这里
//! 预计算，egui 只按行模型绘制。动作按 `AudioField` 回传，再映射到生产
//! `settings::Event`（与 iced 页面同一批事件）。

use lumino_core::storage::config::{GpuLoadDisplay, SynthBackend};
use lumino_core::tr;
use lumino_extras::i18n::{Language, settings_translations};
use lumino_ui_core::settings_event::OutputType;
use lumino_ui_egui::chrome::{AudioControlModel, AudioField, AudioPageModel, AudioRowModel};

mod actions;
mod pages;

pub(super) use actions::action_message;

use crate::root::Root;

/// 构造音频页模型。
pub(super) fn page(root: &Root) -> AudioPageModel {
    let settings = &root.settings;
    let language = settings.display.language;
    let t = settings_translations(language);
    let synth = &settings.synth;
    let midi = &settings.midi;
    let mut rows: Vec<AudioRowModel> = Vec::new();

    let builtin = matches!(synth.backend, SynthBackend::XSynth | SynthBackend::Lgs);
    let output_type = match synth.backend {
        SynthBackend::Lgs | SynthBackend::XSynth => 0,
        SynthBackend::Kdmapi => 1,
        SynthBackend::System => 2,
    };
    rows.push(row(
        AudioField::OutputType,
        t.synthesizer.to_owned(),
        t.kdmapi_hint.to_owned(),
        combo(
            vec![
                output_type_name(OutputType::Builtin, language),
                output_type_name(OutputType::Kdmapi, language),
                output_type_name(OutputType::System, language),
            ],
            output_type,
        ),
        200.0,
    ));

    if builtin {
        let engine = usize::from(synth.backend == SynthBackend::Lgs);
        rows.push(row(
            AudioField::Engine,
            t.builtin_engine.to_owned(),
            String::new(),
            combo(
                vec![
                    engine_name(SynthBackend::XSynth, language),
                    engine_name(SynthBackend::Lgs, language),
                ],
                engine,
            ),
            200.0,
        ));

        // 音频播放输出设备：默认项 + 扫描结果。
        if synth.audio_output_devices.is_empty() {
            rows.push(row(
                AudioField::AudioOutput,
                t.audio_output_device.to_owned(),
                t.audio_output_hint.to_owned(),
                AudioControlModel::Static {
                    text: t.audio_output_no_device.to_owned(),
                    dim: true,
                },
                200.0,
            ));
        } else {
            let mut options = vec![t.audio_output_default.to_owned()];
            options.extend(synth.audio_output_devices.iter().cloned());
            let selected = synth
                .selected_audio_output_device
                .as_ref()
                .and_then(|name| {
                    synth
                        .audio_output_devices
                        .iter()
                        .position(|device| device == name)
                        .map(|index| index + 1)
                })
                .unwrap_or(0);
            rows.push(row(
                AudioField::AudioOutput,
                t.audio_output_device.to_owned(),
                t.audio_output_hint.to_owned(),
                combo(options, selected),
                200.0,
            ));
        }
        rows.push(row(
            AudioField::ScanAudioOutputs,
            t.refresh.to_owned(),
            String::new(),
            AudioControlModel::Button,
            0.0,
        ));
    }

    // MIDI 输入设备。
    if midi.devices.is_empty() {
        rows.push(row(
            AudioField::MidiDevice,
            t.midi_input_device.to_owned(),
            String::new(),
            AudioControlModel::Static {
                text: t.no_device.to_owned(),
                dim: true,
            },
            200.0,
        ));
    } else {
        let options: Vec<String> = midi.devices.iter().map(|(_, name)| name.clone()).collect();
        let selected = midi
            .selected_device
            .and_then(|id| midi.devices.iter().position(|(device, _)| *device == id))
            .unwrap_or(0);
        rows.push(row(
            AudioField::MidiDevice,
            t.midi_input_device.to_owned(),
            String::new(),
            combo(options, selected),
            200.0,
        ));
    }

    match synth.backend {
        SynthBackend::XSynth => xsynth_rows(root, t, &mut rows),
        SynthBackend::Lgs => pages::lgs_rows(root, t, &mut rows),
        SynthBackend::System => pages::system_rows(root, t, &mut rows),
        _ => {}
    }

    AudioPageModel {
        title: t.audio_title.to_owned(),
        rows,
    }
}

fn xsynth_rows(
    root: &Root,
    t: &lumino_extras::i18n::SettingsTranslations,
    rows: &mut Vec<AudioRowModel>,
) {
    let settings = &root.settings;
    let synth = &settings.synth;

    rows.push(row(
        AudioField::SoundfontPath,
        t.soundfont.to_owned(),
        t.xsynth_hint.to_owned(),
        AudioControlModel::Text {
            value: synth.soundfont_path.clone(),
            placeholder: t.soundfont_placeholder.to_owned(),
        },
        300.0,
    ));
    rows.push(row(
        AudioField::BrowseSoundfont,
        t.browse.to_owned(),
        String::new(),
        AudioControlModel::Button,
        0.0,
    ));

    rows.push(row(
        AudioField::XSynthBuffer,
        format!("{}: {:.1} ms", t.buffer_latency, synth.xsynth_buffer_ms),
        String::new(),
        slider(synth.xsynth_buffer_ms as f32, 5.0, 100.0, 1.0),
        200.0,
    ));

    let voices = match synth.xsynth_max_voices_per_key {
        None => tr!("不限制").to_string(),
        Some(value) => value.to_string(),
    };
    rows.push(row(
        AudioField::XSynthMaxVoices,
        format!("{}: {voices}", t.max_voices),
        t.max_voices_hint.to_owned(),
        slider(
            match synth.xsynth_max_voices_per_key {
                None => 0.0,
                Some(value) => (value as f32).clamp(0.0, 64.0),
            },
            0.0,
            64.0,
            1.0,
        ),
        160.0,
    ));
    rows.push(row(
        AudioField::XSynthMaxVoicesCustom,
        String::new(),
        String::new(),
        AudioControlModel::Text {
            value: voices,
            placeholder: tr!("0=不限制 1-128").to_string(),
        },
        80.0,
    ));

    let global = match synth.xsynth_global_voice_limit {
        None => format!("{} (10000)", t.global_voice_limit_auto),
        Some(value) => value.to_string(),
    };
    rows.push(row(
        AudioField::XSynthGlobalLimit,
        format!("{}: {global}", t.global_voice_limit),
        t.global_voice_limit_hint.to_owned(),
        slider(
            synth.xsynth_global_voice_limit.unwrap_or(0) as f32,
            0.0,
            20000.0,
            500.0,
        ),
        200.0,
    ));

    rows.push(row(
        AudioField::XSynthRatio,
        format!(
            "{}: {:.3}",
            t.voice_target_ratio, synth.xsynth_voice_target_ratio
        ),
        t.voice_target_ratio_hint.to_owned(),
        slider(
            ((synth.xsynth_voice_target_ratio * 1000.0).round() / 1000.0) as f32,
            0.50,
            0.90,
            0.001,
        ),
        200.0,
    ));
    rows.push(row(
        AudioField::XSynthGate,
        t.soft_nps_gate.to_owned(),
        t.soft_nps_gate_hint.to_owned(),
        AudioControlModel::Checkbox {
            checked: synth.xsynth_soft_nps_gate,
        },
        0.0,
    ));
    rows.push(row(
        AudioField::XSynthVelocity,
        format!(
            "{}: {}",
            t.velocity_filter, settings.midi.velocity_filter_threshold
        ),
        t.velocity_filter_hint.to_owned(),
        slider(
            f32::from(settings.midi.velocity_filter_threshold),
            0.0,
            127.0,
            1.0,
        ),
        200.0,
    ));
}

fn row(
    field: AudioField,
    label: String,
    hint: String,
    control: AudioControlModel,
    width: f32,
) -> AudioRowModel {
    AudioRowModel {
        field,
        label,
        hint,
        control,
        width,
    }
}

fn combo(options: Vec<String>, selected: usize) -> AudioControlModel {
    AudioControlModel::Combo { options, selected }
}

fn slider(value: f32, min: f32, max: f32, step: f64) -> AudioControlModel {
    AudioControlModel::Slider {
        value,
        min,
        max,
        step,
    }
}

fn output_type_name(output: OutputType, language: Language) -> String {
    match output {
        OutputType::Builtin => match language {
            Language::ZhCn => "内置合成器",
            Language::EnUs => "Built-in Synth",
        },
        OutputType::Kdmapi => "KDMAPI",
        OutputType::System => match language {
            Language::ZhCn => "系统 MIDI",
            Language::EnUs => "System MIDI",
        },
    }
    .to_owned()
}

fn engine_name(backend: SynthBackend, _language: Language) -> String {
    match backend {
        SynthBackend::Lgs => "LGS (GPU)",
        _ => "XSynth (Realtime)",
    }
    .to_owned()
}

fn load_display_name(mode: GpuLoadDisplay, language: Language) -> String {
    match (mode, language) {
        (GpuLoadDisplay::ShortAverage, Language::ZhCn) => "3 秒平均",
        (GpuLoadDisplay::ShortAverage, Language::EnUs) => "3s average",
        (GpuLoadDisplay::Instant, Language::ZhCn) => "瞬时",
        (GpuLoadDisplay::Instant, Language::EnUs) => "Instant",
    }
    .to_owned()
}
