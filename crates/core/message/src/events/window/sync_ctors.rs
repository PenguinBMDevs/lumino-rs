use super::*;

impl Event {
    // ── 同步构造函数（直接构造 sync::Event） ──

    /// 构造本地音符添加同步事件（按值引用，操作者标识由信封承载）。
    pub fn local_note_added(
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteAdded {
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        })
    }
    /// 构造本地音符移动同步事件（按值引用 + 偏移）。
    pub fn local_note_moved(
        tick: f32,
        key: u16,
        length: f32,
        tick_offset: f32,
        key_offset: i16,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteMoved {
            tick,
            key,
            length,
            tick_offset,
            key_offset,
            track_index,
        })
    }
    /// 构造本地音符删除同步事件（按值引用）。
    pub fn local_note_deleted(
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteDeleted {
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        })
    }
    /// 构造本地音轨添加同步事件
    pub fn local_track_added(track_index: usize) -> Self {
        Self::Sync(sync::Event::LocalTrackAdded { track_index })
    }
    /// 构造本地选择变更同步事件
    pub fn local_selection_changed(
        active: bool,
        timestamp: u64,
        fingerprints: Vec<[f64; 4]>,
    ) -> Self {
        Self::Sync(sync::Event::LocalSelectionChanged {
            active,
            timestamp,
            fingerprints,
        })
    }
    /// 构造本地批量音符添加同步事件（100K 粘贴，按值）
    pub fn local_notes_added_batch(notes: Vec<(f32, u16, f32, u8, u8, usize)>) -> Self {
        Self::Sync(sync::Event::LocalNotesAddedBatch { notes })
    }
}
