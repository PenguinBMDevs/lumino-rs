//! 音频页 LGS / 系统 MIDI 行（从 `settings_audio.rs` 拆出）。

use lumino_core::storage::config::GpuLoadDisplay;
use lumino_core::tr;
use lumino_ui_egui::chrome::{AudioControlModel, AudioField, AudioRowModel};

use super::{combo, load_display_name, row, slider};
use crate::root::Root;

pub(super) fn lgs_rows(
    root: &Root,
    t: &lumino_extras::i18n::SettingsTranslations,
    rows: &mut Vec<AudioRowModel>,
) {
    let settings = &root.settings;
    let synth = &settings.synth;

    rows.push(row(
        AudioField::SoundfontPath,
        t.soundfont.to_owned(),
        t.lgs_hint.to_owned(),
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

    let block_index = ((synth.lgs_block_size as f64).log2().round() as f32).clamp(6.0, 13.0);
    rows.push(row(
        AudioField::LgsBlock,
        format!("{}: {}", t.lgs_buffer, synth.lgs_block_size),
        t.lgs_buffer_hint.to_owned(),
        slider(block_index, 6.0, 13.0, 1.0),
        200.0,
    ));

    let offline_options = [512usize, 1024, 2048, 4096, 8192];
    let offline_selected = offline_options
        .iter()
        .position(|value| *value == synth.lgs_offline_block_size)
        .unwrap_or(2);
    rows.push(row(
        AudioField::LgsOfflineBlock,
        format!("{}: {}", t.lgs_offline_buffer, synth.lgs_offline_block_size),
        t.lgs_offline_buffer_hint.to_owned(),
        combo(
            offline_options
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>(),
            offline_selected,
        ),
        160.0,
    ));

    let voices = if synth.lgs_max_voices_per_key == 0 {
        tr!("不限制").to_string()
    } else {
        synth.lgs_max_voices_per_key.to_string()
    };
    rows.push(row(
        AudioField::LgsMaxVoices,
        format!("{}: {voices}", t.max_voices),
        t.max_voices_hint.to_owned(),
        slider(synth.lgs_max_voices_per_key as f32, 0.0, 128.0, 1.0),
        160.0,
    ));

    let global = match synth.lgs_global_voice_limit {
        None => format!(
            "{} ({})",
            t.global_voice_limit_auto,
            lumino_core::storage::config::LGS_AUTO_MAX_VOICES
        ),
        Some(value) => value.to_string(),
    };
    rows.push(row(
        AudioField::LgsGlobalLimit,
        format!("{}: {global}", t.global_voice_limit),
        t.global_voice_limit_hint.to_owned(),
        slider(
            synth.lgs_global_voice_limit.unwrap_or(0) as f32,
            0.0,
            65536.0,
            1024.0,
        ),
        200.0,
    ));

    rows.push(row(
        AudioField::LgsGate,
        t.lgs_nps_gate.to_owned(),
        t.lgs_nps_gate_hint.to_owned(),
        AudioControlModel::Checkbox {
            checked: synth.lgs_soft_nps_gate,
        },
        0.0,
    ));
    rows.push(row(
        AudioField::LgsVelocity,
        format!(
            "{}: {}",
            t.velocity_filter, synth.lgs_velocity_filter_threshold
        ),
        t.velocity_filter_hint.to_owned(),
        slider(
            f32::from(synth.lgs_velocity_filter_threshold),
            0.0,
            127.0,
            1.0,
        ),
        200.0,
    ));
    rows.push(row(
        AudioField::LgsGpuBufferStatus,
        t.gpu_buffer_status.to_owned(),
        t.gpu_buffer_status_hint.to_owned(),
        AudioControlModel::Checkbox {
            checked: synth.gpu_buffer_status_enabled,
        },
        0.0,
    ));
    rows.push(row(
        AudioField::LgsGpuGovernorMetrics,
        t.gpu_governor_metrics.to_owned(),
        t.gpu_governor_metrics_hint.to_owned(),
        AudioControlModel::Checkbox {
            checked: synth.gpu_governor_metrics_enabled,
        },
        0.0,
    ));
    let load_options = [GpuLoadDisplay::ShortAverage, GpuLoadDisplay::Instant];
    let load_selected = load_options
        .iter()
        .position(|mode| *mode == synth.gpu_load_display)
        .unwrap_or(0);
    rows.push(row(
        AudioField::LgsGpuLoadDisplay,
        t.gpu_load_display.to_owned(),
        t.gpu_load_display_hint.to_owned(),
        combo(
            load_options
                .iter()
                .map(|mode| load_display_name(*mode, settings.display.language))
                .collect(),
            load_selected,
        ),
        160.0,
    ));
}

pub(super) fn system_rows(
    root: &Root,
    t: &lumino_extras::i18n::SettingsTranslations,
    rows: &mut Vec<AudioRowModel>,
) {
    let midi = &root.settings.midi;
    if midi.winmm_outputs.is_empty() {
        rows.push(row(
            AudioField::WinmmOutput,
            t.winmm_output_device.to_owned(),
            t.system_hint.to_owned(),
            AudioControlModel::Static {
                text: t.winmm_no_device.to_owned(),
                dim: true,
            },
            200.0,
        ));
    } else {
        let options: Vec<String> = midi
            .winmm_outputs
            .iter()
            .map(|(_, name)| name.clone())
            .collect();
        let selected = midi
            .selected_winmm_output
            .and_then(|id| {
                midi.winmm_outputs
                    .iter()
                    .position(|(output, _)| *output == id)
            })
            .unwrap_or(0);
        rows.push(row(
            AudioField::WinmmOutput,
            t.winmm_output_device.to_owned(),
            t.system_hint.to_owned(),
            combo(options, selected),
            200.0,
        ));
    }
    rows.push(row(
        AudioField::ScanWinmmOutputs,
        t.refresh.to_owned(),
        String::new(),
        AudioControlModel::Button,
        0.0,
    ));
}
