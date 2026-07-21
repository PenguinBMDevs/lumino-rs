//! 空间索引状态相关方法
//!
//! 包含：SpatialIndexState 的 Default 实现，以及 Editor 上操作空间索引的方法

use crate::{Editor, SpatialIndexState};
use std::cell::{Cell, RefCell};

/// 启用空间索引的音符数量阈值
///
/// 低于此阈值时，线性扫描比构建二叉空间索引更快（避免小数据量下的建树开销）。
/// 高于此阈值时，空间索引的 O(log N + K) 查询优势才能抵消建树成本。
pub(crate) const SPATIAL_INDEX_BUILD_THRESHOLD: usize = 50_000;

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
        // 诊断日志：打印调用栈，追踪 notes_changed 被误触发的来源
        if std::env::var("LUMINO_TRACE_NOTES_CHANGED").is_ok() {
            let backtrace = std::backtrace::Backtrace::capture();
            tracing::info!("[onion-dirty] mark_notes_changed 调用栈:\n{}", backtrace);
        }
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

    /// 若空间索引已脏且音符数量达到阈值，立即重建
    ///
    /// 使用内部可变性，允许在 `&self` 的 hit-test/渲染路径中调用。
    /// 小数据量时跳过建树，直接走线性扫描，避免不必要的百毫秒级开销。
    ///
    /// **性能优化**：当 NoteStore 启用时走 `from_note_store` 直接消费 SoA 数据，
    /// 16M 音符场景下避免 ~80ms 的 Note 结构体 clone 开销。
    pub(crate) fn ensure_spatial_index(&self) {
        if !self.spatial.note_index_dirty.get() {
            return;
        }

        let notes = &self.editor_state.data.notes;
        if notes.len() <= SPATIAL_INDEX_BUILD_THRESHOLD {
            // 小数据量：直接标记为已更新，使用线性扫描路径
            self.spatial.note_index_dirty.set(false);
            *self.spatial.note_index.borrow_mut() = None;
            return;
        }

        // 热路径：NoteStore 启用时直接从 SoA 数据构建，跳过 im::Vector 中介
        let new_index = if self.editor_state.data.is_note_store_enabled() {
            crate::spatial_index::NoteSpatialIndex::from_note_store(
                &self.editor_state.data.note_store,
            )
        } else {
            // 冷路径：从 im::Vector 收集 NoteRef
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
            crate::spatial_index::NoteSpatialIndex::from_note_refs(&note_refs)
        };

        *self.spatial.note_index.borrow_mut() = Some(new_index);
        self.spatial.note_index_dirty.set(false);

        tracing::debug!(
            "Editor: rebuild spatial index from ensure_spatial_index for {} notes (note_store={})",
            notes.len(),
            self.editor_state.data.is_note_store_enabled(),
        );
    }
}
