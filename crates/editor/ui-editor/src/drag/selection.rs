//! 框选（Selection）相关逻辑：增量更新、全量重建、矩形差集
//!
//! 从 `drag.rs` 抽出，控制文件行数并保持单一职责。

use crate::{EditState, Editor};
use lumino_editor_state::SelectionSet;

/// 框选命中谓词 —— **全项目框选口径的唯一来源**（索引路径与无索引窗口兜底共用）。
///
/// 语义：音符与选框**重叠**即命中；**仅「边界相触」不算命中**。
/// - tick 轴**半开** `[min_tick, max_tick)`：起点恰等于框右边界、终点恰等于框左边界的
///   音符被排除 —— 这是低精度模式下「框缘贴边音符被误选」的正解；
/// - key 轴**闭**区间 `[min_key, max_key]`：`key` 是离散格，「整格覆盖」语义，
///   且增量差集（`rect_subtract`）的 key 代数依赖闭区间。
///
/// ⚠️ 索引路径必须使用 `NoteSpatialIndex::update_query_marquee`（同口径半开），
/// **不得**使用 `update_query`（tick 轴闭区间，供视口裁剪与播放活跃判定使用）。
/// 两条路径口径不一致会造成「工程规模不同 → 框选结果不同」的口径分裂。
#[inline]
fn marquee_hits(
    note_tick: f32,
    note_end: f32,
    key: u16,
    min_tick: f32,
    max_tick: f32,
    min_key: u16,
    max_key: u16,
) -> bool {
    note_end > min_tick && note_tick < max_tick && key >= min_key && key <= max_key
}

impl Editor {
    /// 增量更新框选：缓存旧边界 → rect_subtract → 仅查 delta 区域（非 O(N) 全量）
    pub(crate) fn update_selection(&mut self) {
        crate::puffin_profiler::update_selection();
        puffin::profile_scope!("diag::update_selection_total");

        // 非 Selecting 状态 → 清除缓存并返回
        if !matches!(
            self.editor_state.interaction.edit_state,
            EditState::Selecting { .. }
        ) {
            self.cached_selection_bounds.set(None);
            return;
        }

        let (start_tick, start_key, current_tick, current_key) =
            match &self.editor_state.interaction.edit_state {
                EditState::Selecting {
                    start_tick,
                    start_key,
                    current_tick,
                    current_key,
                    ..
                } => (*start_tick, *start_key, *current_tick, *current_key),
                _ => {
                    tracing::error!("drag 选择更新：交互状态非 Selecting，跳过（防御性返回）");
                    return;
                }
            };

        let new_min_t = start_tick.min(current_tick);
        let new_max_t = start_tick.max(current_tick);
        let new_min_k = start_key.min(current_key);
        let new_max_k = start_key.max(current_key);
        let new_bounds = (new_min_t, new_max_t, new_min_k, new_max_k);

        // 无缓存 → 全量重建（首帧）
        let Some(old_bounds) = self.cached_selection_bounds.get() else {
            self.cached_selection_bounds.set(Some(new_bounds));
            return rebuild_full_selection(self, new_min_t, new_max_t, new_min_k, new_max_k);
        };

        // 缓存命中 → 跳过或保护性重建
        if old_bounds == new_bounds {
            puffin::profile_scope!("diag::selection_cache_hit");
            if !self.has_selection() {
                self.cached_selection_bounds.set(Some(new_bounds));
                rebuild_full_selection(self, new_min_t, new_max_t, new_min_k, new_max_k);
            }
            return;
        }

        // 安全保护：无选中音符但缓存在 → 全量重建
        if !self.has_selection() {
            self.cached_selection_bounds.set(Some(new_bounds));
            return rebuild_full_selection(self, new_min_t, new_max_t, new_min_k, new_max_k);
        }

        self.cached_selection_bounds.set(Some(new_bounds));
        apply_selection_delta(self, old_bounds, new_min_t, new_max_t, new_min_k, new_max_k);
    }

    /// 全量重建 selected_notes（首帧 / fallback）
    ///
    /// 2026-09 性能修复（超大工程框选）：旧实现逐索引 `selection_insert`
    /// （每次随机 `get_note_view` 二分 + 边界更新）在 19.2M 轨道下 ~130ms/帧。
    /// 现在改为**单遍顺序扫描**：查询得索引集合（位图，O(1) 命中），再沿轨道
    /// 顺序扫描一次得出边界，全程顺序读。
    fn rebuild_selected_notes(&mut self, min_tick: f32, max_tick: f32, min_key: u16, max_key: u16) {
        self.selection_clear();
        self.ensure_spatial_index();
        // lookback = 最大音符长度（精确上界；固定 1M tick 在密集轨道下覆盖整轨）
        let lookback = self.current_track_max_note_len();

        let set = {
            let notes = self.editor_state.data.current_track_notes();
            if let Some(index) = self.spatial.note_index.borrow().as_ref() {
                let mut cache = self.spatial.query_cache.borrow_mut();
                cache.clear();
                // 框选口径：tick 轴半开（`update_query` 是视口/播放的闭区间原语，不可用）
                index.update_query_marquee(min_tick, max_tick, min_key, max_key, &mut cache);
                let mut set = SelectionSet::default();
                set.extend(cache.iter().copied());
                set
            } else {
                // 超大型工程（无空间索引）→ 窗口扫描：块级二分框出 tick 范围
                // （含 lookback 跨入），一次遍历同时收集索引。
                //
                // ⚠️ `window_range` 的 end 参数是**排他**上界，`end_u32 + 1` 使窗口
                // 成为查询区间的**超集**，由 `marquee_hits` 精确收口。
                // 不要为了「对齐半开」把这里改成 `end_u32` —— 那会让窗口变子集，
                // 直接漏音符。
                puffin::profile_scope!("diag::selection_window_scan");
                let start_u32 = min_tick.max(0.0) as u32;
                let end_u32 = max_tick.max(0.0) as u32;
                let (lo, hi) = notes.window_range(start_u32, end_u32 + 1, lookback);
                let mut set = SelectionSet::default();
                for (i, note) in notes.iter_window(lo, hi) {
                    if marquee_hits(
                        note.start_tick as f32,
                        note.end_tick as f32,
                        note.key as u16,
                        min_tick,
                        max_tick,
                        min_key,
                        max_key,
                    ) {
                        set.insert(i);
                    }
                }
                set
            }
        };

        // 边界：单遍顺序扫描轨道 + 位图命中（O(轨道) 顺序读，免逐索引随机 get）
        let mut bounds = None;
        if !set.is_empty() {
            let mut min_t = f32::INFINITY;
            let mut max_te = f32::NEG_INFINITY;
            let mut max_k = u16::MIN;
            let mut min_k = u16::MAX;
            for (i, n) in self
                .editor_state
                .data
                .current_track_notes()
                .iter()
                .enumerate()
            {
                if !set.contains(&i) {
                    continue;
                }
                let tick = n.start_tick as f32;
                let length = (n.end_tick - n.start_tick) as f32;
                min_t = min_t.min(tick);
                max_te = max_te.max(tick + length);
                max_k = max_k.max(n.key as u16);
                min_k = min_k.min(n.key as u16);
            }
            bounds = Some((min_t, max_te, max_k, min_k));
        }

        self.editor_state.interaction.selected_notes = set;
        self.selected_bounds.set(bounds);
    }
}

/// 全量重建封装（用于从 `update_selection` 的 guard 块中调用）
fn rebuild_full_selection(
    editor: &mut Editor,
    min_tick: f32,
    max_tick: f32,
    min_key: u16,
    max_key: u16,
) {
    puffin::profile_scope!("diag::selection_full_rebuild");
    editor.rebuild_selected_notes(min_tick, max_tick, min_key, max_key);
}

/// 查询矩形内的音符索引（追加到 `out`）。
///
/// 优先空间索引（半开口径）；无索引（超大工程）走 `ChunkedList` 窗口扫描兜底——
/// **增量拖动仍保持增量**：旧实现在无索引时直接退化为整框重建（19.2M 轨道 ~130ms/帧）。
/// lookback 取当前轨最大音符长度（跨入查询区间的精确上界）。
///
/// 口径见 [`marquee_hits`]：tick 轴半开、key 轴闭。索引路径必须与窗口兜底一致。
fn query_rect_indices(
    editor: &Editor,
    t_min: f32,
    t_max: f32,
    k_min: u16,
    k_max: u16,
    out: &mut Vec<usize>,
) {
    editor.ensure_spatial_index();
    if let Some(index) = editor.spatial.note_index.borrow().as_ref() {
        let mut cache = editor.spatial.query_cache.borrow_mut();
        cache.clear();
        index.update_query_marquee(t_min, t_max, k_min, k_max, &mut cache);
        out.extend(cache.iter().copied());
        return;
    }
    let track = editor.editor_state.data.current_track_notes();
    let lookback = editor.current_track_max_note_len();
    // 同 `rebuild_selected_notes`：`end_u32 + 1` 让窗口保持超集，由谓词精确收口
    let (lo, hi) = track.window_range(t_min.max(0.0) as u32, t_max.max(0.0) as u32 + 1, lookback);
    for (i, n) in track.iter_window(lo, hi) {
        if marquee_hits(
            n.start_tick as f32,
            n.end_tick as f32,
            n.key as u16,
            t_min,
            t_max,
            k_min,
            k_max,
        ) {
            out.push(i);
        }
    }
}

/// 计算增量 delta 并应用移除/新增的区域
fn apply_selection_delta(
    editor: &mut Editor,
    old_bounds: (f32, f32, u16, u16),
    new_min_t: f32,
    new_max_t: f32,
    new_min_k: u16,
    new_max_k: u16,
) {
    let (old_min_t, old_max_t, old_min_k, old_max_k) = old_bounds;
    let mut delta_rects: Vec<(f32, f32, u16, u16)> = Vec::with_capacity(8);

    // 减少区域（old 里有，new 里没有）
    rect_subtract(
        old_min_t,
        old_max_t,
        old_min_k,
        old_max_k,
        new_min_t,
        new_max_t,
        new_min_k,
        new_max_k,
        &mut delta_rects,
    );

    let mut remove_list: Vec<usize> = Vec::new();
    for &(t_min, t_max, k_min, k_max) in &delta_rects {
        query_rect_indices(editor, t_min, t_max, k_min, k_max, &mut remove_list);
    }
    let removed = remove_list.len();
    for i in remove_list {
        editor.selection_remove(&i);
    }
    delta_rects.clear();

    // 新增区域（new 里有，old 里没有）
    rect_subtract(
        new_min_t,
        new_max_t,
        new_min_k,
        new_max_k,
        old_min_t,
        old_max_t,
        old_min_k,
        old_max_k,
        &mut delta_rects,
    );

    let mut added_indices: Vec<usize> = Vec::new();
    for &(t_min, t_max, k_min, k_max) in &delta_rects {
        query_rect_indices(editor, t_min, t_max, k_min, k_max, &mut added_indices);
    }
    let added = added_indices.len();
    for i in added_indices {
        editor.selection_insert(i);
    }

    puffin::profile_scope!("diag::selection_delta");
    if removed + added > 100 {
        tracing::debug!(
            "diag::selection_delta — 移除了 {} 个, 新增了 {} 个",
            removed,
            added
        );
    }
}

/// 矩形差集：outer - inner = outer 中不在 inner 内的部分。
/// 返回最多 4 个非重叠矩形的列表。
///
/// 算法：先 clamp inner 到 outer 边界，然后从上/下/左/右四个方向切 strip。
/// 上/下 strip 跨越 outer 全宽，左/右 strip 夹在 inner 的垂直范围内 → 不重复。
#[allow(clippy::too_many_arguments)]
fn rect_subtract(
    outer_t_min: f32,
    outer_t_max: f32,
    outer_k_min: u16,
    outer_k_max: u16,
    inner_t_min: f32,
    inner_t_max: f32,
    inner_k_min: u16,
    inner_k_max: u16,
    result: &mut Vec<(f32, f32, u16, u16)>,
) {
    // Clamp inner to outer bounds
    let ic_t_min = inner_t_min.max(outer_t_min);
    let ic_t_max = inner_t_max.min(outer_t_max);
    let ic_k_min = inner_k_min.max(outer_k_min);
    let ic_k_max = inner_k_max.min(outer_k_max);

    // 无重叠 → 整个 outer 都是差集
    if ic_t_min >= ic_t_max || ic_k_min >= ic_k_max {
        result.push((outer_t_min, outer_t_max, outer_k_min, outer_k_max));
        return;
    }

    // 上 strip（outer 在 ic 上方的部分，对应更小的 key 值）
    if ic_k_min > outer_k_min {
        result.push((outer_t_min, outer_t_max, outer_k_min, ic_k_min));
    }
    // 下 strip（outer 在 ic 下方的部分，对应更大的 key 值）
    if ic_k_max < outer_k_max {
        result.push((outer_t_min, outer_t_max, ic_k_max, outer_k_max));
    }
    // 左 strip（outer 在 ic 左侧、上下之间）
    if ic_t_min > outer_t_min {
        result.push((outer_t_min, ic_t_min, ic_k_min, ic_k_max));
    }
    // 右 strip（outer 在 ic 右侧、上下之间）
    if ic_t_max < outer_t_max {
        result.push((ic_t_max, outer_t_max, ic_k_min, ic_k_max));
    }
}
