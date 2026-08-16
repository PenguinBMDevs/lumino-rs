//! 找回删除音轨对话框处理器
//!
//! 处理对话框内部 UI 交互（关闭、选中、恢复、永久删除）。
//! 实际磁盘 I/O（加载 / 销毁 `.lmdeltrack`、扫描缓存目录、刷新 sidebar.tracks）
//! 由 Runner 接管：
//! - `Restore` 通过 `state.dialog_result` 转交 Runner 后关闭对话框；
//! - `PermanentlyDelete` 通过 `state.dialog_result` 转交 Runner，但**保持对话框开启**
//!   （销毁缓存后 Runner 会重新扫描并刷新条目列表，支持连续操作多个缓存）；
//! - `CloseDialog` 仅关闭对话框，无副作用。
//!
//! 对话框打开前的条目列表填充由 Runner 在窗口就绪后调用
//! `Host::set_recover_track_dialog_entries` 完成。

use crate::host::DialogResult;
use crate::message::{Message, RecoverTrackAction};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_recover_track(
        &self,
        root: &mut Root,
        action: RecoverTrackAction,
    ) -> Option<Message> {
        match action {
            RecoverTrackAction::CloseDialog => {
                root.set_recover_track_dialog_open(false);
                root.state.dialog_result = Some(DialogResult::Cancel);
                tracing::debug!("找回删除音轨对话框关闭");
            }
            RecoverTrackAction::SelectionChanged(idx) => {
                let len = root.state.recover_track_dialog.entries.len();
                if idx < len {
                    root.state.recover_track_dialog.selected_index = Some(idx);
                } else {
                    tracing::warn!("RecoverTrack: 选中索引越界 idx={} len={}", idx, len);
                }
            }
            RecoverTrackAction::Restore {
                path,
                original_index,
            } => {
                tracing::info!(
                    "RecoverTrack: 请求恢复 path={:?} original_index={}",
                    path,
                    original_index
                );
                root.state.dialog_result = Some(DialogResult::RecoverTrackRestore {
                    path,
                    original_index,
                });
                root.set_recover_track_dialog_open(false);
            }
            RecoverTrackAction::PermanentlyDelete { path, track_id } => {
                tracing::info!(
                    "RecoverTrack: 请求永久删除 path={:?} track_id={}",
                    path,
                    track_id
                );
                root.state.dialog_result =
                    Some(DialogResult::RecoverTrackPermanentlyDelete { path, track_id });
                // 注意：此处不关闭对话框。Runner 处理完磁盘销毁后会重新扫描缓存目录
                // 并刷新条目列表，面板保持开启以便用户继续处理其他缓存。
            }
        }
        None
    }
}
