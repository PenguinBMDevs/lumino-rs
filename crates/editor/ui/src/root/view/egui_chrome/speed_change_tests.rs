//! 变速对话框：模型镜像与动作映射。

use super::*;
use crate::Message;
use crate::message::SpeedChangeAction as Prod;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::speed_change::SpeedChangeAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::SpeedChange,
    )
}

#[test]
fn speed_change_model_mirrors_state() {
    let mut root = dialog_root();
    root.state.speed_change_dialog.factor_input = "1/3".to_owned();

    let model = root.egui_chrome_model();
    let Some(DialogModel::SpeedChange(panel)) = model.dialog else {
        panic!("变速应由 egui 整窗接管");
    };
    assert!(
        panel.titlebar,
        "默认配置使用自制标题栏：顶部 30px 仍由 旧版 绘制"
    );
    assert_eq!(panel.title, "变速");
    assert_eq!(panel.factor_label, "倍率(S):");
    assert_eq!(panel.factor_hint, "如: 0.5 或 1/3");
    assert_eq!(panel.factor, "1/3");
    assert!(!panel.hint.is_empty());
    assert_eq!(panel.ok, "确定");
    assert_eq!(panel.cancel, "取消");
}

#[test]
fn speed_change_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.speed_change_action_message(&EguiAction::FactorChanged("0.5".to_owned())),
        Some(Message::SpeedChange(Prod::FactorChanged(value))) if value == "0.5"
    ));
    assert!(matches!(
        root.speed_change_action_message(&EguiAction::Confirm),
        Some(Message::SpeedChange(Prod::Confirm))
    ));
    assert!(matches!(
        root.speed_change_action_message(&EguiAction::Cancel),
        Some(Message::SpeedChange(Prod::CloseDialog))
    ));
}
