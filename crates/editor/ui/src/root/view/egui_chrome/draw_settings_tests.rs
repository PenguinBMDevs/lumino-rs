//! 音符画设置：模型镜像与关闭动作映射。

use super::*;
use crate::Message;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::draw_settings::DrawSettingsAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::DrawSettings,
    )
}

#[test]
fn draw_settings_model_carries_production_texts() {
    let root = dialog_root();
    let model = root.egui_chrome_model();
    let Some(DialogModel::DrawSettings(panel)) = model.dialog else {
        panic!("音符画设置应由 egui 整窗接管");
    };
    let t = lumino_extras::i18n::main_translations(lumino_extras::i18n::Language::ZhCn);
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.title, t.tool_panel_settings);
    assert_eq!(panel.placeholder, t.draw_settings_placeholder);
    assert_eq!(panel.close, t.file_close);
    assert!(!panel.placeholder.is_empty(), "占位说明不得为空");
}

#[test]
fn draw_settings_close_maps_to_message() {
    let root = dialog_root();
    let message = root
        .draw_settings_action_message(&DrawSettingsAction::Close)
        .expect("关闭动作应映射为消息");
    assert!(
        matches!(
            message,
            Message::DrawSettings(crate::message::DrawSettingsAction::CloseDialog)
        ),
        "关闭应映射为 CloseDialog"
    );
}
