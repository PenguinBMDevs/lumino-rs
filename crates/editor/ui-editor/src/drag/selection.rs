//! 框选（Selection）相关逻辑：增量更新、全量重建、矩形差集
//!
//! 从 `drag.rs` 抽出，控制文件行数并保持单一职责。

use crate::{EditState, Editor};

/// 框选窗口查询的 lookback 上界（tick），覆盖「跨入」长音符（同
/// `rendering/visible_notes.rs::NOTES_WINDOW_LOOKBACK` 的工程假设）
const SELECTION_WINDOW_LOOKBACK: u32 = 1_000_000;

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
    fn rebuild_selected_notes(&mut self, min_tick: f32, max_tick: f32, min_key: u16, max_key: u16) {
        self.selection_clear();

        self.ensure_spatial_index();
        let indices: Vec<usize> = if let Some(index) = self.spatial.note_index.borrow().as_ref() {
            let mut cache = self.spatial.query_cache.borrow_mut();
            cache.clear();
            index.update_query(min_tick, max_tick, min_key, max_key, &mut cache);
            cache.iter().copied().collect()
        } else {
            // 1600W 超大型工程（ensure_spatial_index 跳过构建 → None）→ 窗口扫描
            // 块级二分框出框选 tick 范围（含 lookback 跨入），替代 O(N) 线性回退。
            puffin::profile_scope!("diag::selection_window_scan");
            let note_count = self.editor_state.data.current_track_note_count();
            tracing::debug!(
                "diag::selection_window_scan — 音符数={}（无空间索引，窗口回退）",
                note_count
            );
            let track_notes = self.editor_state.data.current_track_notes();
            let start_u32 = min_tick.max(0.0) as u32;
            let end_u32 = max_tick.max(0.0) as u32;
            let (lo, hi) =
                track_notes.window_range(start_u32, end_u32 + 1, SELECTION_WINDOW_LOOKBACK);
            track_notes
                .iter_window(lo, hi)
                .filter(|&(_, note)| {
                    let note_end = note.end_tick as f32;
                    note_end >= min_tick
                        && note.start_tick as f32 <= max_tick
                        && note.key as u16 >= min_key
                        && note.key as u16 <= max_key
                })
                .map(|(i, _)| i)
                .collect()
        };
        for i in indices {
            self.selection_insert(i);
        }
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
    editor.ensure_spatial_index();
    if editor.spatial.note_index.borrow().is_none() {
        return rebuild_full_selection(editor, new_min_t, new_max_t, new_min_k, new_max_k);
    }

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

    let remove_list = {
        let index = editor.spatial.note_index.borrow();
        // 守卫（L323）已确认非 None；let-else 防御性兜底，避免 panic 路径
        let Some(index) = index.as_ref() else {
            return;
        };
        let mut cache = editor.spatial.query_cache.borrow_mut();
        let mut list = Vec::new();
        for &(t_min, t_max, k_min, k_max) in &delta_rects {
            cache.clear();
            index.update_query(t_min, t_max, k_min, k_max, &mut cache);
            list.extend(cache.iter().copied());
        }
        list
    };
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

    let added_indices = {
        let index = editor.spatial.note_index.borrow();
        // 守卫（L323）已确认非 None；let-else 防御性兜底，避免 panic 路径
        let Some(index) = index.as_ref() else {
            return;
        };
        let mut cache = editor.spatial.query_cache.borrow_mut();
        let mut list = Vec::new();
        for &(t_min, t_max, k_min, k_max) in &delta_rects {
            cache.clear();
            index.update_query(t_min, t_max, k_min, k_max, &mut cache);
            list.extend(cache.iter().copied());
        }
        list
    };
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
