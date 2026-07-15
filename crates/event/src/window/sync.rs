//! 本地状态同步事件（需要同步到其他用户）

#[derive(Debug, Clone)]
pub enum Event {
    /// 本地笔记更新（需要同步到其他用户）
    LocalNoteAdded {
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    },
    /// 本地音符移动（需要同步到其他用户）
    LocalNoteMoved {
        tick: f32,
        key: u16,
        length: f32,
        tick_offset: f32,
        key_offset: i16,
        track_index: usize,
    },
    /// 本地音符删除（需要同步到其他用户）
    LocalNoteDeleted {
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    },
    /// 本地音轨添加（需要同步到其他用户）
    LocalTrackAdded { track_index: usize },
}
