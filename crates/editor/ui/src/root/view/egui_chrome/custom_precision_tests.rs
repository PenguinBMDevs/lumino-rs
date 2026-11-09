//! 自定义精度对话框：模型镜像与动作映射。

use super::*;
use crate::Message;
use crate::message::CustomPrecisionAction as Prod;
use lumino_core::DotType;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::custom_precision::CustomPrecisionAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::CustomPrecision,
    )
}

#[test]
fn custom_precision_model_mirrors_state() {
    let mut root = dialog_root();
    root.state.custom_precision_dialog.tuplet_count = "5".to_owned();
    root.state.custom_precision_dialog.note_value = "8".to_owned();
    root.state.custom_precision_dialog.divisor = "2".to_owned();
    root.state.custom_precision_dialog.dot_type = DotType::Single;
    root.state.custom_precision_dialog.error = Some("除数不能为 0".to_owned());

    let model = root.egui_chrome_model();
    let Some(DialogModel::CustomPrecision(panel)) = model.dialog else {
        panic!("自定义精度应由 egui 整窗接管");
    };
    assert!(
        panel.titlebar,
        "默认配置使用自制标题栏：顶部 30px 仍由 iced 绘制"
    );
    assert_eq!(panel.tuplet_count, "5");
    assert_eq!(panel.note_value, "8");
    assert_eq!(panel.divisor, "2");
    assert_eq!(panel.note_label, "分音符");
    assert_eq!(panel.divide_label, "除以");
    assert_eq!(panel.dot_label, "附点");
    assert_eq!(panel.error.as_deref(), Some("除数不能为 0"));
    assert_eq!(panel.ok, "确定");
    assert_eq!(panel.cancel, "取消");

    assert_eq!(panel.dots.len(), 4, "四个附点选项常显");
    assert_eq!(panel.dots[0].dot, DotType::None);
    assert_eq!(panel.dots[1].dot, DotType::Tuplet);
    assert_eq!(panel.dots[2].dot, DotType::Single);
    assert!(panel.dots[2].selected, "当前附点类型应高亮");
    assert!(!panel.dots[0].selected);
    assert_eq!(panel.dots[3].dot, DotType::Double);
}

#[test]
fn custom_precision_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.custom_precision_action_message(&EguiAction::TupletCountChanged("7".to_owned())),
        Some(Message::CustomPrecision(Prod::TupletCountChanged(value))) if value == "7"
    ));
    assert!(matches!(
        root.custom_precision_action_message(&EguiAction::NoteValueChanged("16".to_owned())),
        Some(Message::CustomPrecision(Prod::NoteValueChanged(value))) if value == "16"
    ));
    assert!(matches!(
        root.custom_precision_action_message(&EguiAction::DotTypeChanged(DotType::Double)),
        Some(Message::CustomPrecision(Prod::DotTypeChanged(
            DotType::Double
        )))
    ));
    assert!(matches!(
        root.custom_precision_action_message(&EguiAction::DivisorChanged("3".to_owned())),
        Some(Message::CustomPrecision(Prod::DivisorChanged(value))) if value == "3"
    ));
    assert!(matches!(
        root.custom_precision_action_message(&EguiAction::Confirm),
        Some(Message::CustomPrecision(Prod::Confirm))
    ));
    assert!(matches!(
        root.custom_precision_action_message(&EguiAction::Cancel),
        Some(Message::CustomPrecision(Prod::CloseDialog))
    ));
}
