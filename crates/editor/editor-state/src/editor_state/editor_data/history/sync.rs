//! 撤销/重做后的协作同步队列取出
//!
//! 由 `history.rs` 拆分而来：undo/redo 应用条目时填充的待广播队列，
//! 由 ui-editor 层在操作成功后 drain 并发射对应的 `LocalNote*` 事件。

use super::*;

impl EditorData {
    /// 取出并清空「撤销/重做后待广播给协作对端的音符移动」。
    ///
    /// 返回 `(音符全局唯一 ID, 参照 tick, 参照 key, tick 偏移, key 偏移, 音轨索引)` 列表，
    /// 调用方（ui-editor 层 `editor_impl::history`）据此发射 `LocalNoteMoved`。
    pub fn take_pending_collab_move_sync(&mut self) -> Vec<(u64, f32, u16, f32, i16, usize)> {
        std::mem::take(&mut self.pending_collab_move_sync)
    }

    /// 取出并清空「撤销/重做创建/删除后待广播给协作对端的音符创建事件」。
    ///
    /// 返回 `(音符全局唯一 ID, tick, key, length, velocity, channel, 音轨索引, is_added)` 列表，
    /// `is_added=true` 表示应发射 `LocalNoteAdded`（重做重新添加），
    /// `false` 表示应发射 `LocalNoteDeleted`（撤销删除）。
    /// 调用方（ui-editor 层 `editor_impl::history`）据此发射同步事件，
    /// 否则 B 端在 A 撤销创建后残留该音符（本次修复的协作缺陷）。
    pub fn take_pending_collab_create_sync(&mut self) -> Vec<super::CollabCreateSyncEntry> {
        std::mem::take(&mut self.pending_collab_create_sync)
    }

    /// 取出并清空「变换类操作（移调/翻转/变速/批量编辑）后待广播给协作对端的音符增删」。
    ///
    /// 返回 `(is_add, 音符全局唯一 ID, tick, key, length, velocity, channel, 音轨索引)` 列表，
    /// `is_add=true` → 发射 `LocalNoteAdded`，`false` → 发射 `LocalNoteDeleted`。
    /// 调用方（ui-editor 层 `editor_impl::history`）据此发射同步事件，使 B 端在 A 执行
    /// 变换操作或对其撤销/重做后保持一致。
    pub fn take_pending_collab_transform_sync(&mut self) -> Vec<super::CollabTransformSyncEntry> {
        std::mem::take(&mut self.pending_collab_transform_sync)
    }
}
