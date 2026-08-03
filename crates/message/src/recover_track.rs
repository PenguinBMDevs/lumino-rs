//! 找回删除音轨对话框动作
//!
//! 用于"找回删除音轨"对话框内的 UI 交互。实际的磁盘 I/O（读取缓存文件、
//! 写入 sidebar.tracks、释放 reserved_track_ids）由 Runner 在收到
//! `DialogResult::RecoverTrack*` 后完成。

use std::path::PathBuf;

/// 找回删除音轨对话框动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverTrackAction {
    /// 关闭对话框（取消 / 关闭按钮）
    CloseDialog,
    /// 选中条目变化
    SelectionChanged(usize),
    /// 恢复选中条目到原位置
    ///
    /// 由对话框视图在用户点击"恢复"按钮时发出。Root 接收后转换为
    /// `DialogResult::RecoverTrackRestore` 并关闭对话框。
    Restore {
        /// 缓存文件路径
        path: PathBuf,
        /// 删除时记录的原始 sidebar.tracks 索引
        original_index: usize,
    },
    /// 永久销毁选中条目
    ///
    /// 由对话框视图在用户点击"永久删除"按钮时发出。Root 接收后转换为
    /// `DialogResult::RecoverTrackPermanentlyDelete` 并关闭对话框。
    PermanentlyDelete {
        /// 缓存文件路径
        path: PathBuf,
        /// 删除时记录的音轨 ID（用于释放 reserved_track_ids 占用）
        track_id: u16,
    },
}
