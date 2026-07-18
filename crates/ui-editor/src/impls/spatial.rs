//! 空间索引状态相关方法
//!
//! 包含：SpatialIndexState 的 Default 实现，以及 Editor 上操作空间索引的方法

use crate::{Editor, SpatialIndexState};
use std::cell::{Cell, RefCell};

impl Default for SpatialIndexState {
    fn default() -> Self {
        Self {
            note_index: RefCell::new(None),
            note_index_dirty: Cell::new(false),
            query_cache: RefCell::new(Vec::new()),
        }
    }
}

impl Editor {
    /// 标记音符数据已变化
    ///
    /// 同时触发：
    /// - `notes_changed = true`（wgpu 重绘）
    /// - `note_index_dirty = true`（空间索引重建）
    pub fn mark_notes_changed(&mut self) {
        self.notes_changed = true;
        self.spatial.note_index_dirty.set(true);
    }

    /// 仅标记 ghost 位置变化（不触发空间索引重建）
    ///
    /// 用于 ghost 拖动方案：`DraggingSelection` 期间 `data.notes` 未变，
    /// 只是 `DragState.delta` 变了。此时只需 wgpu 重绘，不需要重建空间索引。
    ///
    /// **性能关键**：若误用 `mark_notes_changed`，3106 音符的批量拖动每帧
    /// 都会重建空间索引（~47ms/次 × 60fps ≈ 2.8s 卡顿）。
    pub fn mark_ghost_dirty(&mut self) {
        self.notes_changed = true;
    }
}
