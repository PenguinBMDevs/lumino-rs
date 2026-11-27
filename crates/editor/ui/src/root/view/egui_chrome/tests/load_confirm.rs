//! 载入确认：模型镜像与动作映射。

use super::*;
use crate::Message;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::load_confirm::LoadConfirmAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::LoadConfirm,
    )
}

#[test]
fn load_confirm_model_mirrors_state() {
    let mut root = dialog_root();
    root.state.load_confirm_dialog.file_name = "huge.lmpj".to_owned();
    root.state.load_confirm_dialog.file_path = "D:/projects/huge.lmpj".to_owned();
    root.state.load_confirm_dialog.size_mb = 128.44;

    let model = root.egui_chrome_model();
    let Some(DialogModel::LoadConfirm(panel)) = model.dialog else {
        panic!("载入确认应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.file_line, "文件: huge.lmpj");
    assert_eq!(panel.size_line, "大小: 128.4 MB");
    assert_eq!(panel.path_line, "路径: D:/projects/huge.lmpj");
    assert_eq!(panel.hint, "此文件较大，加载可能需要一些时间…");
    assert_eq!(panel.cancel, "取消");
    assert_eq!(panel.confirm, "加载");
}

#[test]
fn load_confirm_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.load_confirm_action_message(&EguiAction::Confirm),
        Some(Message::LoadConfirm(
            crate::message::LoadConfirmAction::Confirm
        ))
    ));
    assert!(matches!(
        root.load_confirm_action_message(&EguiAction::Cancel),
        Some(Message::LoadConfirm(
            crate::message::LoadConfirmAction::CloseDialog
        ))
    ));
}
