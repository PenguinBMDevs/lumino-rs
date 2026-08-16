//! 找回删除音轨对话框状态
//!
//! 用于显示当前缓存目录下的 `.lmdeltrack` 文件列表，支持永久删除与恢复两种操作。
//! 实际磁盘 I/O 由 Runner 完成，本状态仅承载 UI 显示数据。

use std::path::PathBuf;

/// 已删除音轨缓存条目（UI 展示用）
///
/// 字段从 `lumino_project::deleted_track::DeletedTrackEntry` 提炼而来，
/// 避免在 ui-core 中引入 lumino_project 依赖。
#[derive(Debug, Clone)]
pub struct RecoverTrackEntry {
    /// 缓存文件路径（恢复 / 永久删除时回传给 Runner）
    pub path: PathBuf,
    /// 缓存文件名（不含路径，含扩展名）
    pub filename: String,
    /// 音轨编号（删除时的原始 ID）
    pub track_id: u16,
    /// 音轨名称
    pub track_name: String,
    /// MIDI 端口号
    pub port: u8,
    /// MIDI 通道号
    pub channel: u8,
    /// 音符总数
    pub note_count: u64,
    /// 删除时间（ISO 8601 格式字符串）
    pub deleted_at: String,
    /// 在 sidebar.tracks 中的原始位置索引
    pub original_index: usize,
}

/// 找回删除音轨对话框状态
#[derive(Debug, Clone, Default)]
pub struct RecoverTrackDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 缓存条目列表（按删除时间倒序）
    pub entries: Vec<RecoverTrackEntry>,
    /// 当前选中的条目索引（None 表示未选中）
    pub selected_index: Option<usize>,
}
