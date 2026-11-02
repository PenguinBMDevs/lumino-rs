//! 直接 `track_notes_mut` 编辑的索引提示（与逐音符脏区分离）。
//!
//! 不产生 `CpuOnionDirty.full`：提示超过单点上限时折叠成索引包围盒，
//! 渲染层仍可用上一帧快照按包围盒差分，绝不整建文档。

use super::super::EditorData;
use super::{CpuOnionDirectHint, MAX_CPU_ONION_DIRECT_EDITS};

impl EditorData {
    /// 记录直接 `track_notes_mut` 等长编辑的索引提示（当前轨）。
    pub fn record_cpu_onion_direct_indices(&mut self, track: usize, indices: &[usize]) {
        if !self.cpu_onion_dirty_enabled || self.history_replay || indices.is_empty() {
            return;
        }
        let (Some(lo), Some(hi)) = (indices.iter().min(), indices.iter().max()) else {
            return;
        };
        self.record_history_direct(track, *lo, *hi);
        if self
            .cpu_onion_dirty
            .as_ref()
            .is_some_and(|dirty| dirty.full)
        {
            return;
        }
        if self.cpu_onion_direct_overflow {
            self.union_direct_bbox(track, *lo, *hi);
            return;
        }
        let room = MAX_CPU_ONION_DIRECT_EDITS.saturating_sub(self.cpu_onion_direct_edits.len());
        if indices.len() > room {
            let (lo, hi) = self.direct_bounds(track, *lo, *hi);
            self.cpu_onion_direct_edits.clear();
            self.cpu_onion_direct_bbox = Some((track, lo, hi));
            self.cpu_onion_direct_overflow = true;
            return;
        }
        self.cpu_onion_direct_edits
            .extend(indices.iter().map(|index| (track, *index)));
    }

    /// 记录闭区间 `[lo, hi]` 的直接编辑索引提示（重排/量化路径）。
    pub fn record_cpu_onion_direct_ranges(&mut self, track: usize, ranges: &[(usize, usize)]) {
        if !self.cpu_onion_dirty_enabled || self.history_replay {
            return;
        }
        for &(lo, hi) in ranges {
            self.record_history_direct(track, lo, hi);
            if self.cpu_onion_direct_overflow {
                self.union_direct_bbox(track, lo, hi);
                continue;
            }
            let count = hi.saturating_sub(lo).saturating_add(1);
            let room = MAX_CPU_ONION_DIRECT_EDITS.saturating_sub(self.cpu_onion_direct_edits.len());
            if count > room {
                let (lo, hi) = self.direct_bounds(track, lo, hi);
                self.cpu_onion_direct_edits.clear();
                self.cpu_onion_direct_bbox = Some((track, lo, hi));
                self.cpu_onion_direct_overflow = true;
                continue;
            }
            self.cpu_onion_direct_edits
                .extend((lo..=hi).map(|index| (track, index)));
        }
    }

    /// 当前已收集提示与新范围的索引包围盒。
    fn direct_bounds(&self, track: usize, lo: usize, hi: usize) -> (usize, usize) {
        self.cpu_onion_direct_edits
            .iter()
            .filter(|(edit_track, _)| *edit_track == track)
            .map(|(_, index)| *index)
            .fold((lo, hi), |(lo, hi), index| (lo.min(index), hi.max(index)))
    }

    fn union_direct_bbox(&mut self, track: usize, lo: usize, hi: usize) {
        self.cpu_onion_direct_bbox = Some(match self.cpu_onion_direct_bbox {
            Some((old_track, old_lo, old_hi)) if old_track == track => {
                (track, old_lo.min(lo), old_hi.max(hi))
            }
            Some(existing) => existing,
            None => (track, lo, hi),
        });
    }

    /// 记录历史回放用的直接编辑索引区间（与每帧消费的提示分离）。
    fn record_history_direct(&mut self, track: usize, lo: usize, hi: usize) {
        if hi < lo {
            return;
        }
        for range in &mut self.history_pending_direct {
            if range.0 == track
                && lo <= range.2.saturating_add(1)
                && hi >= range.1.saturating_sub(1)
            {
                range.1 = range.1.min(lo);
                range.2 = range.2.max(hi);
                return;
            }
        }
        self.history_pending_direct.push((track, lo, hi));
    }

    /// 取出直接编辑索引提示（渲染层每帧消费一次）。
    #[must_use]
    pub fn take_cpu_onion_direct_edits(&mut self) -> CpuOnionDirectHint {
        let overflow = std::mem::take(&mut self.cpu_onion_direct_overflow);
        (
            std::mem::take(&mut self.cpu_onion_direct_edits),
            overflow,
            self.cpu_onion_direct_bbox.take(),
        )
    }
}
