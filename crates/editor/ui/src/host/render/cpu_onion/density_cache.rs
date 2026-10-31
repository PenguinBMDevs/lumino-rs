//! CPU 洋葱皮总览密度缓存（极限缩放 LOD 的数据侧）。
//!
//! 与文档快照/索引同生命周期：
//! - 索引整建（加载 / 批量编辑 / 结构变化）→ 丢弃，下次进入总览时重建；
//! - 单笔原子变更 → 若缓存正好对应「上一代文档」则原地 ±1（O(变更数)）；
//! - 直接 `track_notes_mut` 编辑（拖动 / 批量 / 连奏）没有原子变更 → 代次对不上
//!   → 保持过期，由 [`DensityCache::ensure`] 在下次进入总览时整建一次，
//!   绝不用错数据渲染。
//!
//! 统计覆盖**全部音轨**，切轨 / 静音不需要重建；彩色覆盖光栅在渲染时按单元
//! 内第一、第二候选过滤当前轨/静音轨。代价是单笔编辑后覆盖位图失效，下一次
//! 使用前必须整建一次（删除音符时无法低成本找回下层轨道）。

use lumino_cpu_onion::DensityPyramid;
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
}

impl DensityCache {
    /// 索引整建时丢弃（数 MiB～数十 MiB，不能长期留着过期数据）。
    pub(crate) fn reset(&mut self) {
        self.pyramid = None;
        self.built_gen = None;
        self.built_keys = 0;
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
                valid &= pyramid.apply_note(old.start_tick, u16::from(old.key), -1);
            }
            if let Some(new) = change.new {
                valid &= pyramid.apply_note(new.start_tick, u16::from(new.key), 1);
            }
        }
        if valid {
            self.built_gen = Some(new_gen);
        } else {
            // 越过既有 bin 布局（例如把音符移到工程末尾之后）：丢弃缓存，
            // 下次进入总览时按最新文档整建。
            self.reset();
        }
    }

    /// 取得与 `gen` / `key_count` 匹配的金字塔；不匹配则整建一次。
    pub(crate) fn ensure(
        &mut self,
        new_gen: u64,
        document: &MidiDocument,
        key_count: u16,
        workers: usize,
    ) -> Option<&DensityPyramid> {
        let coverage_stale = self
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| !pyramid.coverage_valid());
        if self.pyramid.is_none()
            || self.built_gen != Some(new_gen)
            || self.built_keys != key_count
            || coverage_stale
        {
            let started = std::time::Instant::now();
            self.pyramid = Some(DensityPyramid::build(document, workers, key_count));
            self.built_gen = Some(new_gen);
            self.built_keys = key_count;
            tracing::debug!(
                doc_gen = new_gen,
                key_count,
                ms = started.elapsed().as_millis(),
                "[cpu-onion] 构建总览密度金字塔"
            );
        }
        self.pyramid.as_ref()
    }
}

#[cfg(test)]
#[path = "density_cache/tests.rs"]
mod tests;
