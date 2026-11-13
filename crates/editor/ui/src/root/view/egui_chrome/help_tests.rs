//! Help 窗口：模型镜像与动作映射。

use super::*;
use crate::Message;
use lumino_message::HelpAction as Prod;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::help::HelpAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::Help,
    )
}

#[test]
fn help_model_lists_sections_and_body() {
    let mut root = dialog_root();
    root.state.help_dialog.query.clear();
    root.state.help_dialog.selected = 0;

    let model = root.egui_chrome_model();
    let Some(DialogModel::Help(panel)) = model.dialog else {
        panic!("Help 应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.title, "帮助");
    assert_eq!(panel.search_hint, "搜索帮助...");
    assert_eq!(panel.empty, "没有匹配的话题");
    assert_eq!(panel.reset, "全部重现");
    assert!(!panel.close.is_empty());

    assert!(!panel.sections.is_empty(), "空查询应列出全部话题");
    let total: usize = panel.sections.iter().map(|s| s.topics.len()).sum();
    assert!(total >= 4, "话题总数应覆盖全部内置话题，实际 {total}");
    // 第一行必须是结果下标 0 且高亮（`selected == 0`）。
    let first = &panel.sections[0].topics[0];
    assert_eq!(first.index, 0);
    assert!(first.active, "选中项应高亮");
    assert!(!first.label.is_empty());

    // 正文：标题 / tip / 段落。
    let body = panel.topic.expect("应有当前话题正文");
    assert!(!body.title.is_empty());
    assert!(!body.tip.is_empty());
    assert!(!body.paragraphs.is_empty(), "正文应按空行切段");
    assert_eq!(body.title, first.label, "列表高亮项与正文标题同源");

    // 搜索无结果：分区清空、正文为空、文案为「没有匹配的话题」。
    root.state.help_dialog.query = "zzz-不存在的关键词".to_owned();
    root.state.help_dialog.selected = 0;
    let model = root.egui_chrome_model();
    let Some(DialogModel::Help(panel)) = model.dialog else {
        panic!("Help 应由 egui 整窗接管");
    };
    assert!(panel.sections.is_empty(), "无匹配时不应有分区");
    assert!(panel.topic.is_none(), "无匹配时不应有正文");
    assert_eq!(panel.query, "zzz-不存在的关键词");
}

#[test]
fn help_actions_map_to_messages() {
    let root = dialog_root();
    assert!(matches!(
        root.help_action_message(&EguiAction::QueryChanged("brush".to_owned())),
        Some(Message::Help(Prod::QueryChanged(value))) if value == "brush"
    ));
    assert!(matches!(
        root.help_action_message(&EguiAction::TopicSelected(3)),
        Some(Message::Help(Prod::TopicSelected(3)))
    ));
    assert!(matches!(
        root.help_action_message(&EguiAction::ResetTips),
        Some(Message::Help(Prod::ResetTips))
    ));
    assert!(matches!(
        root.help_action_message(&EguiAction::Close),
        Some(Message::Help(Prod::Close))
    ));
}
