//! 找回删除音轨：模型镜像（表头 / 条目 / 选中载荷）与动作映射。

use super::*;
use crate::Message;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::recover_track::RecoverTrackAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::RecoverTrack,
    )
}

#[test]
fn recover_track_model_mirrors_entries_and_selection() {
    let mut root = dialog_root();
    root.state.recover_track_dialog.entries = vec![
        crate::state::root_state::RecoverTrackEntry {
            path: std::path::PathBuf::from("C:/cache/track3.lmdeltrack"),
            filename: "track3.lmdeltrack".to_owned(),
            deleted_at: "2026-11-01 12:00".to_owned(),
            note_count: 1200,
            original_index: 3,
            track_id: 3,
            track_name: "Track 3".to_owned(),
            port: 0,
            channel: 3,
        },
        crate::state::root_state::RecoverTrackEntry {
            path: std::path::PathBuf::from("C:/cache/track7.lmdeltrack"),
            filename: "track7.lmdeltrack".to_owned(),
            deleted_at: "2026-11-02 09:30".to_owned(),
            note_count: 42,
            original_index: 7,
            track_id: 7,
            track_name: "Track 7".to_owned(),
            port: 0,
            channel: 7,
        },
    ];
    root.state.recover_track_dialog.selected_index = Some(1);

    let model = root.egui_chrome_model();
    let Some(DialogModel::RecoverTrack(panel)) = model.dialog else {
        panic!("找回音轨应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.title, "找回删除音轨");
    assert_eq!(panel.filename_header, "文件名");
    assert_eq!(panel.deleted_header, "删除时间");
    assert_eq!(panel.notes_header, "音符总数");
    assert_eq!(panel.empty, "暂无已删除的音轨缓存");
    assert_eq!(panel.entries.len(), 2);
    assert!(!panel.entries[0].selected);
    assert!(panel.entries[1].selected, "第 2 行应高亮");
    assert_eq!(panel.entries[0].note_count, "1200");

    // 选中载荷：path / original_index / track_id。
    let selected = panel.selected.expect("应带选中条目载荷");
    assert_eq!(selected.path, "C:/cache/track7.lmdeltrack");
    assert_eq!(selected.original_index, 7);
    assert_eq!(selected.track_id, 7);
    assert!(panel.can_act);

    // 未选中：can_act 为 false，selected 为空。
    root.state.recover_track_dialog.selected_index = None;
    let model = root.egui_chrome_model();
    let Some(DialogModel::RecoverTrack(panel)) = model.dialog else {
        panic!("找回音轨应由 egui 整窗接管");
    };
    assert!(!panel.can_act);
    assert!(panel.selected.is_none());
}

#[test]
fn recover_track_actions_map_to_messages() {
    let root = dialog_root();
    use crate::message::RecoverTrackAction as Prod;
    assert!(matches!(
        root.recover_track_action_message(&EguiAction::SelectionChanged(2)),
        Some(Message::RecoverTrack(Prod::SelectionChanged(2)))
    ));
    match root
        .recover_track_action_message(&EguiAction::Restore {
            path: "C:/cache/track3.lmdeltrack".to_owned(),
            original_index: 3,
        })
        .expect("恢复应映射为消息")
    {
        Message::RecoverTrack(Prod::Restore {
            path,
            original_index,
        }) => {
            assert_eq!(path.to_string_lossy(), "C:/cache/track3.lmdeltrack");
            assert_eq!(original_index, 3);
        }
        other => panic!("恢复映射错误：{other:?}"),
    }
    match root
        .recover_track_action_message(&EguiAction::PermanentlyDelete {
            path: "C:/cache/track3.lmdeltrack".to_owned(),
            track_id: 3,
        })
        .expect("永久删除应映射为消息")
    {
        Message::RecoverTrack(Prod::PermanentlyDelete { path, track_id }) => {
            assert_eq!(path.to_string_lossy(), "C:/cache/track3.lmdeltrack");
            assert_eq!(track_id, 3);
        }
        other => panic!("永久删除映射错误：{other:?}"),
    }
    assert!(matches!(
        root.recover_track_action_message(&EguiAction::CloseDialog),
        Some(Message::RecoverTrack(Prod::CloseDialog))
    ));
}
