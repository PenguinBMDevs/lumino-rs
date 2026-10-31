//! 文档快照 / 派生索引 / 总览密度缓存的维护（`state.rs` 子模块）。
//!
//! 拆出来只为守住单文件 < 400 行：三个动作都只改 `CpuOnionState` 的字段，
//! 不参与帧计划判定（见 `plan.rs`）。

use std::collections::HashSet;
use std::sync::Arc;

use lumino_cpu_onion::TrackIndex;
use lumino_editor_state::{CpuOnionChange, CpuOnionDirty, MAX_CPU_ONION_CHANGES};
use lumino_midi_model::MidiDocument;

use super::super::plan::{CpuOnionIndex, group_changes};
use super::CpuOnionState;

impl CpuOnionState {
    /// 把直接编辑索引提示合成为按值变更（供密度缓存增量）。
    ///
    /// 直接 `track_notes_mut` 只给索引、不给 old/new；这里用**上一帧文档快照**
    /// 与当前文档按索引差分。索引稳定（等长修改），成本 O(提示数)，不扫整轨。
    /// 已有原子变更或来源无法对齐时保持原脏区/整帧兜底。
    pub(crate) fn merge_direct_edits(
        &self,
        document: &MidiDocument,
        dirty: Option<CpuOnionDirty>,
        direct_edits: &[(usize, usize)],
        overflow: bool,
    ) -> Option<CpuOnionDirty> {
        if (direct_edits.is_empty() && !overflow)
            || dirty
                .as_ref()
                .is_some_and(|dirty| dirty.full || !dirty.changes.is_empty())
        {
            return dirty;
        }
        let full = || {
            Some(CpuOnionDirty {
                full: true,
                changes: Vec::new(),
            })
        };
        if overflow {
            return full();
        }
        let Some(previous) = self.document.as_ref() else {
            return full();
        };
        let mut seen = HashSet::with_capacity(direct_edits.len());
        let mut changes = Vec::new();
        for &(track, index) in direct_edits {
            if !seen.insert((track, index)) {
                continue;
            }
            let old = previous.track_notes(track).get(index).copied();
            let new = document.track_notes(track).get(index).copied();
            if old == new {
                continue;
            }
            if changes.len() >= MAX_CPU_ONION_CHANGES {
                return full();
            }
            changes.push(CpuOnionChange { track, old, new });
        }
        Some(CpuOnionDirty {
            full: false,
            changes,
        })
    }

    /// 维护文档快照与派生索引（`Rebuild` / `Incremental` / `Keep`）。
    ///
    /// 密度缓存与索引同寿命：整建丢弃；单笔原子变更与直接编辑索引提示都走
    /// 增量；只有提示溢出/超过内存上限才在下次进入总览时整建。
    pub(super) fn maintain_index(
        &mut self,
        document: &MidiDocument,
        action: &CpuOnionIndex,
        dirty: Option<&CpuOnionDirty>,
        current_track: usize,
        new_gen: u64,
    ) {
        match action {
            CpuOnionIndex::Keep => {}
            CpuOnionIndex::Rebuild => {
                let snapshot = Arc::new(document.clone());
                self.index = Some(Arc::new(TrackIndex::build(snapshot.as_ref())));
                self.document = Some(snapshot);
                // 整建 = 全量按最新文档聚合 → 所有轨几何都新鲜
                self.stale_tracks.clear();
                self.density.reset();
            }
            CpuOnionIndex::Incremental => {
                self.document = Some(Arc::new(document.clone()));
                if let Some(dirty) = dirty.filter(|dirty| dirty.has_changes()) {
                    self.density
                        .apply_changes(self.index_gen, new_gen, &dirty.changes);
                }
                self.apply_incremental(dirty, current_track);
            }
        }
    }

    /// 按原子变更增量更新索引；先补齐所有「已不再是当前轨」的过期轨几何。
    ///
    /// 当前轨的过期几何**保留**（CPU 不画它，热路径不为它付 O(该轨音符数)），
    /// 等切轨那一刻再补。
    fn apply_incremental(&mut self, dirty: Option<&CpuOnionDirty>, current_track: usize) {
        let Some(snapshot) = self.document.clone() else {
            return;
        };
        let Some(mut index) = self.index.clone() else {
            return;
        };
        let repaired: Vec<usize> = self
            .stale_tracks
            .iter()
            .copied()
            .filter(|track| *track != current_track)
            .collect();
        for track in &repaired {
            index = Arc::new(index.rebuild_track_geometry(snapshot.as_ref(), *track));
            self.stale_tracks.remove(track);
        }
        if let Some(dirty) = dirty.filter(|dirty| dirty.has_changes()) {
            for (track, removed, added) in group_changes(&dirty.changes) {
                if repaired.contains(&track) {
                    // 该轨已按最新文档整轨重算，再叠 changes 会重复计数
                    continue;
                }
                index = Arc::new(index.with_changes_fast(track, &removed, &added));
            }
        }
        self.index = Some(index);
    }
}
