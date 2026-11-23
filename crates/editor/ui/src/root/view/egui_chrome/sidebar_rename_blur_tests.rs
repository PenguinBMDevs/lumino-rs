//! 左侧栏重命名失焦 → 生产消息：先落草稿，再收口。
//!
//! 失焦信号独立于单槽面板动作，因此「点击另一条音轨」时提交草稿与切换选中
//! 都能送达。这里锁死两条消息的顺序与语义，避免退化成「只确认、丢最后一次输入」
//! 或「只取消、没清会话」。

use super::*;
use lumino_ui_egui::chrome::{RenameBlur, SidebarRenameBlur};

fn root() -> Root {
    Root::new(&lumino_core::storage::config::UiConfig::default())
}

/// 进入重命名并写一份草稿（对齐 `RenameChanged` 之后的生产状态）。
fn start_renaming(root: &mut Root, track_id: usize, draft: &str) {
    use crate::sidebar::Event;
    root.sidebar.update(Event::TrackRenameStarted(track_id));
    root.sidebar
        .update(Event::TrackRenameChanged(track_id, draft.to_owned()));
}

fn name_of(root: &Root, track_id: usize) -> String {
    root.sidebar
        .tracks
        .iter()
        .find(|track| track.id == track_id)
        .map(|track| track.name.clone())
        .expect("应找到目标音轨")
}

/// 提交：用失焦时携带的最新草稿落库，并清空重命名会话。
#[test]
fn commit_signal_writes_the_latest_draft_then_confirms() {
    let mut root = root();
    let track_id = root.sidebar.tracks[1].id;
    start_renaming(&mut root, track_id, "旧草稿");

    let messages = root.sidebar_rename_blur_messages(SidebarRenameBlur {
        track_id,
        value: "新名字".to_owned(),
        outcome: RenameBlur::Commit,
    });
    assert_eq!(messages.len(), 2, "提交 = 先落草稿 + 再确认");
    for message in messages {
        root.update(message);
    }

    assert_eq!(
        name_of(&root, track_id),
        "新名字",
        "必须以失焦信号里的最新草稿为准（同帧最后一次击键可能挤掉了 RenameChanged）"
    );
    assert!(
        root.sidebar.renaming_track.is_none(),
        "确认后必须退出重命名"
    );
}

/// 取消：丢弃草稿、恢复原名，同样清空会话。
#[test]
fn cancel_signal_discards_the_draft() {
    let mut root = root();
    let track_id = root.sidebar.tracks[1].id;
    let original = name_of(&root, track_id);
    start_renaming(&mut root, track_id, "写了一半");

    let messages = root.sidebar_rename_blur_messages(SidebarRenameBlur {
        track_id,
        value: "写了一半".to_owned(),
        outcome: RenameBlur::Cancel,
    });
    assert_eq!(messages.len(), 1, "取消只需一条消息");
    for message in messages {
        root.update(message);
    }

    assert_eq!(name_of(&root, track_id), original, "Esc 不得改名字");
    assert!(
        root.sidebar.renaming_track.is_none(),
        "取消后必须退出重命名"
    );
}
