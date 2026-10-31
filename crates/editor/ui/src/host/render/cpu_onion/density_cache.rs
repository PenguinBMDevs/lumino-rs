//! CPU 洋葱皮总览密度缓存（极限缩放 LOD 的数据侧）。
//!
//! 与文档快照/索引同生命周期：
//! - 索引整建（加载 / 批量编辑 / 结构变化）→ 丢弃，下次进入总览时重建；
//! - 单笔原子变更 → 若缓存正好对应「上一代文档」则原地 ±1（O(变更数)），
//!   覆盖层只标记受影响 tick 区间，下一次 ensure 局部重建；
//! - 直接 `track_notes_mut` 编辑没有原子变更 → 渲染层用索引提示与上一帧快照
//!   差分后合成变更（O(提示数)），再走同一条增量路径；只有提示溢出才整建。
//!
//! 统计覆盖**全部音轨**，切轨 / 静音不需要重建；彩色覆盖光栅在渲染时按单元
//! 内第一、第二候选过滤当前轨/静音轨。单笔编辑只标记受影响 tick 区间，下一次
//! 使用前做**局部重建**（只扫该区间的音符），不再整建 310M 音符。

use lumino_cpu_onion::{DensityPyramid, TrackIndex};
use lumino_editor_state::CpuOnionChange;
use lumino_midi_model::MidiDocument;

/// 密度金字塔缓存。
#[derive(Debug, Default)]
pub(crate) struct DensityCache {
    /// 当前金字塔（`None` = 未构建 / 已整建失效）。
    pyramid: Option<DensityPyramid>,
    /// 金字塔对应的文档代次。
    built_gen: Option<u64>,
    /// 金字塔对应的 key 数（256-key 切换时必须重建）。
    built_keys: u16,
    /// 尚未重建的脏 tick 区间（历史回放/大变更），下次 ensure 局部重建。
    pending_ranges: Vec<(u32, u32)>,
    /// 测试诊断：按区间重建的次数（用于断言没有退回整建）。
    #[cfg(test)]
    pub(crate) range_rebuilds: u32,
}

impl DensityCache {
    /// 索引整建时丢弃（数 MiB～数十 MiB，不能长期留着过期数据）。
    pub(crate) fn reset(&mut self) {
        self.pyramid = None;
        self.built_gen = None;
        self.built_keys = 0;
        self.pending_ranges.clear();
    }

    /// 记录脏 tick 区间；下次 [`Self::ensure`] 只重建这些区间。
    pub(crate) fn apply_ranges(
        &mut self,
        old_gen: Option<u64>,
        new_gen: u64,
        ranges: &[(u32, u32)],
    ) {
        if ranges.is_empty() || self.built_gen != old_gen || self.pyramid.is_none() {
            return;
        }
        for &(lo, hi) in ranges {
            if hi > lo {
                self.pending_ranges.push((lo, hi));
            }
        }
        if !self.pending_ranges.is_empty() {
            self.built_gen = Some(new_gen);
        }
    }

    /// 按原子变更增量维护（仅当缓存正好对应 `old_gen` 时）。
    pub(crate) fn apply_changes(
        &mut self,
        old_gen: Option<u64>,
        new_gen: u64,
        changes: &[CpuOnionChange],
    ) {
        let (Some(pyramid), Some(built_gen)) = (self.pyramid.as_mut(), self.built_gen) else {
            return;
        };
        if Some(built_gen) != old_gen || changes.is_empty() {
            return;
        }
        let mut valid = true;
        for change in changes {
            if let Some(old) = change.old {
                valid &=
                    pyramid.apply_note_range(old.start_tick, old.end_tick, u16::from(old.key), -1);
            }
            if let Some(new) = change.new {
                valid &=
                    pyramid.apply_note_range(new.start_tick, new.end_tick, u16::from(new.key), 1);
            }
        }
        if valid {
            self.built_gen = Some(new_gen);
        } else {
            // 超过安全内存上限（极远越界）：丢弃缓存，下次进入总览时整建。
            self.reset();
        }
    }

    /// 取得与 `gen` / `key_count` 匹配的金字塔；不匹配则整建一次。
    pub(crate) fn ensure(
        &mut self,
        new_gen: u64,
        document: &MidiDocument,
        index: &TrackIndex,
        key_count: u16,
        workers: usize,
    ) -> Option<&DensityPyramid> {
        let coverage_stale = self
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| !pyramid.coverage_valid());
        let shape_changed = self.pyramid.is_none()
            || self.built_gen != Some(new_gen)
            || self.built_keys != key_count;
        if shape_changed {
            let started = std::time::Instant::now();
            self.pyramid = Some(DensityPyramid::build(document, workers, key_count));
            self.built_gen = Some(new_gen);
            self.built_keys = key_count;
            self.pending_ranges.clear();
            tracing::debug!(
                doc_gen = new_gen,
                key_count,
                ms = started.elapsed().as_millis(),
                "[cpu-onion] 构建总览密度金字塔"
            );
        } else if !self.pending_ranges.is_empty() {
            #[cfg(test)]
            {
                self.range_rebuilds = self.range_rebuilds.saturating_add(1);
            }
            let started = std::time::Instant::now();
            let ranges = std::mem::take(&mut self.pending_ranges);
            let rebuilt = self
                .pyramid
                .as_mut()
                .is_some_and(|pyramid| pyramid.rebuild_dirty_ranges(document, index, &ranges));
            if !rebuilt {
                self.pyramid = Some(DensityPyramid::build(document, workers, key_count));
            }
            self.built_gen = Some(new_gen);
            tracing::debug!(
                doc_gen = new_gen,
                ranges = ranges.len(),
                dirty = rebuilt,
                ms = started.elapsed().as_millis(),
                "[cpu-onion] 按脏区间重建密度"
            );
        } else if coverage_stale {
            let started = std::time::Instant::now();
            let dirty_rebuilt = self
                .pyramid
                .as_mut()
                .is_some_and(|pyramid| pyramid.rebuild_dirty_coverage(document, index));
            if !dirty_rebuilt {
                self.pyramid = Some(DensityPyramid::build(document, workers, key_count));
            }
            self.built_gen = Some(new_gen);
            tracing::debug!(
                doc_gen = new_gen,
                key_count,
                dirty = dirty_rebuilt,
                ms = started.elapsed().as_millis(),
                "[cpu-onion] 局部重建覆盖金字塔"
            );
        }
        self.pyramid.as_ref()
    }
}

#[cfg(test)]
#[path = "density_cache/tests.rs"]
mod tests;
