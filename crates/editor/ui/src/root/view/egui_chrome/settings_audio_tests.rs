//! 音频页：三后端行镜像与字段动作翻译（从 `settings_page_tests.rs` 拆出）。

use super::*;
use lumino_core::storage::config::SynthBackend;
use lumino_ui_core::settings_event::OutputType;
use lumino_ui_egui::chrome::{AudioField, DialogModel, SettingsPageModel};
use lumino_ui_egui::settings::SettingsAction;

use crate::Message;

fn settings_root() -> Root {
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let config = lumino_core::storage::config::UiConfig::default();
    Root::new_settings_dialog(&theme, &config)
}

fn settings_model(root: &mut Root) -> lumino_ui_egui::chrome::SettingsSnapshot {
    let model = root.egui_chrome_model();
    let Some(DialogModel::Settings(values)) = model.dialog else {
        panic!("已迁移页面应整窗接管");
    };
    *values
}

#[test]
fn audio_page_reflects_backend_and_maps_actions() {
    let mut root = settings_root();
    root.settings.selected_menu_index = 1;

    // XSynth：顶层选择 + 引擎 + 设备 + 音色库 + 参数行。
    root.settings.synth.backend = SynthBackend::XSynth;
    let values = settings_model(&mut root);
    let SettingsPageModel::Audio(page) = values.page else {
        panic!("第 1 页应为音频");
    };
    let fields: Vec<AudioField> = page.rows.iter().map(|row| row.field).collect();
    for field in [
        AudioField::OutputType,
        AudioField::Engine,
        AudioField::MidiDevice,
        AudioField::SoundfontPath,
        AudioField::XSynthBuffer,
        AudioField::XSynthMaxVoices,
        AudioField::XSynthGlobalLimit,
        AudioField::XSynthRatio,
        AudioField::XSynthGate,
        AudioField::XSynthVelocity,
    ] {
        assert!(fields.contains(&field), "XSynth 页缺少 {field:?}");
    }
    assert!(!page.title.is_empty());

    // LGS：块大小 / 离线块 / GPU 指标。
    root.settings.synth.backend = SynthBackend::Lgs;
    let values = settings_model(&mut root);
    let SettingsPageModel::Audio(page) = values.page else {
        panic!("音频页");
    };
    let fields: Vec<AudioField> = page.rows.iter().map(|row| row.field).collect();
    for field in [
        AudioField::LgsBlock,
        AudioField::LgsOfflineBlock,
        AudioField::LgsMaxVoices,
        AudioField::LgsGlobalLimit,
        AudioField::LgsGate,
        AudioField::LgsGpuBufferStatus,
        AudioField::LgsGpuGovernorMetrics,
        AudioField::LgsGpuLoadDisplay,
    ] {
        assert!(fields.contains(&field), "LGS 页缺少 {field:?}");
    }

    // System：WinMM 输出。
    root.settings.synth.backend = SynthBackend::System;
    let values = settings_model(&mut root);
    let SettingsPageModel::Audio(page) = values.page else {
        panic!("音频页");
    };
    assert!(
        page.rows
            .iter()
            .any(|row| row.field == AudioField::WinmmOutput),
        "系统 MIDI 页应有 WinMM 输出选择"
    );

    // 动作 → 生产 Message。
    assert!(matches!(
        root.settings_action_message(&SettingsAction::AudioCombo {
            field: AudioField::OutputType,
            index: 2,
        }),
        Some(Message::Settings(
            crate::settings::Event::OutputTypeChanged(OutputType::System)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::AudioCombo {
            field: AudioField::Engine,
            index: 1,
        }),
        Some(Message::Settings(
            crate::settings::Event::SynthBackendChanged(SynthBackend::Lgs)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::AudioSlider {
            field: AudioField::XSynthBuffer,
            value: 20.0,
        }),
        Some(Message::Settings(crate::settings::Event::XSynthBufferChanged(
            value
        ))) if (value - 20.0).abs() < f64::EPSILON
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::AudioSlider {
            field: AudioField::LgsBlock,
            value: 10.0,
        }),
        Some(Message::Settings(
            crate::settings::Event::LgsBlockSizeChanged(1024)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::AudioToggle {
            field: AudioField::XSynthGate,
            checked: true,
        }),
        Some(Message::Settings(
            crate::settings::Event::XSynthSoftNpsGateChanged(true)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::AudioButton {
            field: AudioField::BrowseSoundfont,
        }),
        Some(Message::Settings(crate::settings::Event::BrowseSoundfont))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::AudioText {
            field: AudioField::SoundfontPath,
            value: "C:/sf2/demo.sf2".to_owned(),
        }),
        Some(Message::Settings(
            crate::settings::Event::SoundfontPathChanged(path)
        )) if path == "C:/sf2/demo.sf2"
    ));
}
