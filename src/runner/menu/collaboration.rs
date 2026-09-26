//! Runner 协作处理

mod cursor;
mod events;
mod room;
mod sync;

/// 本地音符快照（按值，操作者标识由信封承载，共用 6 字段）。
pub(crate) struct LocalNoteSnapshot {
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

/// 本地音符移动事件（按值 + 偏移，6 字段）。
pub(crate) struct LocalNoteMove {
    /// 移动前起始 tick（参照位置）
    pub tick: f32,
    /// 音高（参照位置）
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
