//! 设置对话框：已迁移页面的模型镜像与动作翻译。

use super::*;
use lumino_core::storage::config::{
    EraserBehavior, OnionSkinBackend, SynthBackend, TrackAddBehavior,
};
use lumino_ui_core::settings_event::OutputType;
use lumino_ui_egui::chrome::{AudioField, DialogModel, SettingsPageModel, UiField, UiRow};
use lumino_ui_egui::settings::SettingsAction;

use crate::Message;
use crate::message::SettingsDialogAction;
use crate::settings::Event;

fn settings_root() -> Root {
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let config = lumino_core::storage::config::UiConfig::default();
    Root::new_settings_dialog(&theme, &config)
}

fn settings_model(root: &mut Root) -> lumino_ui_egui::chrome::SettingsSnapshot {
    let model = root.egui_chrome_model();
    let Some(DialogModel::Settings(values)) = model.dialog else {
        panic!(
            "已迁移页面应整窗接管（selected={}）",
            root.settings.selected_menu_index
        );
    };
    *values
}

#[test]
fn migrated_settings_pages_are_taken_over_only() {
    let mut root = settings_root();

    // 快捷键页。
    root.settings.selected_menu_index = 3;
    let values = settings_model(&mut root);
    let SettingsPageModel::Shortcuts(shortcuts) = values.page else {
        panic!("第 3 页应为快捷键");
    };
    assert!(!shortcuts.title.is_empty());
    assert!(!shortcuts.placeholder.is_empty());

    // 洋葱皮页。
    root.settings.selected_menu_index = 4;
    root.settings.onion_skin.backend = OnionSkinBackend::Cpu;
    root.settings.onion_skin.backend_locked = false;
    let values = settings_model(&mut root);
    let SettingsPageModel::OnionSkin(onion) = values.page else {
        panic!("第 4 页应为洋葱皮");
    };
    assert_eq!(onion.options, vec!["GPU".to_owned(), "CPU".to_owned()]);
    assert_eq!(onion.selected, 1, "CPU = 选项下标 1");
    assert!(!onion.locked);
    assert!(!onion.hint.is_empty());
    assert!(!onion.locked_hint.is_empty());

    // 锁定态镜像。
    root.settings.onion_skin.backend_locked = true;
    let values = settings_model(&mut root);
    let SettingsPageModel::OnionSkin(onion) = values.page else {
        panic!("洋葱皮页");
    };
    assert!(onion.locked);
    assert_eq!(onion.locked_value, "CPU");

    // 兼容性页。
    root.settings.selected_menu_index = 8;
    root.settings.compat.check_on_startup = true;
    root.settings.compat.warning_suppressed = false;
    let values = settings_model(&mut root);
    let SettingsPageModel::Compatibility(compat) = values.page else {
        panic!("第 8 页应为兼容性");
    };
    assert!(!compat.title.is_empty());
    assert!(compat.check_on_startup);
    assert!(compat.show_warning);
    assert!(!compat.domino_label.is_empty());
    assert_eq!(compat.domino_enabled, cfg!(windows));
    assert!(!compat.result_lines.is_empty(), "至少有「尚未检测」结果行");

    // 未迁移页面（调色板）仍交回 iced。
    root.settings.selected_menu_index = 5;
    assert!(
        root.egui_chrome_model().dialog.is_none(),
        "未迁移页面必须整块让位"
    );
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

#[test]
fn ui_page_reflects_settings_and_maps_actions() {
    let mut root = settings_root();
    root.settings.selected_menu_index = 2;
    root.settings.display.enable_256key = true;
    root.settings.editing.program_font_path = "C:/fonts/demo.ttf".to_owned();
    root.settings.editing.automation_line_thickness = 3.5;

    let values = settings_model(&mut root);
    let SettingsPageModel::Ui(page) = values.page else {
        panic!("第 2 页应为界面");
    };
    assert!(!page.title.is_empty());
    let has_field = |field| {
        page.rows.iter().any(|row| match row {
            UiRow::Combo { field: f, .. }
            | UiRow::Text { field: f, .. }
            | UiRow::Checkbox { field: f, .. }
            | UiRow::Slider { field: f, .. }
            | UiRow::Button { field: f, .. } => *f == field,
            _ => false,
        })
    };
    for field in [
        UiField::Language,
        UiField::ProgramFontName,
        UiField::ProgramFontPath,
        UiField::BrowseProgramFont,
        UiField::IconHidpi,
        UiField::NoteCountDisplay,
        UiField::AutoScrollFixed,
        UiField::AutoScrollTrigger,
        UiField::AutoScrollReturn,
        UiField::SelectionBoxMode,
        UiField::Enable256Key,
        UiField::VelocityCurveStyle,
        UiField::PlaybackKeyColors,
        UiField::AutomationLineThickness,
        UiField::MonitorRefreshInterval,
    ] {
        assert!(has_field(field), "界面页缺少 {field:?}");
    }
    let cards = page
        .rows
        .iter()
        .find_map(|row| match row {
            UiRow::ThemeCards { cards, .. } => Some(cards),
            _ => None,
        })
        .expect("主题卡片行");
    assert_eq!(
        cards.len(),
        lumino_ui_core::Theme::ALL.len() + 1,
        "高对比度 + 注册表全部主题"
    );
    assert_eq!(cards[0].display, "高对比度");

    // 动作 → 生产 Message。
    assert!(matches!(
        root.settings_action_message(&SettingsAction::UiTheme("Light".to_owned())),
        Some(Message::Window(crate::window::Event::Theme(value))) if value == "Light"
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::UiCombo {
            field: UiField::Language,
            index: 2,
        }),
        Some(Message::Settings(Event::LanguagePreferenceChanged(
            lumino_core::locale::LanguagePreference::Fixed(_)
        )))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::UiCombo {
            field: UiField::NoteCountDisplay,
            index: 1,
        }),
        Some(Message::Settings(Event::NoteCountDisplayChanged(
            lumino_core::storage::config::NoteCountDisplay::StatusBar
        )))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::UiToggle {
            field: UiField::Enable256Key,
            checked: true,
        }),
        Some(Message::Settings(Event::Enable256keyChanged(true)))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::UiText {
            field: UiField::AutoScrollFixed,
            value: "120".to_owned(),
        }),
        Some(Message::Settings(Event::AutoScrollFixedPositionChanged(value)))
            if value == "120"
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::UiSlider {
            field: UiField::MonitorRefreshInterval,
            value: 250.0,
        }),
        Some(Message::Settings(Event::MonitorRefreshIntervalChanged(value)))
            if (value - 250.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::UiButton {
            field: UiField::BrowseProgramFont,
        }),
        Some(Message::Settings(Event::BrowseProgramFont))
    ));
}

#[test]
fn settings_actions_map_to_production_messages() {
    let root = settings_root();

    assert!(matches!(
        root.settings_action_message(&SettingsAction::MenuSelected(4)),
        Some(Message::Settings(crate::settings::Event::MenuSelected(4)))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::EraserBehavior(
            EraserBehavior::DirectSelect
        )),
        Some(Message::Settings(
            crate::settings::Event::EraserBehaviorChanged(EraserBehavior::DirectSelect)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::TrackAddBehavior(
            TrackAddBehavior::StayCurrent
        )),
        Some(Message::Settings(
            crate::settings::Event::TrackAddBehaviorChanged(TrackAddBehavior::StayCurrent)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::LogRetention("25".to_owned())),
        Some(Message::Settings(
            crate::settings::Event::LogRetentionCountChanged(value)
        )) if value == "25"
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::OnionSkinBackend(OnionSkinBackend::Cpu)),
        Some(Message::Settings(
            crate::settings::Event::OnionSkinBackendChanged(OnionSkinBackend::Cpu)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::GpuCheckOnStartup(false)),
        Some(Message::Settings(
            crate::settings::Event::GpuCheckOnStartupChanged(false)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::GpuWarningSuppressed(true)),
        Some(Message::Settings(
            crate::settings::Event::GpuWarningSuppressedChanged(true)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::DominoClipboard(true)),
        Some(Message::Settings(
            crate::settings::Event::DominoClipboardEnabledChanged(true)
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::RunGpuCheck),
        Some(Message::Settings(
            crate::settings::Event::RunGpuCompatibilityCheck
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::CopyGpuDiagnostics),
        Some(Message::Settings(
            crate::settings::Event::CopyGpuDiagnostics
        ))
    ));
    assert!(matches!(
        root.settings_action_message(&SettingsAction::Confirm),
        Some(Message::SettingsDialog(SettingsDialogAction::CloseDialog))
    ));
}
