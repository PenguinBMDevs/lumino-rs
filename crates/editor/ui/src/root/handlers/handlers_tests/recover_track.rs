//! 恢复已删除音轨对话框测试

use super::*;

/// 构造已打开的找回删除音轨对话框（含 2 个缓存条目，默认选中第一个）
fn setup_recover_track_dialog() -> Root {
    let mut root = create_root();
    root.set_recover_track_dialog_open(true);
    root.set_recover_track_dialog_entries(vec![
        crate::state::root_state::RecoverTrackEntry {
            path: "C:\\cache\\a.lmdeltrack".into(),
            filename: "a.lmdeltrack".into(),
            track_id: 1,
            track_name: "A".into(),
            port: 0,
            channel: 1,
            note_count: 10,
            deleted_at: "ts:1".into(),
            original_index: 0,
        },
        crate::state::root_state::RecoverTrackEntry {
            path: "C:\\cache\\b.lmdeltrack".into(),
            filename: "b.lmdeltrack".into(),
            track_id: 2,
            track_name: "B".into(),
            port: 0,
            channel: 2,
            note_count: 20,
            deleted_at: "ts:2".into(),
            original_index: 1,
        },
    ]);
    root
}

/// 永久删除：对话框必须保持开启（bug 回归），并产出结果转交 Runner 执行磁盘销毁
#[test]
fn test_recover_track_permanent_delete_keeps_dialog_open() {
    let mut root = setup_recover_track_dialog();
    let mut handler = DialogHandler::new();

    let result = handler.handle(
        &mut root,
        Message::RecoverTrack(lumino_message::RecoverTrackAction::PermanentlyDelete {
            path: "C:\\cache\\a.lmdeltrack".into(),
            track_id: 1,
        }),
    );

    assert!(result.is_none(), "处理器应消费消息");
    assert!(
        root.state.recover_track_dialog.is_open,
        "永久删除后对话框应保持开启，支持连续操作"
    );
    assert!(
        matches!(
            root.state.dialog_result,
            Some(crate::host::DialogResult::RecoverTrackPermanentlyDelete { track_id: 1, .. })
        ),
        "应产出 RecoverTrackPermanentlyDelete 结果转交 Runner"
    );
}

/// 恢复：对话框仍应关闭（行为不变，回归保护）
#[test]
fn test_recover_track_restore_closes_dialog() {
    let mut root = setup_recover_track_dialog();
    let mut handler = DialogHandler::new();

    let result = handler.handle(
        &mut root,
        Message::RecoverTrack(lumino_message::RecoverTrackAction::Restore {
            path: "C:\\cache\\a.lmdeltrack".into(),
            original_index: 0,
        }),
    );

    assert!(result.is_none(), "处理器应消费消息");
    assert!(
        !root.state.recover_track_dialog.is_open,
        "恢复后对话框应关闭"
    );
    assert!(
        matches!(
            root.state.dialog_result,
            Some(crate::host::DialogResult::RecoverTrackRestore { .. })
        ),
        "应产出 RecoverTrackRestore 结果转交 Runner"
    );
}
