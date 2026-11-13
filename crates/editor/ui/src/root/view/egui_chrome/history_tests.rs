//! 撤销历史对话框：模型镜像与动作映射。

use super::*;
use crate::Message;
use crate::message::HistoryAction as Prod;
use lumino_ui_core::state::{HistoryKind, HistoryStepView};
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::history::HistoryAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::History,
    )
}

#[test]
fn history_model_mirrors_steps_and_cursor() {
    let mut root = dialog_root();
    root.state.history_dialog.steps = vec![
        HistoryStepView {
            index: 1,
            kind: HistoryKind::NoteCreate,
        },
        HistoryStepView {
            index: 2,
            kind: HistoryKind::NoteMove,
        },
        HistoryStepView {
            index: 3,
            kind: HistoryKind::NoteDelete,
        },
    ];
    root.state.history_dialog.current = 1;

    let model = root.egui_chrome_model();
    let Some(DialogModel::History(panel)) = model.dialog else {
        panic!("撤销历史应由 egui 整窗接管");
    };
    assert!(
        panel.titlebar,
        "默认配置使用自制标题栏：顶部 30px 仍由 旧版 绘制"
    );
    assert_eq!(panel.title, "撤销历史");
    assert_eq!(panel.close, "关闭");
    assert_eq!(panel.counter, "1 / 3");
    assert_eq!(panel.rows.len(), 4, "初始状态 + 三个步骤");

    assert_eq!(panel.rows[0].label, "初始状态");
    assert_eq!(panel.rows[0].index, 0);
    assert!(!panel.rows[0].current, "游标在 1，初始状态不是当前行");
    assert!(!panel.rows[0].future);

    assert_eq!(panel.rows[1].label, "新建音符");
    assert!(panel.rows[1].current, "第 1 步是当前游标");
    assert!(!panel.rows[1].future);

    assert_eq!(panel.rows[2].label, "移动音符");
    assert!(!panel.rows[2].current);
    assert!(panel.rows[2].future, "游标之后的步骤应为浅色");

    assert_eq!(panel.rows[3].label, "删除音符");
    assert!(panel.rows[3].future);
}

#[test]
fn history_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.history_action_message(&EguiAction::Jump(2)),
        Some(Message::History(Prod::Jump(2)))
    ));
    assert!(matches!(
        root.history_action_message(&EguiAction::Close),
        Some(Message::History(Prod::Close))
    ));
}
