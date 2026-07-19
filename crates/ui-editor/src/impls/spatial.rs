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
    /// 只是 `DragState.delta` 变了。此时只需 wgpu 重绘 + Canvas 缓存失效，
    /// 不需要重建空间索引。
    ///
    /// **性能关键**：若误用 `mark_notes_changed`，3106 音符的批量拖动每帧
    /// 都会重建空间索引（~47ms/次 × 60fps ≈ 2.8s 卡顿）。
    pub fn mark_ghost_dirty(&mut self) {
        self.notes_changed = true;
        self.grid_cache.clear();
    }

    /// 若空间索引已脏，立即重建
    ///
    /// 使用内部可变性，允许在 `&self` 的 hit-test 路径中调用。
    /// 避免 `hit_test_note` 在异步提交后使用旧空间索引命中原位置。
    pub(crate) fn ensure_spatial_index(&self) {
        if !self.spatial.note_index_dirty.get() {
            return;
        }

        let notes = &self.editor_state.data.notes;
        let note_refs: Vec<lumino_core::NoteRef> = notes
            .iter()
            .enumerate()
            .map(|(i, n)| lumino_core::NoteRef {
                tick: n.tick,
                key: n.key,
                length: n.length,
                index: i,
            })
            .collect();

        *self.spatial.note_index.borrow_mut() = Some(
            crate::spatial_index::NoteSpatialIndex::from_note_refs(&note_refs),
        );
        self.spatial.note_index_dirty.set(false);

        tracing::debug!(
            "Editor: rebuild spatial index from ensure_spatial_index for {} notes",
            notes.len()
        );
    }
}
