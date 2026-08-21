//! 空间索引状态相关方法
//!
//! 包含：SpatialIndexState 的 Default 实现，以及 Editor 上操作空间索引的方法

use crate::{Editor, SpatialIndexState};
use std::cell::{Cell, RefCell};

/// 启用空间索引的音符数量阈值
///
/// 低于此阈值时，线性扫描/窗口扫描比构建二叉空间索引更快（避免小数据量下的建树开销）。
/// 高于此阈值时，空间索引的 O(log N + K) 查询优势才能抵消建树成本。
/// 2026-08-08 批量写入优化：100k 建树需 69ms(dev)/~15ms(release)，对 1k 批量插入
/// 的 4ms 合并而言占比过高；阈值从 50k 提升至 200k，使 100k 以内走
/// `ChunkedList::window_range` 零建树路径，批量插入后无重建卡顿。
pub(crate) const SPATIAL_INDEX_BUILD_THRESHOLD: usize = 200_000;

/// 空间索引**上限**：超过此规模不再构建空间索引
///
/// 2026-08-06 性能修复：1600W 音符工程每次编辑后重建空间索引需 O(N log N)
/// （collect NoteRef + sort + 递归建树 ≈ 2-4s + 数百 MB 临时内存），是「编辑
/// 中间插入 4s + 内存 2-3G」的主因。超大工程改由 `ChunkedList::window_range`
/// 块级二分窗口查询（O(log 块数 + 窗口长度)，免建索引）兜底，见
/// `visible_notes.rs::collect_via_window` 与 `note_ops/hit_test.rs`。
pub(crate) const SPATIAL_INDEX_MAX_BUILD: usize = 2_000_000;

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
    /// 用于 ghost 拖动方案：`DraggingSelection` 期间 document 未变，
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
    /// 小数据量时跳过建树，直接走线性扫描/窗口扫描，避免不必要的百毫秒级开销。
    ///
    /// **性能优化**：当 NoteStore 启用时走 `from_note_store` 直接消费 SoA 数据，
    /// 16M 音符场景下避免 ~80ms 的 Note 结构体 clone 开销。
    ///
    /// 2026-08-06 超大工程保护：音符量超过 [`SPATIAL_INDEX_MAX_BUILD`] 时不再
    /// 构建空间索引——改为清除索引（`None`），由 `collect_via_window` /
    /// `hit_test_note` 的 ChunkedList 窗口扫描兜底（O(log N + K) 同量级，
    /// 免 O(N log N) 全量重建 ≈ 2-4s）。保证该量级下查询不建树、不卡顿。
    pub(crate) fn ensure_spatial_index(&self) {
        if !self.spatial.note_index_dirty.get() {
            return;
        }

        // 2026-08 单一权威源：从 document 当前轨切片收集 NoteRef（NoteEvent → NoteRef）
        let notes = self.editor_state.data.current_track_notes();
        if notes.len() <= SPATIAL_INDEX_BUILD_THRESHOLD {
            // 小数据量：直接标记为已更新，使用线性扫描路径
            self.spatial.note_index_dirty.set(false);
            *self.spatial.note_index.borrow_mut() = None;
            return;
        }
        if notes.len() > SPATIAL_INDEX_MAX_BUILD {
            // 超大型工程：不建索引，查询走 ChunkedList 窗口扫描（见 hit_test.rs）
            self.spatial.note_index_dirty.set(false);
            *self.spatial.note_index.borrow_mut() = None;
            tracing::debug!(
                "Editor: 超大型工程（{} 音符）跳过空间索引构建，走窗口查询",
                notes.len(),
            );
            return;
        }

        // 从 &[NoteEvent] 收集 NoteRef 构建空间索引（NoteStore 冗余层已删除）
        let note_refs: Vec<lumino_note_core::NoteRef> = notes
            .iter()
            .enumerate()
            .map(|(i, n)| lumino_note_core::NoteRef {
                tick: n.start_tick as f32,
                key: n.key as u16,
                length: (n.end_tick - n.start_tick) as f32,
                index: i,
            })
            .collect();
        let new_index = crate::spatial_index::NoteSpatialIndex::from_note_refs(&note_refs);

        *self.spatial.note_index.borrow_mut() = Some(new_index);
        self.spatial.note_index_dirty.set(false);

        tracing::debug!(
            "Editor: rebuild spatial index from ensure_spatial_index for {} notes",
            notes.len(),
        );
    }
}
