//! 洋葱皮视口范围查询 — 增量缓存版
//!
//! 单一职责：以缓存为中心的四条渲染路径调度。
//! 数据结构和缓存 API 在 `onion_skin_cache`，Editor API 在 `onion_skin_editor`，
//! 简单查询在 `onion_skin_ops`。本模块只做这一件事。

use std::collections::HashMap;

use crate::editor::Editor;
use lumino_core::midi::MidiDocument;
use lumino_gfx::NoteInstance;
use rayon::prelude::*;

use super::onion_skin_cache::{
    MergedCell, ONION_SKIN_CACHE, ONION_SKIN_POOL, OnionSkinCache, merge_cell, merge_one_track,
    rebuild_output_from_cells, track_config_hash, track_hash_no_color,
};

impl Editor {
    /// 获取所有洋葱皮音符实例（视口范围内）—— 增量缓存版
    ///
    /// 四条路径（按速度从快到慢）：
    /// 1. **脏音轨路径** O(dirty×query)：单轨数据变化时只重查该轨
    /// 2. **颜色快速路径** O(C)：仅颜色/透明度变化 → 重打包 output，不重查 document
    /// 3. **增量路径** O(ΔT×N)：水平滚动 → 移除出界 cell + 查询新区间 + 合并
    /// 4. **全量重建** O(T×N)：音轨集合/缩放变化 → 完整重查 + 合并
    ///
    /// 避免 799 个音轨全量重查，16M 事件迭代。
    pub fn get_all_onion_skin_instances_in_range(
        &mut self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return Vec::new();
        };

        let search_start = visible_tick_start;
        let search_end = visible_tick_end;
        let search_key_min = visible_key_min;
        let search_key_max = visible_key_max;

        let track_indices = self.collect_visible_track_indices(track_onion_states);
        if track_indices.is_empty() {
            return Vec::new();
        }

        let tick_span = (search_end - search_start) as u32;
        let tick_quant = (tick_span / 100).max(1);

        // 预收集音轨颜色
        let track_colors: Vec<(usize, [f32; 4])> = track_indices
            .iter()
            .map(|&track_idx| {
                let color = self.onion_skin_config.get_track_color(track_idx);
                let color_arr = super::note::color_to_array(color);
                (track_idx, color_arr)
            })
            .collect();

        let config_hash = track_config_hash(&track_colors);
        let track_hash = track_hash_no_color(&track_colors);

        // === 尝试缓存 ===
        let mut cache_guard = ONION_SKIN_CACHE.write().unwrap();

        if let Some(ref mut cache) = *cache_guard {
            // ----- 路径 1：脏音轨处理 -----
            if !cache.dirty_tracks.is_empty() {
                return Self::dirty_track_path(
                    cache,
                    doc,
                    &track_colors,
                    config_hash,
                    track_hash,
                    search_start,
                    search_end,
                    search_key_min,
                    search_key_max,
                    tick_quant,
                );
            }

            // ----- 路径 2：颜色快速路径 -----
            if cache.colors_dirty && cache.track_hash == track_hash {
                cache.output = rebuild_output_from_cells(&cache.cells, &track_colors);
                cache.config_hash = config_hash;
                cache.colors_dirty = false;

                tracing::info!(
                    "Onion skin (color fast path): {} cells rebuilt",
                    cache.output.len(),
                );
                return (*cache.output).clone();
            }

            // ----- 路径 3：增量路径 -----
            if cache.can_incremental(
                tick_quant,
                search_start,
                search_end,
                search_key_min,
                search_key_max,
                track_hash,
            ) {
                return Self::incremental_path(
                    cache,
                    doc,
                    &track_colors,
                    config_hash,
                    track_hash,
                    search_start,
                    search_end,
                    search_key_min,
                    search_key_max,
                    tick_quant,
                );
            }
        }

        // ----- 路径 4：全量重建 -----
        let merged_map: HashMap<u64, MergedCell> = ONION_SKIN_POOL.install(|| {
            track_colors
                .par_iter()
                .filter_map(|&(track_idx, _)| {
                    let raw =
                        doc.get_track_notes_in_range(track_idx as u16, search_start, search_end);
                    if raw.is_empty() {
                        return None;
                    }
                    merge_one_track(
                        &raw,
                        search_start,
                        search_end,
                        search_key_min,
                        search_key_max,
                        tick_quant,
                        track_idx as u16,
                    )
                })
                .reduce(HashMap::new, |mut acc, cells| {
                    for (k, v) in cells {
                        merge_cell(&mut acc, k, v);
                    }
                    acc
                })
        });

        let result = rebuild_output_from_cells(&merged_map, &track_colors);

        tracing::info!(
            "Onion skin (full): {} tracks → {} instances (quant={})",
            track_indices.len(),
            result.len(),
            tick_quant,
        );

        // 存入缓存（Arc clone，仅原子操作）
        *cache_guard = Some(OnionSkinCache {
            tick_quant,
            search_start,
            search_end,
            search_key_min,
            search_key_max,
            track_hash,
            config_hash,
            cells: merged_map,
            output: std::sync::Arc::clone(&result),
            colors_dirty: false,
            dirty_tracks: std::collections::HashSet::new(),
        });

        // 返回 Vec（Arc 是唯一引用时 try_unwrap 零拷贝，否则 fallback clone）
        std::sync::Arc::try_unwrap(result).unwrap_or_else(|arc| (*arc).clone())
    }

    // ───── 私有辅助方法 ─────

    /// 脏音轨路径：移除脏轨 cells → 重新查询 → 合并 → 重建 output
    fn dirty_track_path(
        cache: &mut OnionSkinCache,
        doc: &std::sync::Arc<MidiDocument>,
        track_colors: &[(usize, [f32; 4])],
        config_hash: u64,
        track_hash: u64,
        search_start: f32,
        search_end: f32,
        search_key_min: u16,
        search_key_max: u16,
        tick_quant: u32,
    ) -> Vec<NoteInstance> {
        let dirty_tracks: Vec<u16> = cache.dirty_tracks.drain().collect();
        for &dirty_idx in &dirty_tracks {
            cache.cells.retain(|_, cell| cell.track_idx != dirty_idx);
        }
        for &dirty_idx in &dirty_tracks {
            let raw = doc.get_track_notes_in_range(dirty_idx, search_start, search_end);
            if !raw.is_empty()
                && let Some(new_cells) = merge_one_track(
                    &raw,
                    search_start,
                    search_end,
                    search_key_min,
                    search_key_max,
                    tick_quant,
                    dirty_idx,
                )
            {
                for (k, v) in new_cells {
                    merge_cell(&mut cache.cells, k, v);
                }
            }
        }
        cache.output = rebuild_output_from_cells(&cache.cells, track_colors);
        cache.search_start = search_start;
        cache.search_end = search_end;
        cache.search_key_min = search_key_min;
        cache.search_key_max = search_key_max;
        cache.config_hash = config_hash;
        cache.track_hash = track_hash;
        cache.colors_dirty = false;
        tracing::debug!(
            "Onion skin (dirty tracks): {} tracks → {} cells",
            dirty_tracks.len(),
            cache.output.len(),
        );
        (*cache.output).clone()
    }

    /// 增量路径：移除出界 cell → 确定新区间 → 只查询新区间 → 合并 → 重建
    fn incremental_path(
        cache: &mut OnionSkinCache,
        doc: &std::sync::Arc<MidiDocument>,
        track_colors: &[(usize, [f32; 4])],
        config_hash: u64,
        track_hash: u64,
        search_start: f32,
        search_end: f32,
        search_key_min: u16,
        search_key_max: u16,
        tick_quant: u32,
    ) -> Vec<NoteInstance> {
        // key 范围变化 → 移除超界 cell
        if cache.search_key_min != search_key_min || cache.search_key_max != search_key_max {
            cache.cells.retain(|_, cell| {
                let k = cell.key as u16;
                k >= search_key_min && k <= search_key_max
            });
        }
        // 移除水平出界 cell
        cache
            .cells
            .retain(|_, cell| cell.max_right >= search_start && cell.tick_start <= search_end);

        // 确定新区间
        let (new_start, new_end) = if search_start > cache.search_start {
            (cache.search_end, search_end)
        } else {
            (search_start, cache.search_start)
        };

        if new_end > new_start {
            let new_cells = ONION_SKIN_POOL.install(|| {
                track_colors
                    .par_iter()
                    .filter_map(|&(track_idx, _)| {
                        let raw =
                            doc.get_track_notes_in_range(track_idx as u16, new_start, new_end);
                        if raw.is_empty() {
                            return None;
                        }
                        merge_one_track(
                            &raw,
                            new_start,
                            new_end,
                            search_key_min,
                            search_key_max,
                            tick_quant,
                            track_idx as u16,
                        )
                    })
                    .reduce(HashMap::new, |mut acc, cells| {
                        for (k, v) in cells {
                            merge_cell(&mut acc, k, v);
                        }
                        acc
                    })
            });
            for (k, v) in new_cells {
                merge_cell(&mut cache.cells, k, v);
            }
        }

        cache.search_start = search_start;
        cache.search_end = search_end;
        cache.search_key_min = search_key_min;
        cache.search_key_max = search_key_max;
        cache.config_hash = config_hash;
        cache.track_hash = track_hash;
        cache.colors_dirty = false;
        cache.output = rebuild_output_from_cells(&cache.cells, track_colors);
        tracing::info!(
            "Onion skin (incremental): {} cells, new_range=[{},{}]",
            cache.output.len(),
            new_start,
            new_end,
        );
        (*cache.output).clone()
    }
}
