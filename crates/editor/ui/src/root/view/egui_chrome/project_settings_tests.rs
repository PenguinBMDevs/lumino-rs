//! 工程设置对话框：模型镜像与动作映射。

use super::*;
use crate::Message;
use crate::message::ProjectSettingsAction as Prod;
use lumino_ui_core::state::ProjectSettingsDialogState;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::project_settings::ProjectSettingsAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::ProjectSettings,
    )
}

#[test]
fn project_settings_model_mirrors_dialog_state() {
    let mut root = dialog_root();
    root.state.project_settings_dialog = ProjectSettingsDialogState {
        is_open: true,
        title: "我的工程".to_owned(),
        tempo: "96".to_owned(),
        copyright: "© 2026".to_owned(),
        author: "张三".to_owned(),
        created_display: "2026-07-01 10:00:00".to_owned(),
        total_editing_time_seconds: 66.0,
        note_count: 1234,
        note_count_display: lumino_core::storage::config::NoteCountDisplay::StatusBar,
        time_signature_numerator: "6".to_owned(),
        time_signature_denominator: "8".to_owned(),
    };

    let model = root.egui_chrome_model();
    let Some(DialogModel::ProjectSettings(panel)) = model.dialog else {
        panic!("工程设置应由 egui 整窗接管");
    };
    assert_eq!(panel.title, "工程信息设置");
    assert!(
        panel.titlebar,
        "默认配置使用自制标题栏：顶部 30px 仍由 旧版 绘制"
    );
    assert_eq!(panel.name, "我的工程");
    assert_eq!(panel.bpm, "96");
    assert_eq!(panel.copyright, "© 2026");
    assert_eq!(panel.author, "张三");
    assert_eq!(panel.numerator, "6");
    assert_eq!(panel.denominator, "8");
    assert_eq!(panel.created, "2026-07-01 10:00:00");
    assert_eq!(panel.editing_time, "1 分钟 6 秒");
    assert_eq!(panel.ok, "确定");
    assert_eq!(panel.cancel, "取消");
    assert!(
        panel.note_count.is_none(),
        "音符总量显示在状态栏时，对话框里不得重复显示"
    );

    // 切到「工程设置里显示」时才出现只读行（千位分隔）。
    root.state.project_settings_dialog.note_count_display =
        lumino_core::storage::config::NoteCountDisplay::ProjectSettings;
    let model = root.egui_chrome_model();
    let Some(DialogModel::ProjectSettings(panel)) = model.dialog else {
        panic!("工程设置应由 egui 整窗接管");
    };
    let note = panel.note_count.expect("应出现音符总量行");
    assert_eq!(note.label, "音符总量");
    assert_eq!(note.value, "1,234");

    // 创建日期为空时显示「未知」。
    root.state.project_settings_dialog.created_display.clear();
    let model = root.egui_chrome_model();
    let Some(DialogModel::ProjectSettings(panel)) = model.dialog else {
        panic!("工程设置应由 egui 整窗接管");
    };
    assert_eq!(panel.created, "未知");
}

#[test]
fn project_settings_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::NameChanged("新名称".to_owned())),
        Some(Message::ProjectSettings(Prod::TitleChanged(value))) if value == "新名称"
    ));
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::TempoChanged("128".to_owned())),
        Some(Message::ProjectSettings(Prod::TempoChanged(value))) if value == "128"
    ));
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::CopyrightChanged("© 2027".to_owned())),
        Some(Message::ProjectSettings(Prod::CopyrightChanged(value))) if value == "© 2027"
    ));
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::AuthorChanged("李四".to_owned())),
        Some(Message::ProjectSettings(Prod::AuthorChanged(value))) if value == "李四"
    ));
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::NumeratorChanged("3".to_owned())),
        Some(Message::ProjectSettings(Prod::TimeSignatureNumeratorChanged(value))) if value == "3"
    ));
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::DenominatorChanged("4".to_owned())),
        Some(Message::ProjectSettings(Prod::TimeSignatureDenominatorChanged(value))) if value == "4"
    ));
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::Confirm),
        Some(Message::ProjectSettings(Prod::Confirm))
    ));
    assert!(matches!(
        root.project_settings_action_message(&EguiAction::Cancel),
        Some(Message::ProjectSettings(Prod::CloseDialog))
    ));
}
