//! 可见区间 cull 计划（UI 侧计算，随 `RenderParams` 下发渲染线程）。
//!
//! # 口径
//!
//! 与 `cull.wgsl` 完全一致：音符可见需同时满足
//! - `start_tick * zoom_x - scroll_x + keyboard_width + canvas_offset_x <= viewport_w`
//! - `(start_tick + length) * zoom_x - scroll_x + keyboard_width + canvas_offset_x >= 0`
//!
//! 因此候选区间为 `start_tick <= tick_right` 且
//! `start_tick + length >= tick_left`。后者用每轨**最大音符长度**做保守下界：
//! `start_tick >= tick_left - max_len`；`max_len` 缓存为脏（编辑后）时该轨
//! 退化为整轨候选，只损失性能、不丢可见音符。
//!
//! # 为什么在 UI 侧算
//!
//! 渲染线程没有 CPU 侧音符副本（GPU 常驻是单一权威源）；UI 持有
//! `MidiDocument` 与相机参数，二分定位成本 O(音轨数 × log N)，并按每轨
//! 前缀长度得到全局实例区间——一份 8 字节/区间的计划随帧下发即可。

use lumino_gfx::{CullPlan, CullRange};
use lumino_midi_loader::MidiDocument;

/// 参与区间计算的音轨数上限（超过则回退整段 cull，防每帧二分开销失控）。
const MAX_TRACKS_FOR_RANGES: usize = 2048;

/// 相邻区间合并阈值（实例数）：少量间隙合并可减少 dispatch 次数，
/// 多扫几千个实例的成本远低于一次 dispatch 的固定开销。
const MERGE_GAP_INSTANCES: u32 = 8192;

/// 区间数上限（与 `range_cull::MAX_CULL_RANGES` 对齐；超出回退整段）。
const MAX_RANGES: usize = 1024;

/// 单次 dispatch 的固定开销折算成「等价全量扫描实例数」。
///
/// 实测（RTX 2060 / 43 区间）：2M 音符全量 cull 0.46ms，43 区间反而 0.65ms；
/// 8M 时 43 区间 0.47ms vs 全量 0.79ms 才开始占优。按此折算约 10 万实例/
/// dispatch，用于在小区间多、总实例少时直接回退整段，避免负优化。
const DISPATCH_COST_EQUIV_INSTANCES: usize = 100_000;

/// 区间方案是否值得：区间数 × 单次开销 < 总实例数（否则回退整段）。
fn should_use_ranges(total_instances: usize, range_count: usize) -> bool {
    range_count > 0
        && range_count <= MAX_RANGES
        && range_count.saturating_mul(DISPATCH_COST_EQUIV_INSTANCES) < total_instances
}

/// 二分：首个 `start_tick >= tick` 的索引。
fn lower_bound(
    notes: &lumino_midi_loader::ChunkedList<lumino_midi_loader::NoteEvent>,
    tick: u32,
) -> usize {
    let mut lo = 0usize;
    let mut hi = notes.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let before = notes.get(mid).is_some_and(|n| n.start_tick < tick);
        if before {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// 二分：首个 `start_tick > tick` 的索引。
fn upper_bound(
    notes: &lumino_midi_loader::ChunkedList<lumino_midi_loader::NoteEvent>,
    tick: u32,
) -> usize {
    let mut lo = 0usize;
    let mut hi = notes.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let not_after = notes.get(mid).is_some_and(|n| n.start_tick <= tick);
        if not_after {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// 计算本帧可见区间计划。
///
/// `viewport_w` 用渲染器实际 uniform 的逻辑视口宽度（不是 canvas 宽）；
/// `canvas_offset_x`/`keyboard_width` 与 `CameraUniform` 同源。
pub(crate) fn compute_cull_plan(
    doc: Option<&MidiDocument>,
    scroll_x: f32,
    zoom_x: f32,
    viewport_w: f32,
    keyboard_width: f32,
    canvas_offset_x: f32,
) -> CullPlan {
    compute_cull_plan_inner(
        doc,
        scroll_x,
        zoom_x,
        viewport_w,
        keyboard_width,
        canvas_offset_x,
        true,
    )
}

/// 内部实现；`apply_dispatch_budget=false` 仅供单测绕过小样本的规模门槛。
fn compute_cull_plan_inner(
    doc: Option<&MidiDocument>,
    scroll_x: f32,
    zoom_x: f32,
    viewport_w: f32,
    keyboard_width: f32,
    canvas_offset_x: f32,
    apply_dispatch_budget: bool,
) -> CullPlan {
    let Some(doc) = doc else {
        return CullPlan::default();
    };
    let total: usize = (0..doc.track_count())
        .map(|t| doc.track_notes(t).len())
        .sum();
    if total == 0 || doc.track_count() > MAX_TRACKS_FOR_RANGES {
        return CullPlan {
            ranges: Vec::new(),
            total_instances: total,
        };
    }
    if !zoom_x.is_finite() || zoom_x <= 0.0 || !viewport_w.is_finite() {
        return CullPlan {
            ranges: Vec::new(),
            total_instances: total,
        };
    }

    let left_px = scroll_x - keyboard_width - canvas_offset_x;
    let right_px = scroll_x + viewport_w - keyboard_width - canvas_offset_x;
    let tick_left_f = left_px / zoom_x;
    let tick_right_f = right_px / zoom_x;
    let tick_left = tick_left_f.max(0.0).floor().min(u32::MAX as f32) as u32;
    let tick_right = tick_right_f.max(0.0).floor().min(u32::MAX as f32) as u32;

    let mut ranges: Vec<CullRange> = Vec::new();
    let mut cursor: usize = 0;
    for track_id in 0..doc.track_count() {
        let notes = doc.track_notes(track_id);
        let len = notes.len();
        if len > 0 {
            let end_idx = upper_bound(notes, tick_right);
            let start_idx = match doc.track_max_note_len(track_id) {
                // 已知最大长度：起点可安全前移到 tick_left - max_len
                Some(max_len) => lower_bound(notes, tick_left.saturating_sub(max_len)),
                // 脏（编辑后）：整轨候选，只损失性能不丢可见音符
                None => 0,
            };
            if end_idx > start_idx {
                let start = cursor + start_idx;
                let count = end_idx - start_idx;
                let start = u32::try_from(start).unwrap_or(u32::MAX);
                let count = u32::try_from(count).unwrap_or(u32::MAX);
                push_merged(&mut ranges, CullRange { start, count });
            }
        }
        cursor += len;
    }

    if apply_dispatch_budget && !should_use_ranges(total, ranges.len()) {
        // 区间过多或总实例太少：dispatch 固定开销会吃掉收益，回退整段 cull。
        return CullPlan {
            ranges: Vec::new(),
            total_instances: total,
        };
    }
    CullPlan {
        ranges,
        total_instances: total,
    }
}

/// 追加区间；与上一段间隙 ≤ [`MERGE_GAP_INSTANCES`] 时合并。
fn push_merged(ranges: &mut Vec<CullRange>, range: CullRange) {
    if let Some(last) = ranges.last_mut() {
        let last_end = last.start.saturating_add(last.count);
        if range.start <= last_end.saturating_add(MERGE_GAP_INSTANCES) {
            let new_end = range.start.saturating_add(range.count).max(last_end);
            last.count = new_end.saturating_sub(last.start);
            return;
        }
    }
    ranges.push(range);
}

#[cfg(test)]
mod tests;
