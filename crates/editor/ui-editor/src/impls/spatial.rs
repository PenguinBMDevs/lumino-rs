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
            note_index_key: Cell::new(None),
            query_cache: RefCell::new(Vec::new()),
            max_note_len: Cell::new(0),
            max_note_len_key: Cell::new(None),
        }
    }
}

impl Editor {
    /// 派生缓存的权威失效键：`(音符内容世代, 当前轨)`。
    ///
    /// 空间索引与最大音符长度都只由「当前轨的音符内容」决定，因此这两项就是
    /// 完备的失效依据（见 `SpatialIndexState` 文档）：
    /// - `track_notes_gen` 由数据层所有音符写入路径 bump（含协作/素材/整轨恢复）；
    /// - `current_track` 变化即索引的输入底座变化。
    pub(crate) fn spatial_cache_key(&self) -> (u64, usize) {
        (
            self.editor_state.data.track_notes_gen,
            self.editor_state.data.current_track,
        )
    }

    /// 空间索引是否可安全复用（已构建且键未过期）
    ///
    /// 供渲染路径判断能否走索引查询；不可复用时调用方回退窗口扫描。
    pub(crate) fn has_clean_spatial_index(&self) -> bool {
        self.spatial.note_index_key.get() == Some(self.spatial_cache_key())
            && self.spatial.note_index.borrow().is_some()
    }
}

impl SpatialIndexState {
    /// 显式失效两个派生缓存（清空失效键，下次访问必重算）
    ///
    /// 由 [`Editor::mark_notes_changed`] 与「清空编辑器」路径调用，兜住
    /// 「改了音符但没 bump `track_notes_gen`」的增量热路径。
    pub fn invalidate_spatial_caches(&self) {
        self.note_index_key.set(None);
        self.max_note_len_key.set(None);
    }
}

impl Editor {
    /// 当前轨最大音符长度（tick；惰性计算并缓存，音符世代/当前轨变化时重算）。
    ///
    /// 用途：框选增量窗口的 **lookback 精确上界**——查询区间左侧只需回看
    /// 「可能跨入区间的最长音符」，无需固定 1M tick（密集轨道下 1M tick 可覆盖
    /// 整轨，拖动增量会退化为每次全轨扫描）。
    pub(crate) fn current_track_max_note_len(&self) -> u32 {
        let key = self.spatial_cache_key();
        if self.spatial.max_note_len_key.get() == Some(key) {
            return self.spatial.max_note_len.get().max(1);
        }
        let mut max = 0u32;
        for n in self.editor_state.data.current_track_notes().iter() {
            max = max.max(n.end_tick.saturating_sub(n.start_tick));
        }
        let max = max.max(1);
        self.spatial.max_note_len.set(max);
        self.spatial.max_note_len_key.set(Some(key));
        max
    }
}

impl Editor {
    /// 标记音符数据已变化
    ///
    /// 职责有两层：
    /// 1. `notes_changed = true`（wgpu 重绘）；
    /// 2. **显式**失效空间索引与最长音符长度缓存（清空失效键 → 下次
    ///    [`Self::ensure_spatial_index`] / [`Self::current_track_max_note_len`] 必重算）。
    ///
    /// 第 2 层为什么不能只靠 [`Self::spatial_cache_key`]：拖动热路径的增量接口
    /// （`restore_current_track_sorted_incremental` / `record_update_ranges`，
    /// 见 `released.rs` 拉伸提交）**刻意不 bump** `track_notes_gen`——每帧全量同步
    /// 代价过高，它们靠 `mark_current_track_changed` + 本方法完成失效。
    /// 反之，只靠手工标记又会漏掉数据层写入（协作/素材/整轨恢复），
    /// 故两条通路都要有：本方法管「UI 层显式失效」，世代键管「数据层自动失效」。
    pub fn mark_notes_changed(&mut self) {
        self.notes_changed = true;
        self.spatial.invalidate_spatial_caches();
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
        let key = self.spatial_cache_key();
        if self.spatial.note_index_key.get() == Some(key) {
            return;
        }

        // 2026-08 单一权威源：从 document 当前轨切片收集 NoteRef（NoteEvent → NoteRef）
        let notes = self.editor_state.data.current_track_notes();
        if notes.len() <= SPATIAL_INDEX_BUILD_THRESHOLD {
            // 小数据量：直接标记为已更新（记录键），使用线性扫描路径
            self.spatial.note_index_key.set(Some(key));
            *self.spatial.note_index.borrow_mut() = None;
            return;
        }
        if notes.len() > SPATIAL_INDEX_MAX_BUILD {
            // 超大型工程：不建索引，查询走 ChunkedList 窗口扫描（见 hit_test.rs）
            self.spatial.note_index_key.set(Some(key));
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
        self.spatial.note_index_key.set(Some(key));

        tracing::debug!(
            "Editor: rebuild spatial index from ensure_spatial_index for {} notes",
            notes.len(),
        );
    }
}
