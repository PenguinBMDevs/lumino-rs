//! 设置对话框：已迁移页面的模型镜像与动作翻译。

use super::*;
use lumino_core::storage::config::{EraserBehavior, OnionSkinBackend, TrackAddBehavior};
use lumino_ui_egui::chrome::{DialogModel, SettingsPageModel};
use lumino_ui_egui::settings::SettingsAction;

use crate::Message;
use crate::message::SettingsDialogAction;

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

    // 未迁移页面（音频）仍交回 iced。
    root.settings.selected_menu_index = 1;
    assert!(
        root.egui_chrome_model().dialog.is_none(),
        "未迁移页面必须整块让位"
    );
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
