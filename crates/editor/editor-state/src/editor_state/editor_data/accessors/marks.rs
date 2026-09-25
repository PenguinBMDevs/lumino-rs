//! 音符数据变化标记（脏标记 / 版本号 / 未保存状态）
//!
//! 由 `accessors.rs` 拆分而来。

use super::*;

impl EditorData {
    /// 标记音符数据已变化（递增版本号）
    ///
    /// 所有直接修改音符数据的地方都必须在操作后调用此方法，
    /// 否则 NoteWorker 快照缓存无法感知数据变化。
    ///
    /// 变化来源未知或影响全部音轨（`onion_dirty_tracks = None`），
    /// 洋葱皮会保守执行全量重建。调用方若能明确受影响音轨，
    /// 请使用 [`Self::mark_track_notes_changed_for`] 以获得增量豁免。
    #[inline]
    pub fn mark_track_notes_changed(&mut self) {
        self.mark_track_notes_changed_for(None);
    }

    /// 标记工程存在未保存的更改（供关闭/打开/退出前的保存确认对话框使用）
    ///
    /// 所有直接修改音符/工程数据的入口都应调用本方法（见 `mark_track_notes_changed_for`、
    /// `insert_note`/`remove_note`/`update_note`/`replace_track_notes`、`undo`/`redo`、
    /// `set_tempo_points`/`set_time_signatures`）。保存完成 / 加载 / 新建 / 关闭工程
    /// 时由 Host 显式复位（`mark_project_clean`），因此此处是单向置位、无需清零。
    #[inline]
    pub fn mark_modified(&mut self) {
        self.modified = true;
    }

    /// 标记音符数据已变化，并记录明确受影响的音轨集合
    ///
    /// `tracks` 为本次操作实际修改的音轨 id 集合：
    /// - `Some({current_track})`：当前音轨变化。统一全量渲染下 GPU 持有所有轨
    ///   数据，当前音轨可通过 `TrackDelta` 增量同步，不再 fallback 到全量重建。
    /// - `Some({other_track})`：其他音轨变化，不影响主音轨增量路径；洋葱皮层
    ///   通过 `Delta` 同步。
    /// - `None`：未知或影响全部音轨（保守语义，同 [`Self::mark_track_notes_changed`]），
    ///   必须全量兜底。
    #[inline]
    pub fn mark_track_notes_changed_for(&mut self, tracks: Option<HashSet<usize>>) {
        self.onion_dirty_tracks = tracks;
        self.track_notes_gen = self.track_notes_gen.wrapping_add(1);
        // 未知来源才需要全量兜底；已知音轨变化（含当前轨）走 Delta 增量。
        if self.onion_dirty_tracks.is_none() {
            self.note_delta_dirty = true;
        }
        // 任意音符数据变化都意味着未保存更改（供保存确认对话框使用）
        self.modified = true;
    }

    /// 标记当前音轨的音符已变化（热路径专用）
    ///
    /// 编辑操作绝大多数作用于当前音轨（拖动音符、增删改），而洋葱皮
    /// 不显示当前音轨——精确记录音轨 id 后，洋葱皮可豁免全量重建上传，
    /// 避免「拖动主音轨音符 → 每帧全量重传其他所有音轨」的冗余。
    #[inline]
    pub fn mark_current_track_changed(&mut self) {
        let current_track = self.current_track;
        self.mark_track_notes_changed_for(Some(HashSet::from([current_track])));
    }

    /// 标记当前轨发生**结构性**变化（批量插入/删除、undo 整轨替换），且无段内增量事件。
    ///
    /// 渲染层据此以单轨 `TrackDelta` 重建当前轨段（见
    /// [`crate::editor_state::EditorData::main_track_struct_dirty`]），
    /// 替代 `note_delta_dirty` 的全量会话重建（大工程下 ~5x 成本差）。
    /// 调用方须同时保证 `note_delta_events` 已清空（本方法负责清空）。
    #[inline]
    pub fn mark_main_track_struct_changed(&mut self) {
        self.note_delta_events.clear();
        self.main_track_struct_dirty = true;
    }
}
