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
//!   避免「长音符 underlay 先画」导致晚开始的长音符错误覆盖早开始的短音符。

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
}

/// band 渲染统计。
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct BandStats {
    /// 实际尝试绘制的音符数（通过 tick/key 过滤）。
    pub(crate) visible_notes: u64,
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
        draw_track(job, buffer, track, notes, &mut stats);
    }
    stats
}

fn draw_track(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    notes: &ChunkedList<NoteEvent>,
    stats: &mut BandStats,
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
        let take_long = match (long_iter.peek(), window_iter.peek()) {
            (Some(long), Some(window)) => long.start_tick <= window.start_tick,
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
        if (note.key as u16) < job.key_min || (note.key as u16) > job.key_max {
            continue;
        }
        if draw_note(job, buffer, track, note) {
            stats.visible_notes = stats.visible_notes.saturating_add(1);
        }
    }
}
