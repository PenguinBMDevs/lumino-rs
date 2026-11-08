//! 批量编辑对话框：模型镜像与动作映射。

use super::*;
use crate::Message;
use crate::message::BatchEditAction as Prod;
use lumino_ui_egui::batch_edit::BatchEditAction as EguiAction;
use lumino_ui_egui::chrome::{BatchEditFieldModel, DialogModel};

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::BatchEdit,
    )
}

#[test]
fn batch_edit_model_mirrors_state() {
    let mut root = dialog_root();
    root.state.batch_edit_dialog.velocity_input = "+20".to_owned();
    root.state.batch_edit_dialog.gate_input = "*1.5".to_owned();
    root.state.batch_edit_dialog.key_input = "-3".to_owned();
    root.state.batch_edit_dialog.tick_input = "480".to_owned();

    let model = root.egui_chrome_model();
    let Some(DialogModel::BatchEdit(panel)) = model.dialog else {
        panic!("批量编辑应由 egui 整窗接管");
    };
    assert!(
        panel.titlebar,
        "默认配置使用自制标题栏：顶部 30px 仍由 iced 绘制"
    );
    assert_eq!(panel.title, "批量编辑");
    assert_eq!(panel.settings_title, "设置");
    assert_eq!(panel.format_title, "格式");
    assert_eq!(panel.rows.len(), 4);
    assert_eq!(panel.rows[0].field, BatchEditFieldModel::Velocity);
    assert_eq!(panel.rows[0].label, "音符力度:");
    assert_eq!(panel.rows[0].placeholder, "如: +20");
    assert_eq!(panel.rows[0].value, "+20");
    assert_eq!(panel.rows[1].field, BatchEditFieldModel::Gate);
    assert_eq!(panel.rows[1].value, "*1.5");
    assert_eq!(panel.rows[2].field, BatchEditFieldModel::Key);
    assert_eq!(panel.rows[2].value, "-3");
    assert_eq!(panel.rows[3].field, BatchEditFieldModel::Tick);
    assert_eq!(panel.rows[3].value, "480");
    assert_eq!(panel.format_hints.len(), 4, "四行格式说明");
    assert_eq!(panel.ok, "确定");
    assert_eq!(panel.cancel, "取消");
}

#[test]
fn batch_edit_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.batch_edit_action_message(&EguiAction::InputChanged(
            BatchEditFieldModel::Gate,
            "*2".to_owned()
        )),
        Some(Message::BatchEdit(Prod::InputChanged(
            crate::message::BatchEditField::Gate,
            value
        ))) if value == "*2"
    ));
    assert!(matches!(
        root.batch_edit_action_message(&EguiAction::InputChanged(
            BatchEditFieldModel::Velocity,
            "+5".to_owned()
        )),
        Some(Message::BatchEdit(Prod::InputChanged(
            crate::message::BatchEditField::Velocity,
            value
        ))) if value == "+5"
    ));
    assert!(matches!(
        root.batch_edit_action_message(&EguiAction::Confirm),
        Some(Message::BatchEdit(Prod::Confirm))
    ));
    assert!(matches!(
        root.batch_edit_action_message(&EguiAction::Cancel),
        Some(Message::BatchEdit(Prod::CloseDialog))
    ));
}
