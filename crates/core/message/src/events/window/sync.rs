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
    /// 本地选择变更（需要同步到其他用户）
    ///
    /// 指纹（fingerprint）为选中音符的 `(track_index, tick, key, length)` 四元组，
    /// 以 `[f64; 4]` 表示以匹配 serde JSON 传输。
    LocalSelectionChanged {
        /// 选择是否激活（true=框选完成/编辑中，false=取消或编辑已提交）
        active: bool,
        /// 选择时间戳（ms，用于 first-writer-wins 冲突判定）
        timestamp: u64,
        /// 选中音符指纹列表
        fingerprints: Vec<[f64; 4]>,
    },
}
