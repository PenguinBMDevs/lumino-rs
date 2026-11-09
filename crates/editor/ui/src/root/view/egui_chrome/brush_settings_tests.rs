//! 画刷设置：模型镜像（逐层选项 / 选中态 / 最小粗细）与动作映射。

use super::*;
use crate::Message;
use lumino_core::BrushConfig;
use lumino_ui_egui::brush_settings::BrushSettingsAction as EguiAction;
use lumino_ui_egui::chrome::DialogModel;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::BrushSettings,
    )
}

#[test]
fn brush_settings_model_mirrors_draft_and_tracks() {
    let mut root = dialog_root();
    let mut draft = BrushConfig::new();
    draft.tracks = vec![Some(1), None, Some(0)];
    root.state.brush_settings_draft = draft;
    root.state.brush_settings_tracks = vec![(0, "钢琴".to_owned()), (1, "弦乐".to_owned())];

    let model = root.egui_chrome_model();
    let Some(DialogModel::BrushSettings(panel)) = model.dialog else {
        panic!("画刷设置应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.title, "画刷绘制行为设置");
    assert!(panel.has_tracks);
    assert_eq!(panel.levels.len(), 3, "层数 = 草稿层数");
    assert_eq!(panel.levels[0].label, "第 1 层");
    assert_eq!(panel.levels[0].index, 0);

    // 下拉：首项固定「默认（自动分配）」，其余为普通音轨。
    let choices = &panel.levels[0].tracks;
    assert_eq!(choices.len(), 3);
    assert_eq!(choices[0].id, None);
    assert!(!choices[0].selected, "第 1 层已绑音轨 1");
    assert_eq!(choices[1].id, Some(0));
    assert_eq!(choices[1].label, "钢琴");
    assert!(choices[2].selected, "音轨 1（弦乐）应高亮");
    assert!(panel.levels[0].can_remove, "三层 > 最小粗细（1），可删除");

    // 第 2 层未绑定 → 「默认」高亮。
    assert!(panel.levels[1].tracks[0].selected);
    assert_eq!(panel.levels[2].tracks[1].id, Some(0));
    assert!(panel.levels[2].tracks[1].selected);
}

#[test]
fn brush_settings_empty_tracks_and_removable_flags() {
    let mut root = dialog_root();
    let mut draft = BrushConfig::new();
    draft.tracks = vec![None, None, None, None];
    root.state.brush_settings_draft = draft;
    root.state.brush_settings_tracks.clear();

    let model = root.egui_chrome_model();
    let Some(DialogModel::BrushSettings(panel)) = model.dialog else {
        panic!("画刷设置应由 egui 整窗接管");
    };
    assert!(!panel.has_tracks, "无音轨时应给空态提示");
    assert_eq!(panel.empty, "无可用音轨，请先在工程中创建普通音轨。");
    assert_eq!(panel.levels.len(), 4);
    assert!(panel.levels[0].can_remove, "四层 > 最小粗细，可删除");
}

#[test]
fn brush_settings_actions_map_to_messages() {
    let root = dialog_root();
    use crate::message::BrushSettingsAction as Prod;
    assert!(matches!(
        root.brush_settings_action_message(&EguiAction::LevelTrackChanged(1, Some(4))),
        Some(Message::BrushSettings(Prod::LevelTrackChanged(1, Some(4))))
    ));
    assert!(matches!(
        root.brush_settings_action_message(&EguiAction::LevelTrackChanged(0, None)),
        Some(Message::BrushSettings(Prod::LevelTrackChanged(0, None)))
    ));
    assert!(matches!(
        root.brush_settings_action_message(&EguiAction::AddLevel(2)),
        Some(Message::BrushSettings(Prod::AddLevel(2)))
    ));
    assert!(matches!(
        root.brush_settings_action_message(&EguiAction::RemoveLevel(1)),
        Some(Message::BrushSettings(Prod::RemoveLevel(1)))
    ));
    assert!(matches!(
        root.brush_settings_action_message(&EguiAction::Save),
        Some(Message::BrushSettings(Prod::Save))
    ));
    assert!(matches!(
        root.brush_settings_action_message(&EguiAction::Cancel),
        Some(Message::BrushSettings(Prod::Cancel))
    ));
}
