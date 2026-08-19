//! 本地状态同步事件（需要同步到其他用户）

#[derive(Debug, Clone)]
/// 本地状态同步事件（需要同步到其他用户）
pub enum Event {
    /// 本地笔记更新（需要同步到其他用户）
    LocalNoteAdded {
        /// 起始 tick
        tick: f32,
        /// 音符键位
        key: u16,
        /// 音符长度（tick）
        length: f32,
        /// 音符力度
        velocity: u8,
        /// 通道
        channel: u8,
        /// 音轨索引
        track_index: usize,
    },
    /// 本地音符移动（需要同步到其他用户）
    LocalNoteMoved {
        /// 起始 tick
        tick: f32,
        /// 音符键位
        key: u16,
        /// 音符长度（tick）
        length: f32,
        /// tick 偏移量
        tick_offset: f32,
        /// 键位偏移量
        key_offset: i16,
        /// 音轨索引
        track_index: usize,
    },
    /// 本地音符删除（需要同步到其他用户）
    LocalNoteDeleted {
        /// 起始 tick
        tick: f32,
        /// 音符键位
        key: u16,
        /// 音符长度（tick）
        length: f32,
        /// 音符力度
        velocity: u8,
        /// 通道
        channel: u8,
        /// 音轨索引
        track_index: usize,
    },
    /// 本地音轨添加（需要同步到其他用户）
    LocalTrackAdded {
        /// 音轨索引
        track_index: usize,
    },
}
