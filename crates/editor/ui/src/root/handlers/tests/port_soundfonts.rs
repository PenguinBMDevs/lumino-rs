//! 端口音色库对话框处理器测试（REND-005 P4）。

use crate::host::DialogResult;
use crate::message::{Message, PortSoundfontsAction};
use crate::root::Root;
use crate::state::root_state::DialogType;
use lumino_midi_model::multi_port::PortSoundfontMap;

fn dialog_root() -> Root {
    let mut root = Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        DialogType::PortSoundfonts,
    );
    let mut map = PortSoundfontMap::default();
    map.set(1, vec!["D:/音色库/port1.sf2".to_string()]);
    root.set_port_soundfonts_dialog(&map, 1);
    root
}

/// 清除只修改草稿，不关闭窗口。
#[test]
fn clear_updates_draft_without_closing() {
    let mut root = dialog_root();
    assert_eq!(
        root.state.port_soundfonts_dialog.first_path(1),
        Some("D:/音色库/port1.sf2")
    );

    root.update(Message::PortSoundfonts(PortSoundfontsAction::Clear(1)));

    assert!(
        root.state.port_soundfonts_dialog.first_path(1).is_none(),
        "清除后该端口应回退全局"
    );
    assert!(
        root.state.dialog_result.is_none(),
        "清除不得关闭对话框或产生结果"
    );
}

/// 确定回传当前映射；取消回传 Cancel。
#[test]
fn confirm_returns_mapping_and_cancel_discards() {
    let mut root = dialog_root();
    root.update(Message::PortSoundfonts(PortSoundfontsAction::Confirm));
    match root.state.dialog_result.take() {
        Some(DialogResult::PortSoundfonts(map)) => {
            assert_eq!(
                map.get(1).map(|entry| entry.paths.clone()),
                Some(vec!["D:/音色库/port1.sf2".to_string()]),
                "确定必须回传草稿中的映射"
            );
        }
        other => panic!("应回传 PortSoundfonts，实际 {other:?}"),
    }

    let mut root = dialog_root();
    root.update(Message::PortSoundfonts(PortSoundfontsAction::Cancel));
    assert!(
        matches!(root.state.dialog_result, Some(DialogResult::Cancel)),
        "取消应回传 Cancel"
    );
}
