//! 多状态角落的**数据构造**：把 `Root` 灌成有内容的样子
//!
//! 数据要贴近真实使用（长文件名、多条目、跨盘符路径），否则"控件被挤"这类问题
//! 在尺子上永远不会出现——空列表走的是另一套分支。

use std::path::PathBuf;

use crate::Root;
use crate::state::cloud_state::{CloudClipboard, CloudConnInfo, CloudEntryUi, PendingUpload};
use crate::state::root_state::RecoverTrackEntry;

/// 云浏览面板：两条连接 + 三个条目 + 提示 + 剪贴板（顶部工具行全按钮可见）
pub(super) fn cloud_browser_filled(root: &mut Root) {
    let cloud = &mut root.cloud;

    cloud.connections = vec![
        CloudConnInfo {
            id: "nas".to_string(),
            name: "我的 NAS".to_string(),
            protocol: "SFTP".to_string(),
            online: true,
        },
        CloudConnInfo {
            id: "backup".to_string(),
            name: "异地备份服务器".to_string(),
            protocol: "WebDAV".to_string(),
            online: false,
        },
    ];
    cloud.selected_id = Some("nas".to_string());
    cloud.current_path = "/作品/2026/钢琴协奏曲".to_string();
    cloud.entries = vec![
        CloudEntryUi {
            name: "工程备份.lumino".to_string(),
            path: "/作品/2026/钢琴协奏曲/工程备份.lumino".to_string(),
            is_dir: false,
            size: 42_949_672,
            modified: Some(1_760_000_000),
        },
        CloudEntryUi {
            name: "素材".to_string(),
            path: "/作品/2026/钢琴协奏曲/素材".to_string(),
            is_dir: true,
            size: 0,
            modified: None,
        },
        CloudEntryUi {
            name: "很长的文件名用于测试按钮与名称的挤压行为_最终版_修订二.lumino".to_string(),
            path: "/作品/2026/钢琴协奏曲/很长的文件名用于测试按钮与名称的挤压行为_最终版_修订二.lumino"
                .to_string(),
            is_dir: false,
            size: 1024,
            modified: Some(1_760_000_001),
        },
    ];
    cloud.notice = Some("已加载 3 个条目".to_string());
    cloud.clipboard = Some(CloudClipboard::new(
        "/作品/2026/钢琴协奏曲/素材",
        true,
        true,
    ));
}

/// 云浏览面板：保存模式（顶部多出「保存到此处」区块与素材上传标题）
pub(super) fn cloud_browser_save_mode(root: &mut Root) {
    cloud_browser_filled(root);

    let cloud = &mut root.cloud;
    cloud.save_mode = true;
    cloud.pending_upload = Some(PendingUpload {
        local_path: "D:\\素材\\钢琴音色.lmmaterial".to_string(),
        file_name: "钢琴音色.lmmaterial".to_string(),
    });
}

/// 找回音轨：三条缓存条目 + 选中首行
pub(super) fn recover_track_filled(root: &mut Root) {
    let state = &mut root.state.recover_track_dialog;

    state.entries = vec![
        RecoverTrackEntry {
            path: PathBuf::from("C:\\cache\\track_003.lmdeltrack"),
            filename: "track_003.lmdeltrack".to_string(),
            track_id: 3,
            track_name: "钢琴主旋律".to_string(),
            port: 1,
            channel: 1,
            note_count: 12_345,
            deleted_at: "2026-10-16 12:04:33".to_string(),
            original_index: 2,
        },
        RecoverTrackEntry {
            path: PathBuf::from("C:\\cache\\track_007.lmdeltrack"),
            filename: "track_007.lmdeltrack".to_string(),
            track_id: 7,
            track_name: "弦乐伴奏声部（第二版，含弱起与延音处理）".to_string(),
            port: 2,
            channel: 10,
            note_count: 8_921,
            deleted_at: "2026-10-15 21:38:02".to_string(),
            original_index: 6,
        },
        RecoverTrackEntry {
            path: PathBuf::from("C:\\cache\\track_012.lmdeltrack"),
            filename: "track_012.lmdeltrack".to_string(),
            track_id: 12,
            track_name: "鼓组".to_string(),
            port: 10,
            channel: 10,
            note_count: 421,
            deleted_at: "2026-10-14 09:12:47".to_string(),
            original_index: 11,
        },
    ];
    state.selected_index = Some(0);
}
