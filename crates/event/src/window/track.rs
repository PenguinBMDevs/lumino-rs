//! 音轨删除 / 恢复事件
//!
//! 当用户在音轨列表中删除音轨时，UI 立即释放入口（sidebar.tracks），
//! 同时通过 `DeleteTrack` 事件把音轨元数据 + 音符列表传给 Runner，
//! 由 Runner 调用 `lumino_project::deleted_track::save_deleted_track`
//! 写入 `.lmdeltrack` 缓存文件。
//!
//! 用户在"找回删除音轨"对话框中点击"恢复"或"永久删除"时，
//! Runner 完成磁盘 I/O 后通过 `TrackRestored` / `TrackPermanentlyDeleted`
//! 通知 UI 更新 sidebar.tracks / reserved_track_ids。
//!
//! 所有数据使用基础类型字段，避免 event crate 引入 lumino-project 依赖。

use std::path::PathBuf;

/// 单条已删除音符（NoteOn 事件，足够恢复音轨）
#[derive(Debug, Clone)]
pub struct TrackDeletionNote {
    /// 起始 tick
    pub start_tick: u32,
    /// 结束 tick（包含，便于恢复时直接使用）
    pub end_tick: u32,
    /// 音高（0-127）
    pub key: u8,
    /// 力度（0-127）
    pub velocity: u8,
    /// 通道号（0-15）
    pub channel: u8,
    /// 端口号（0-15）
    pub port: u8,
}

/// 待写入 `.lmdeltrack` 的音轨数据（由 UI 从 editor_state 提取后传给 Runner）
#[derive(Debug, Clone)]
pub struct TrackDeletionPayload {
    /// 音轨编号（删除时的原始 ID）
    pub track_id: u16,
    /// 音轨名称
    pub track_name: String,
    /// MIDI 端口号
    pub port: u8,
    /// MIDI 通道号
    pub channel: u8,
    /// 是否为鼓音轨
    pub is_drum: bool,
    /// 此音轨最后一个事件的 tick（用于恢复时设置 max_tick）
    pub max_tick: u32,
    /// 在 sidebar.tracks 中的原始位置索引（恢复时优先放回此位置）
    pub original_index: usize,
    /// 音符列表（已按 start_tick 排序）
    pub notes: Vec<TrackDeletionNote>,
}

/// "找回删除音轨"对话框条目（由 Runner 扫描缓存目录后传给 UI）
#[derive(Debug, Clone)]
pub struct RecoverTrackEntryPayload {
    /// 缓存文件路径
    pub path: PathBuf,
    /// 缓存文件名（含扩展名）
    pub filename: String,
    /// 音轨编号
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

/// 音轨删除 / 恢复事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 通知 Runner：sidebar 已删除入口，请将音轨数据写入 `.lmdeltrack` 缓存
    DeleteTrack(TrackDeletionPayload),
    /// 通知 Runner：从 `.lmdeltrack` 恢复音轨（用户点击"恢复"按钮）
    RestoreTrack {
        /// 缓存文件路径
        path: PathBuf,
        /// 删除时记录的原始 sidebar.tracks 索引
        original_index: usize,
    },
    /// 通知 Runner：永久销毁 `.lmdeltrack` 缓存（用户点击"永久删除"按钮）
    PermanentlyDeleteTrack {
        /// 缓存文件路径
        path: PathBuf,
        /// 删除时记录的音轨 ID（用于释放 reserved_track_ids 占用）
        track_id: u16,
    },
    /// 通知 UI：Runner 扫描缓存目录完成，填充对话框条目列表
    RecoverTrackDialogScanned(Vec<RecoverTrackEntryPayload>),
    /// 通知 UI：Runner 已加载 `.lmdeltrack` 并恢复音轨，请把音轨重新加入 sidebar
    TrackRestored(TrackDeletionPayload),
    /// 通知 UI：Runner 已永久销毁 `.lmdeltrack`，请释放 reserved_track_id
    TrackPermanentlyDeleted {
        /// 已销毁的音轨 ID
        track_id: u16,
    },
}
