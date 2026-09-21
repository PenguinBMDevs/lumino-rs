//! Runner 协作处理

mod cursor;
mod events;
mod room;
mod sync;

/// 本地音符快照（`handle_local_note_added/deleted` 共用 7 字段，结构体化满足 `too_many_arguments`）。
pub(crate) struct LocalNoteSnapshot {
    /// 音符全局唯一 ID
    pub id: u64,
    /// 起始 tick
    pub tick: f32,
    /// 音高
    pub key: u16,
    /// 长度
    pub length: f32,
    /// 力度
    pub velocity: u8,
    /// 通道
    pub channel: u8,
    /// 音轨索引
    pub track_index: usize,
}

/// 本地音符移动事件（7 字段，结构体化满足 `too_many_arguments`）。
pub(crate) struct LocalNoteMove {
    /// 音符全局唯一 ID
    pub id: u64,
    /// 移动后起始 tick
    pub tick: f32,
    /// 音高
    pub key: u16,
    /// 长度
    pub length: f32,
    /// tick 偏移
    pub tick_offset: f32,
    /// key 偏移
    pub key_offset: i16,
    /// 音轨索引
    pub track_index: usize,
}
