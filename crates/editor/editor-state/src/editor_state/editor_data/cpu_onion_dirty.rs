//! CPU 洋葱皮脏区元数据（Step 5a）
//!
//! 单一职责：把「音符数据刚刚发生了什么变化」以**最小代价**传达给 CPU 洋葱皮
//! 渲染层，让它能只重画受影响的像素矩形而不是整帧。
//!
//! # 为什么不用 `note_delta_events`
//!
//! [`super::NoteDeltaEvent`] 只服务**当前音轨**的 GPU 段内增量（索引语义、等长
//! 修改）。CPU 洋葱皮画的恰好是**非当前轨**——两者覆盖范围互补而非重叠，
//! 复用它只会引入语义误读。因此本模块独立记录「按值」的原子变更。
//!
//! # 开销守卫（默认 GPU 用户零成本）
//!
//! 默认后端是 GPU，若每次 `insert/remove/update` 都无条件 push，GPU 用户的
//! 拖动/批量编辑会白付 `Vec` 分配 + `NoteEvent` 拷贝，且脏区会一直积累到
//! Host 清理。故本模块全部记录入口受 [`EditorData::cpu_onion_dirty_enabled`]
//! 闸门控制：`false` 时**立即返回**（零分配、零拷贝），由 Host 在切到 CPU
//! 后端时开启、切回 GPU 时关闭并清空。
//!
//! # 容量
//!
//! 只服务**单笔编辑**（插入 / 删除 / 更新单个音符）。批量/整轨替换/区间删除
//! 一律升级为 [`CpuOnionDirty::full`]（整帧兜底），因此 `changes` 不需要无界：
//! 超过 [`MAX_CPU_ONION_CHANGES`] 直接升级为 full，杜绝长事务把队列撑爆。

use lumino_midi_model::NoteEvent;
use lumino_note_core::history::SnapshotNoteChange;

use super::EditorData;

/// 单笔原子变更：`old`/`new` 二者其一为 `None` 表达插入/删除。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuOnionChange {
    /// 文档音轨索引
    pub track: usize,
    /// 变更前音符（插入时为 `None`）
    pub old: Option<NoteEvent>,
    /// 变更后音符（删除时为 `None`）
    pub new: Option<NoteEvent>,
}

/// 自上次消费以来的脏区集合。
///
/// `full == true` 时 [`changes`](Self::changes) 恒为空（整帧兜底优先）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuOnionDirty {
    /// 整帧兜底：变化无法用原子变更表达（批量插入/区间删除/整轨替换/结构变化）
    pub full: bool,
    /// 原子变更（按记录顺序，供按轨聚合）
    pub changes: Vec<CpuOnionChange>,
    /// 大变更的脏 tick 区间；非空时密度只重建这些区间，不再整建。
    pub ranges: Vec<(u32, u32)>,
}

impl CpuOnionDirty {
    /// 是否有可用的原子变更或脏区间（`full` 为真时恒为 `false`）。
    #[must_use]
    pub fn has_changes(&self) -> bool {
        !self.full && (!self.changes.is_empty() || !self.ranges.is_empty())
    }

    /// 是否没有任何待处理变化。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.full && self.changes.is_empty() && self.ranges.is_empty()
    }

    /// 合并一个脏 tick 区间；相邻/重叠区间直接合并。
    pub fn push_range(&mut self, start: u32, end: u32) {
        if self.full {
            return;
        }
        push_dirty_range(&mut self.ranges, start, end);
    }
}

/// 原子变更条数上限：超过即升级为整帧兜底（单笔编辑远达不到这个量级）。
pub const MAX_CPU_ONION_CHANGES: usize = 32;

/// 直接编辑索引提示上限：超过后折叠为索引包围盒。
pub const MAX_CPU_ONION_DIRECT_EDITS: usize = 8192;
/// 直接编辑提示：`(索引点, 是否溢出, 包围盒)`。
pub type CpuOnionDirectHint = (Vec<(usize, usize)>, bool, Option<(usize, usize, usize)>);

/// 脏 tick 区间数上限：超出后折叠为一个包围区间，杜绝内存无界增长。
pub const MAX_CPU_ONION_RANGES: usize = 4096;

/// 合并区间；到达上限后折叠为包围区间（仍然只重建区间，不整建）。
pub(crate) fn push_dirty_range(out: &mut Vec<(u32, u32)>, start: u32, end: u32) {
    if end <= start {
        return;
    }
    for range in &mut *out {
        if start <= range.1 && end >= range.0 {
            range.0 = range.0.min(start);
            range.1 = range.1.max(end);
            return;
        }
    }
    if out.len() >= MAX_CPU_ONION_RANGES {
        let lo = out
            .iter()
            .map(|range| range.0)
            .min()
            .map_or(start, |lo| lo.min(start));
        let hi = out
            .iter()
            .map(|range| range.1)
            .max()
            .map_or(end, |hi| hi.max(end));
        out.clear();
        out.push((lo, hi));
        return;
    }
    out.push((start, end));
}

mod direct;

impl EditorData {
    /// CPU 洋葱皮脏区记录是否开启（默认 `false`，GPU 后端零开销）。
    #[inline]
    #[must_use]
    pub fn cpu_onion_dirty_enabled(&self) -> bool {
        self.cpu_onion_dirty_enabled
    }

    /// 开关 CPU 洋葱皮脏区记录。
    ///
    /// 关闭时**立即清空**已有脏区：GPU 后端不消费脏区，留着只会泄漏到下一次
    /// 切回 CPU 时被误当成「真实变化」。
    pub fn set_cpu_onion_dirty_enabled(&mut self, enabled: bool) {
        self.cpu_onion_dirty_enabled = enabled;
        if !enabled {
            self.cpu_onion_dirty = None;
            self.cpu_onion_direct_edits.clear();
            self.cpu_onion_direct_bbox = None;
            self.cpu_onion_direct_overflow = false;
            self.history_pending_changes.clear();
            self.history_pending_ranges.clear();
            self.history_pending_direct.clear();
            self.history_pending_full = false;
        }
    }

    /// 记录单笔原子变更（插入 / 删除 / 更新）。
    ///
    /// 关闭时立即返回（零分配）；已存在 full 时直接忽略（full 已覆盖一切）。
    pub fn record_cpu_onion_change(
        &mut self,
        track: usize,
        old: Option<NoteEvent>,
        new: Option<NoteEvent>,
    ) {
        if !self.cpu_onion_dirty_enabled || self.history_replay {
            return;
        }
        let Some(dirty) = self.cpu_onion_dirty.as_mut() else {
            self.cpu_onion_dirty = Some(CpuOnionDirty {
                full: false,
                changes: vec![CpuOnionChange { track, old, new }],
                ranges: Vec::new(),
            });
            self.push_history_pending(CpuOnionChange { track, old, new });
            return;
        };
        if dirty.full {
            return;
        }
        if dirty.changes.len() >= MAX_CPU_ONION_CHANGES {
            // 超过逐音符上限：转成脏 tick 区间，密度只重建区间，不整建。
            if let Some((lo, hi)) = note_change_range(old, new) {
                dirty.push_range(lo, hi);
            }
            return;
        }
        dirty.changes.push(CpuOnionChange { track, old, new });
        self.push_history_pending(CpuOnionChange { track, old, new });
    }

    /// 记录脏 tick 区间（大批量删除/历史回放），密度只重建该区间。
    pub fn record_cpu_onion_range(&mut self, start: u32, end: u32) {
        if !self.cpu_onion_dirty_enabled || self.history_replay || end <= start {
            return;
        }
        push_dirty_range(&mut self.history_pending_ranges, start, end);
        self.history_pending_full = false;
        let dirty = self
            .cpu_onion_dirty
            .get_or_insert_with(CpuOnionDirty::default);
        if !dirty.full {
            dirty.push_range(start, end);
        }
    }

    /// 记录整帧兜底（批量插入 / 区间删除 / 整轨替换 / 结构变化）。
    ///
    /// 关闭时立即返回；升级为 full 时清空已积累的原子变更（full 优先）。
    pub fn record_cpu_onion_full(&mut self) {
        if !self.cpu_onion_dirty_enabled || self.history_replay {
            return;
        }
        self.history_pending_changes.clear();
        self.history_pending_ranges.clear();
        self.history_pending_direct.clear();
        self.history_pending_full = true;
        self.cpu_onion_direct_edits.clear();
        self.cpu_onion_direct_bbox = None;
        self.cpu_onion_direct_overflow = false;
        match self.cpu_onion_dirty.as_mut() {
            Some(dirty) => {
                dirty.full = true;
                dirty.changes.clear();
            }
            None => {
                self.cpu_onion_dirty = Some(CpuOnionDirty {
                    full: true,
                    changes: Vec::new(),
                    ranges: Vec::new(),
                });
            }
        }
    }

    /// 取出并清空脏区（渲染层每帧消费一次）。
    #[must_use]
    pub fn take_cpu_onion_dirty(&mut self) -> Option<CpuOnionDirty> {
        self.cpu_onion_dirty.take()
    }

    /// 丢弃脏区（GPU 后端 / 无文档 / 重置）。
    pub fn clear_cpu_onion_dirty(&mut self) {
        self.cpu_onion_dirty = None;
        self.cpu_onion_direct_edits.clear();
        self.cpu_onion_direct_bbox = None;
        self.cpu_onion_direct_overflow = false;
    }

    /// 回放历史变更：只写入待渲染脏区，不污染下一笔历史。
    pub(crate) fn record_cpu_onion_replay_changes(
        &mut self,
        changes: &[CpuOnionChange],
        ranges: &[(u32, u32)],
        full: bool,
    ) {
        if !self.cpu_onion_dirty_enabled {
            return;
        }
        // 有 tick 区间就只重建区间；逐音符变更超过上限也降级为区间，绝不整建。
        let mut dirty = CpuOnionDirty {
            full: false,
            changes: if changes.len() <= MAX_CPU_ONION_CHANGES {
                changes.to_vec()
            } else {
                Vec::new()
            },
            ranges: Vec::new(),
        };
        for &(lo, hi) in ranges {
            push_dirty_range(&mut dirty.ranges, lo, hi);
        }
        if dirty.ranges.is_empty() && changes.len() > MAX_CPU_ONION_CHANGES {
            dirty.changes.clear();
            for change in changes {
                if let Some((lo, hi)) = note_change_range(change.old, change.new) {
                    dirty.push_range(lo, hi);
                }
            }
        }
        if full && dirty.ranges.is_empty() {
            dirty.full = true;
            dirty.changes.clear();
        }
        self.cpu_onion_dirty = Some(dirty);
    }

    /// 把一笔变更追加到「待挂历史」；超限后转脏区间，不再整建。
    fn push_history_pending(&mut self, change: CpuOnionChange) {
        if let Some((lo, hi)) = note_change_range(change.old, change.new) {
            push_dirty_range(&mut self.history_pending_ranges, lo, hi);
        }
        if self.history_pending_full {
            return;
        }
        if self.history_pending_changes.len() >= MAX_CPU_ONION_CHANGES {
            self.history_pending_changes.clear();
            return;
        }
        self.history_pending_changes.push(SnapshotNoteChange {
            track: change.track,
            old: change.old,
            new: change.new,
        });
    }

    /// 只读脏区引用（诊断与测试用，不消费）。
    #[inline]
    #[must_use]
    pub fn cpu_onion_dirty(&self) -> Option<&CpuOnionDirty> {
        self.cpu_onion_dirty.as_ref()
    }
}

/// 一组音符覆盖的 tick 区间（只读）。
pub(crate) fn notes_range<'a>(notes: impl Iterator<Item = &'a NoteEvent>) -> Option<(u32, u32)> {
    let mut range: Option<(u32, u32)> = None;
    for note in notes {
        let end = note.end_tick.max(note.start_tick.saturating_add(1));
        range = Some(match range {
            Some((lo, hi)) => (lo.min(note.start_tick), hi.max(end)),
            None => (note.start_tick, end),
        });
    }
    range
}

/// 音符增删改覆盖的 tick 区间（旧/新位置的并集）。
pub(crate) fn note_change_range(
    old: Option<NoteEvent>,
    new: Option<NoteEvent>,
) -> Option<(u32, u32)> {
    let mut range: Option<(u32, u32)> = None;
    for note in [old, new].into_iter().flatten() {
        let end = note.end_tick.max(note.start_tick.saturating_add(1));
        range = Some(match range {
            Some((lo, hi)) => (lo.min(note.start_tick), hi.max(end)),
            None => (note.start_tick, end),
        });
    }
    range
}

#[cfg(test)]
#[path = "cpu_onion_dirty/tests.rs"]
mod tests;
