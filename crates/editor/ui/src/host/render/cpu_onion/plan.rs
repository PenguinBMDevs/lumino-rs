//! CPU 洋葱皮更新计划（纯函数，Step 5a）。
//!
//! 把「布局 / 文档代次 / 脏区 / 当前轨 / 静音」这些输入映射成
//! **重绘范围 + 索引动作**，与渲染器/状态机完全解耦——这样每条语义分支都能
//! 用纯单测锁死，不必构造 `Host` 或 GPU 设备。
//!
//! # 判定优先级（与 `state::CpuOnionState::update` 一一对应）
//!
//! 1. 索引缺失（冷启动 / 停用后重启）→ 整帧 + 整索引；
//! 2. 帧缺失但索引还在（`clear_frame` 后恢复）→ 整帧，索引沿用；
//! 3. 布局键变化（相机 / 当前轨 / 静音 / 调色板）→ 整帧；索引只按文档侧证据决定
//!    （**切轨不等于重建索引**：只需补齐旧当前轨的几何）；
//! 4. `dirty.full` → 整帧 + 整索引；代次变了且**无脏区又无受影响轨**（真正未知来源）
//!    → 整帧 + 整索引；
//! 5. 代次未变、无原子变更、无过期轨 → 复用（不重绘、不递增版本）；
//! 6. 存在「未出现在 changes 里的洋葱轨」变化 → 增量表达不了 → 整帧 + 整索引
//!    （含 `dirty == None` 但来源集合里点名了洋葱轨的情形）；
//! 7. 视觉变更（排除当前轨/静音轨）为空 → 只更新快照/索引，不重绘
//!    —— **当前轨直接编辑（`dirty == None` + `dirty_tracks == Some(empty|{current})`）
//!    走的就是这条**，这是 Q4 B「当前轨拖动不重绘 CPU 帧」的落点；
//! 8. 脏区面积 > 整帧 1/4 → 整帧，否则区域重绘。

use std::collections::HashSet;

use lumino_cpu_onion::{CanvasCamera, CpuOnionRect};
use lumino_editor_state::{CpuOnionChange, CpuOnionDirty};
use lumino_midi_model::NoteEvent;

/// 脏区向四周扩张的像素数。
///
/// 覆盖三层不确定性：`draw_note` 的像素中心取整（±1px）、每侧 2px 描边（共 2px）、
/// 以及浮点坐标 × DPI 的累积误差。4px 是「够用且不放大太多」的折中。
pub(crate) const DIRTY_PAD_PX: f32 = 4.0;

/// 脏区面积超过整帧这个分数时改用整帧重绘（区域重绘已无收益）。
const FULL_REDRAW_AREA_DIVISOR: u64 = 4;

/// 重绘范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CpuOnionRedraw {
    /// 整帧重画
    Full,
    /// 只重画该矩形
    Region(CpuOnionRect),
}

/// 索引动作（成本从低到高）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CpuOnionIndex {
    /// 索引与快照都是最新的
    Keep,
    /// 按原子变更增量更新（外加旧当前轨几何补齐）
    Incremental,
    /// 整份重建（O(总音符数)）
    Rebuild,
}

/// 一次 `update` 的计划。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CpuOnionPlan {
    /// 重绘范围；`None` = 不重绘、不递增版本（可能仍需更新索引）
    pub redraw: Option<CpuOnionRedraw>,
    /// 索引动作
    pub index: CpuOnionIndex,
}

impl CpuOnionPlan {
    /// 完全复用上一帧。
    pub(crate) const REUSE: Self = Self {
        redraw: None,
        index: CpuOnionIndex::Keep,
    };
}

/// 计划输入（全部来自调用方采集的运行时真值）。
pub(crate) struct PlanInput<'a> {
    /// 已有可复用帧
    pub frame_ready: bool,
    /// 已有索引
    pub index_ready: bool,
    /// 布局键（相机/当前轨/静音/调色板）相对上一帧变化
    pub layout_changed: bool,
    /// 文档音符代次相对索引/快照变化
    pub gen_changed: bool,
    /// 存在「索引几何过期且已不再是当前轨」的音轨（切轨后必须补齐）
    pub stale_needs_repair: bool,
    /// 本次消费的脏区
    pub dirty: Option<&'a CpuOnionDirty>,
    /// 本次变化明确的受影响音轨（`None` = 未知/全部）
    pub dirty_tracks: Option<&'a HashSet<usize>>,
    /// 当前音轨（CPU 帧不画）
    pub current_track: usize,
    /// 静音表（长度不足按未静音处理）
    pub muted: &'a [bool],
    /// 帧相机
    pub camera: &'a CanvasCamera,
    /// 帧像素尺寸 `(宽, 高)`
    pub frame_size: (u32, u32),
}

/// 计算一次更新的计划。
pub(crate) fn plan_update(input: PlanInput<'_>) -> CpuOnionPlan {
    let dirty_full = input.dirty.is_some_and(|dirty| dirty.full);
    let has_changes = input.dirty.is_some_and(CpuOnionDirty::has_changes);

    // ① 索引缺失：唯一无条件整建路径
    if !input.index_ready {
        return CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Rebuild,
        };
    }

    // ② 帧缺失（停用后重启 / 相机退化后恢复）：必须整帧；索引沿用既有证据
    if !input.frame_ready {
        return CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: index_action(&input, dirty_full, has_changes),
        };
    }

    // ③ 布局键变化：必须整帧；索引按文档侧证据决定（切轨不等于重建索引）
    if input.layout_changed {
        return CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: index_action(&input, dirty_full, has_changes),
        };
    }

    // ④ 整帧兜底：显式 full，或**连变化来源都不知道**（无脏区 + 无受影响轨）
    //
    // ⚠️ 这里必须是 `source_unknown`，不能只看 `dirty.is_none()`：
    // 当前轨拖动/批量/连奏直接改 `track_notes_mut`，不经记录点 → `dirty == None`，
    // 但 `onion_dirty_tracks` 仍然是 `Some(empty | {current})`。那是**已知来源**
    // （当前轨，CPU 不画），必须走「只更新快照/索引、不重绘」；否则每一帧拖动都会
    // 整帧重绘 + 整索引重建，Step 5a 对当前轨的收益直接归零。
    if dirty_full || (input.gen_changed && source_unknown(&input)) {
        return CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Rebuild,
        };
    }

    // ⑤ 什么都没变：复用（最高频路径，零分配）
    if !input.gen_changed && !has_changes && !input.stale_needs_repair {
        return CpuOnionPlan::REUSE;
    }

    // ⑥ 有洋葱轨变化没被 changes 覆盖（含「来源未知」）→ 保守整帧
    if has_uncovered_onion_tracks(&input, has_changes) {
        return CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Rebuild,
        };
    }

    let index = index_action(&input, dirty_full, has_changes);

    // ⑦ 视觉变更为空（只动了当前轨/静音轨）：更新快照/索引即可
    let Some(rect) = visual_dirty_rect(&input) else {
        return CpuOnionPlan {
            redraw: None,
            index,
        };
    };

    // ⑧ 面积过大则整帧（区域重绘已无收益）
    let frame_area = u64::from(input.frame_size.0) * u64::from(input.frame_size.1);
    let redraw = if rect.area().saturating_mul(FULL_REDRAW_AREA_DIVISOR) > frame_area {
        CpuOnionRedraw::Full
    } else {
        CpuOnionRedraw::Region(rect)
    };
    CpuOnionPlan {
        redraw: Some(redraw),
        index,
    }
}

/// 是否**连变化来源都不知道**（既没有脏区，也没有受影响轨集合）。
///
/// 必须与「`dirty == None` 但 `dirty_tracks == Some(..)`」严格区分：后者是
/// **已知来源**——拖动 / 批量编辑 / 连奏直接改 `track_notes_mut`，不经插入删除
/// 更新 API 所以没有原子变更，但 `onion_dirty_tracks` 明确告诉我们是哪条轨
/// （通常是当前轨，CPU 帧本来就不画它）。把它当成「未知」会让当前轨的每一帧
/// 拖动都退化成整帧重绘 + 整索引重建，正是 Step 5a 要消灭的开销。
fn source_unknown(input: &PlanInput<'_>) -> bool {
    input.dirty.is_none() && input.dirty_tracks.is_none()
}

/// 索引动作判定（布局分支与常规分支共用）。
fn index_action(input: &PlanInput<'_>, dirty_full: bool, has_changes: bool) -> CpuOnionIndex {
    if !input.index_ready || dirty_full {
        return CpuOnionIndex::Rebuild;
    }
    if !input.gen_changed {
        // 代次没变也要补旧当前轨几何（切轨后它是洋葱轨了）
        return if input.stale_needs_repair {
            CpuOnionIndex::Incremental
        } else {
            CpuOnionIndex::Keep
        };
    }
    // 无 changes 但有受影响轨集合（当前轨直接编辑）→ 增量刷新快照/代次即可；
    // 只有「来源未知」或「有未覆盖的洋葱轨」才必须整建。
    if source_unknown(input) || has_uncovered_onion_tracks(input, has_changes) {
        return CpuOnionIndex::Rebuild;
    }
    CpuOnionIndex::Incremental
}

/// 是否存在「未出现在 `changes` 里的洋葱轨」变化。
///
/// `dirty_tracks == None` 表示来源未知（影响全部音轨）→ 一律视为未覆盖（保守）。
fn has_uncovered_onion_tracks(input: &PlanInput<'_>, has_changes: bool) -> bool {
    let Some(tracks) = input.dirty_tracks else {
        return true;
    };
    if tracks.is_empty() {
        return false;
    }
    let covered: HashSet<usize> = if has_changes {
        input
            .dirty
            .map(|dirty| dirty.changes.iter().map(|change| change.track).collect())
            .unwrap_or_default()
    } else {
        HashSet::new()
    };
    tracks
        .iter()
        .any(|track| is_onion_track(*track, input) && !covered.contains(track))
}

/// 该轨是否由 CPU 洋葱皮负责（非当前轨且非静音）。
fn is_onion_track(track: usize, input: &PlanInput<'_>) -> bool {
    track != input.current_track && !input.muted.get(track).copied().unwrap_or(false)
}

/// 视觉脏区（排除当前轨/静音轨；空或全部越界时返回 `None`）。
fn visual_dirty_rect(input: &PlanInput<'_>) -> Option<CpuOnionRect> {
    let dirty = input.dirty?;
    if dirty.full {
        return None;
    }
    let mut union = CpuOnionRect::new(0, 0, 0, 0);
    for change in &dirty.changes {
        if !is_onion_track(change.track, input) {
            continue;
        }
        for note in [change.old, change.new].into_iter().flatten() {
            if let Some(rect) = note_dirty_rect(input.camera, &note, input.frame_size) {
                union = union.union(rect);
            }
        }
    }
    (!union.is_empty()).then_some(union)
}

/// 单音符在帧内的像素矩形（含描边/取整余量），越界裁剪。
fn note_dirty_rect(
    camera: &CanvasCamera,
    note: &NoteEvent,
    frame_size: (u32, u32),
) -> Option<CpuOnionRect> {
    if camera.zoom_x <= 0.0 || camera.zoom_y <= 0.0 {
        return None;
    }
    let sx = note.start_tick as f32 * camera.zoom_x - camera.scroll_x;
    let sw = note.length() as f32 * camera.zoom_x;
    let sy = camera.key_to_y(note.key as u16);
    let sh = camera.zoom_y;
    let (x0, x1) = clamp_span(sx, sx + sw, frame_size.0)?;
    let (y0, y1) = clamp_span(sy, sy + sh, frame_size.1)?;
    Some(CpuOnionRect::new(x0, y0, x1 - x0, y1 - y0))
}

/// 把 `[start, end)` 向外扩 [`DIRTY_PAD_PX`] 后夹到 `[0, limit]`。
fn clamp_span(start: f32, end: f32, limit: u32) -> Option<(u32, u32)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let limit_f = limit as f32;
    let lo = (start - DIRTY_PAD_PX).max(0.0);
    let hi = (end + DIRTY_PAD_PX).min(limit_f);
    if hi <= lo || lo >= limit_f {
        return None;
    }
    let lo = lo.floor() as u32;
    let hi = hi.ceil().min(limit_f) as u32;
    (hi > lo).then_some((lo, hi))
}

/// 按轨聚合原子变更：`(track, removed, added)`（轨索引升序）。
pub(crate) fn group_changes(
    changes: &[CpuOnionChange],
) -> Vec<(usize, Vec<NoteEvent>, Vec<NoteEvent>)> {
    let mut tracks: Vec<usize> = changes.iter().map(|change| change.track).collect();
    tracks.sort_unstable();
    tracks.dedup();
    let mut grouped = Vec::with_capacity(tracks.len());
    for track in tracks {
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for change in changes.iter().filter(|change| change.track == track) {
            if let Some(old) = change.old {
                removed.push(old);
            }
            if let Some(new) = change.new {
                added.push(new);
            }
        }
        grouped.push((track, removed, added));
    }
    grouped
}

#[cfg(test)]
#[path = "plan/tests.rs"]
mod tests;
