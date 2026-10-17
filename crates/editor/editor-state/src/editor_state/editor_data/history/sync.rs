//! 撤销/重做后的协作同步队列取出
//!
//! 由 `history.rs` 拆分而来：undo/redo 应用条目时填充的待广播队列，
//! 由 ui-editor 层在操作成功后 drain 并发射对应的 `LocalNote*` 事件。

use super::*;

impl EditorData {
    /// 设置协作同步开关（上层在协作连接建立/断开时调用）。
    ///
    /// 关闭时立即清空三个待广播队列，避免陈旧数据在下次开启时被误发射。
    pub fn set_collab_sync_enabled(&mut self, enabled: bool) {
        self.collab_sync_enabled = enabled;
        if !enabled {
            self.pending_collab_move_sync.clear();
            self.pending_collab_create_sync.clear();
            self.pending_collab_transform_sync.clear();
        }
    }

    /// 协作同步是否开启（默认 `false`：本地编辑跳过协作对账）。
    pub fn collab_sync_enabled(&self) -> bool {
        self.collab_sync_enabled
    }

    /// 记录一条「按值」协作变换待广播项（`is_add = true` 为新增，`false` 为删除），
    /// 与 [`Self::take_pending_collab_transform_sync`] 成对使用。
    ///
    /// 收敛 7 元组构造口径：形状重生成、拆分、合并等改变音符数量的提交统一走这里，
    /// 避免每个调用点各写一遍元组顺序。协作关闭时为空操作。
    #[allow(clippy::too_many_arguments)]
    pub fn push_collab_transform_sync(
        &mut self,
        is_add: bool,
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track: usize,
    ) {
        if !self.collab_sync_enabled {
            return;
        }
        self.pending_collab_transform_sync
            .push((is_add, tick, key, length, velocity, channel, track));
    }

    /// 取出并清空「撤销/重做后待广播给协作对端的音符移动」。
    ///
    /// 返回 `(参照 tick, 参照 key, tick 偏移, key 偏移, 音轨索引)` 列表（按值引用，
    /// 操作者标识由信封 `user_id + timestamp` 承载），
    /// 调用方（ui-editor 层 `editor_impl::history`）据此发射 `LocalNoteMoved`。
    pub fn take_pending_collab_move_sync(&mut self) -> Vec<(f32, u16, f32, i16, usize)> {
        std::mem::take(&mut self.pending_collab_move_sync)
    }

    /// 取出并清空「撤销/重做创建/删除后待广播给协作对端的音符创建事件」。
    ///
    /// 返回 `(tick, key, length, velocity, channel, 音轨索引, is_added)` 列表（按值），
    /// `is_added=true` 表示应发射 `LocalNoteAdded`（重做重新添加），
    /// `false` 表示应发射 `LocalNoteDeleted`（撤销删除）。
    /// 调用方（ui-editor 层 `editor_impl::history`）据此发射同步事件，
    /// 否则 B 端在 A 撤销创建后残留该音符（本次修复的协作缺陷）。
    pub fn take_pending_collab_create_sync(&mut self) -> Vec<super::CollabCreateSyncEntry> {
        std::mem::take(&mut self.pending_collab_create_sync)
    }

    /// 取出并清空「变换类操作（移调/翻转/变速/批量编辑）后待广播给协作对端的音符增删」。
    ///
    /// 返回 `(is_add, tick, key, length, velocity, channel, 音轨索引)` 列表（按值），
    /// `is_add=true` → 发射 `LocalNoteAdded`，`false` → 发射 `LocalNoteDeleted`。
    /// 调用方（ui-editor 层 `editor_impl::history`）据此发射同步事件，使 B 端在 A 执行
    /// 变换操作或对其撤销/重做后保持一致。
    pub fn take_pending_collab_transform_sync(&mut self) -> Vec<super::CollabTransformSyncEntry> {
        std::mem::take(&mut self.pending_collab_transform_sync)
    }
}
