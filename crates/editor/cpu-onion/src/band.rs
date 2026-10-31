//! 单 band 光栅化：只画非当前轨、非静音轨（onion-only）。
//!
//! 与 GPU `onion_note.wgsl` 的洋葱皮区语义对齐：
//! - 跳过当前音轨（主轨由 GPU 渲染，本 crate 不画）；
//! - 跳过静音轨；`current_track` 越界或 `muted` 长度不足时按 false 处理；
//! - 音轨按索引升序、轨内音符按 `start_tick` 升序绘制；
//! - owner = 1、owner_limit = 0 实现 first-wins：先画的音符占住像素后，
//!   后画的音符（含边框）不能覆盖，对应 GPU 洋葱皮区「全局源索引越小越靠前」
//!   的稳定深度语义；
//! - 长音符（> [`LOOKBACK_TICKS`]）与正常窗口音符按 `start_tick` 归并，
//!   避免「长音符 underlay 先画」导致晚开始的长音符错误覆盖早开始的短音符；
//! - 两条流在同一个 `start_tick` 上相遇（tie）时，顺序不能靠「长音先 / 短音先」
//!   猜——回文档按**全局源索引**取该 tick 的音符组，与 GPU 洋葱皮区
//!   （`window_range` / `iter_window` 返回的源顺序）保持一致。

use std::sync::Arc;

use lumino_midi_model::{ChunkedList, MidiDocument, NoteEvent};

use crate::{
    buffer::BandBuffer,
    camera::CanvasCamera,
    color::NoteStyle,
    index::{LOOKBACK_TICKS, TrackIndex},
    notes::draw_note,
    simd::SimdLevel,
};

/// 一条 band 的完整渲染输入。
#[derive(Clone)]
pub(crate) struct BandJob {
    /// band 在画布中的起始 x（含）。
    pub(crate) x0: u32,
    /// band 结束 x（排他）。
    pub(crate) x1: u32,
    /// 相机快照。
    pub(crate) camera: CanvasCamera,
    /// band 的 tick 区间（含端点）。
    pub(crate) tick_lo: u32,
    pub(crate) tick_hi: u32,
    /// 可见 key 区间。
    pub(crate) key_min: u16,
    pub(crate) key_max: u16,
    /// 当前音轨（该轨不画）。
    pub(crate) current_track: usize,
    /// 静音位图（长度不足时缺省 false）。
    pub(crate) muted: Arc<[bool]>,
    /// 每轨颜色。
    pub(crate) style: Arc<NoteStyle>,
    /// 聚合索引（只读）。
    pub(crate) index: Arc<TrackIndex>,
    /// 音符唯一权威源（与编辑层同一个 Arc）。
    pub(crate) document: Arc<MidiDocument>,
    /// SIMD 档位。
    pub(crate) simd: SimdLevel,
    /// 是否允许画布覆盖后提前退出（测试可关闭做逐像素对照）。
    pub(crate) early_coverage: bool,
}

/// band 渲染统计。
#[derive(Debug, Default)]
#[cfg_attr(not(test), derive(Clone, Copy))]
pub(crate) struct BandStats {
    /// 实际尝试绘制的音符数（通过 tick/key 过滤）。
    pub(crate) visible_notes: u64,
    /// 覆盖提前退出的像素目标。
    pub(crate) coverage_target: usize,
    /// 实际覆盖像素数。
    pub(crate) covered: usize,
    /// 测试专用：`draw_note` 的调用顺序（生产构建没有该字段，热路径零成本）。
    #[cfg(test)]
    pub(crate) draw_order: Vec<NoteEvent>,
}

/// 渲染一条 band 到透明 band 缓冲。
pub(crate) fn render_band(job: &BandJob, buffer: &mut BandBuffer) -> BandStats {
    let width = (job.x1 - job.x0) as usize;
    let height = job.camera.height as usize;
    buffer.ensure(width, height);
    buffer.clear();

    let mut stats = BandStats::default();
    if width == 0 || height == 0 {
        return stats;
    }

    let key_lo = u32::from(job.key_min.min(127));
    let key_hi = u32::from(job.key_max.min(127));
    let visible_key_mask = if key_hi >= key_lo {
        let span = key_hi - key_lo + 1;
        if span >= 128 {
            u128::MAX
        } else {
            ((1u128 << span) - 1) << key_lo
        }
    } else {
        0
    };
    let active_keys =
        job.index
            .active_keys(job.tick_lo, job.tick_hi, job.current_track, &job.muted)
            & visible_key_mask;
    let active_rows = if job.index.key_masks.is_empty() {
        height
    } else {
        (active_keys.count_ones() as usize)
            .saturating_mul(job.camera.zoom_y.ceil().max(1.0) as usize)
            .min(height)
    };
    let coverage_target = if job.early_coverage {
        width.saturating_mul(active_rows)
    } else {
        usize::MAX
    };
    stats.coverage_target = coverage_target;
    if coverage_target == 0 {
        return stats;
    }

    let mut covered = 0usize;
    for track in 0..job.document.notes.len() {
        if track == job.current_track {
            continue;
        }
        if job.muted.get(track).copied().unwrap_or(false) {
            continue;
        }
        let notes = job.document.track_notes(track);
        if notes.is_empty() {
            continue;
        }
        draw_track(
            job,
            buffer,
            track,
            notes,
            &mut stats,
            &mut covered,
            coverage_target,
        );
        if covered >= coverage_target {
            break;
        }
    }
    stats.covered = covered;
    stats
}

fn draw_track(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    notes: &ChunkedList<NoteEvent>,
    stats: &mut BandStats,
    covered: &mut usize,
    coverage_target: usize,
) {
    let (tick_lo, tick_hi) = (job.tick_lo, job.tick_hi);
    let lookback = job.index.lookback(track);
    let (lo, hi) = notes.window_range(tick_lo, tick_hi.saturating_add(1), lookback);

    // 长音符候选：start_tick 已排序，take_while 到窗口右界即可；end_tick
    // 早于窗口左界的跳过。正常窗口音符只保留 <= LOOKBACK_TICKS 的，避免与
    // long_notes 重复绘制。
    let mut long_iter = job
        .index
        .long_notes(track)
        .iter()
        .take_while(|note| note.start_tick <= tick_hi)
        .filter(|note| note.end_tick >= tick_lo)
        .peekable();
    let mut window_iter = notes
        .iter_window(lo, hi)
        .filter_map(|(_, note)| {
            (note.length() <= LOOKBACK_TICKS && note.end_tick >= tick_lo).then_some(note)
        })
        .peekable();

    loop {
        // 两条流都在同一个 `start_tick` 上：谁先画不能靠「长音先 / 短音先」猜，
        // 必须回文档按全局源索引取这一组，否则与 GPU 的源顺序不一致。
        // 组查询只在 tie 时发生，常路径（不同 start）不为它付代价。
        let tie_tick = match (long_iter.peek(), window_iter.peek()) {
            (Some(long), Some(window)) if long.start_tick == window.start_tick => {
                Some(long.start_tick)
            }
            _ => None,
        };
        if let Some(tick) = tie_tick {
            *covered += draw_tick_group(
                job,
                buffer,
                track,
                notes,
                tick,
                stats,
                coverage_target.saturating_sub(*covered),
            );
            if *covered >= coverage_target {
                return;
            }
            // 两个流都必须跳过该 tick：组查询已经把其中所有可见音符画完了。
            while long_iter.peek().is_some_and(|note| note.start_tick == tick) {
                long_iter.next();
            }
            while window_iter
                .peek()
                .is_some_and(|note| note.start_tick == tick)
            {
                window_iter.next();
            }
            continue;
        }

        let take_long = match (long_iter.peek(), window_iter.peek()) {
            (Some(long), Some(window)) => long.start_tick < window.start_tick,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        let candidate = if take_long {
            long_iter.next()
        } else {
            window_iter.next()
        };
        let Some(note) = candidate else {
            continue;
        };
        *covered += draw_one(job, buffer, track, note, stats);
        if *covered >= coverage_target {
            return;
        }
    }
}

/// 取同一 `start_tick` 的音符组，按文档**全局源索引**顺序绘制。
///
/// 长音符与正常窗口音符平时分两条流各自有序，但同 tick 内没有先后关系——
/// 短音可能先插入（源索引更小），也可能后插入。GPU 洋葱皮区按源索引稳定排序，
/// 所以 tie 时只能按 tick 回文档查一次。
///
/// `window_range(tick, tick + 1, 0)` 只框这一个 tick（`lookback = 0`，不会把前一
/// 个 tick 的音符带进来）。`tick.saturating_add(1)` 防 `u32::MAX` 溢出；溢出时
/// 区间退化为空，该组是文档末尾的 `u32::MAX` 音符，直接取到末尾（组内仍按
/// `start_tick == tick` 过滤，不会混入别的 tick）。
fn draw_tick_group(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    notes: &ChunkedList<NoteEvent>,
    tick: u32,
    stats: &mut BandStats,
    budget: usize,
) -> usize {
    let (lo, hi) = notes.window_range(tick, tick.saturating_add(1), 0);
    let hi = if tick == u32::MAX { notes.len() } else { hi };
    let mut covered = 0usize;
    for (_, note) in notes.iter_window(lo, hi) {
        if note.start_tick != tick || note.end_tick < job.tick_lo {
            continue;
        }
        covered += draw_one(job, buffer, track, note, stats);
        if covered >= budget {
            break;
        }
    }
    covered
}

/// 单个音符：key 过滤 + 光栅化 + 统计（单流路径与 tick 组查询共用）。
fn draw_one(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    note: &NoteEvent,
    stats: &mut BandStats,
) -> usize {
    if (note.key as u16) < job.key_min || (note.key as u16) > job.key_max {
        return 0;
    }
    #[cfg(test)]
    stats.draw_order.push(*note);
    if let Some(covered) = draw_note(job, buffer, track, note) {
        stats.visible_notes = stats.visible_notes.saturating_add(1);
        covered
    } else {
        0
    }
}

#[cfg(test)]
mod tests;
