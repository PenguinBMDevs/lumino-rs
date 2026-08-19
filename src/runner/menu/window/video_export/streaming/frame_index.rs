//! 帧索引构建（按 start_tick 二分窗口查询）

use lumino_midi_loader::TICK_SEARCH_BUFFER;
use rayon::prelude::*;

use super::super::{seconds_to_tick, ticks_to_seconds};
use super::NoteRecord;

/// 帧索引条目：记录本帧需要渲染的音符范围
#[derive(Debug, Clone, Copy)]
pub struct FrameIndexEntry {
    /// 在 note_records 数组中的起始索引
    pub note_offset: u32,
    /// 本帧需要渲染的音符数量
    pub note_count: u32,
}

pub(crate) fn build_frame_index(
    records: &[NoteRecord],
    ppqn: u32,
    total_ticks: u32,
    tempos: &[(u32, f32)],
    fps: f64,
    viewport_beats: f64,
) -> Result<Vec<FrameIndexEntry>, String> {
    let total_secs = ticks_to_seconds(total_ticks as u64, ppqn, tempos);
    let total_frames = (total_secs * fps).ceil() as u64;
    // 视口 tick 跨度 = 拍数 × PPQN（与内存模式 viewport_tick_span = ppq * 16 一致）
    let viewport_tick_span = ((ppqn as f64 * viewport_beats.max(1.0)) as u32).max(1);

    let note_count = records.len();

    if note_count == 0 {
        return Ok(Vec::new());
    }

    // 预计算全文件最大音符长度，用作搜索窗口下界的动态缓冲区。
    // 固定 TICK_SEARCH_BUFFER=19200 会导致时长超过该值的超长音符
    // 在 start_tick 远早于 vp_start 时被窗口排除，音符半路消失。
    let max_note_length = records
        .iter()
        .map(|r| r.end_tick.saturating_sub(r.start_tick))
        .max()
        .unwrap_or(0)
        .max(TICK_SEARCH_BUFFER);

    let index: Vec<FrameIndexEntry> = (0..total_frames)
        .into_par_iter()
        .map(|frame_idx| {
            let frame_time = frame_idx as f64 / fps;
            let center_tick = seconds_to_tick(frame_time, tempos, ppqn);
            let vp_start = center_tick;
            let vp_end = vp_start.saturating_add(viewport_tick_span);

            // 二分窗口 [left, right)：left = 第一个 start_tick >= vp_start - max_note_length
            // 的记录，right = 第一个 start_tick > vp_end 的记录。
            //
            // 正确性：视口内可见音符（end_tick >= vp_start && start_tick <= vp_end）必然
            // start_tick <= vp_end（在上界内）；任意时长的跨视口长音符必然满足
            // start_tick >= vp_start - max_note_length（因为 end_tick - start_tick <= max_note_length，
            // 而 end_tick >= vp_start → start_tick >= vp_start - max_note_length）。
            // 因此窗口是可见集合的超集，渲染前再按完整区间条件过滤即可。
            //
            // 性能：每帧读取量从"视口起点到文件末尾"（O(N)）降为 O(窗口内音符数)，
            // 修复导出耗时随总音符数线性上升的问题。
            let left = records
                .partition_point(|r| r.start_tick < vp_start.saturating_sub(max_note_length));
            let right = records.partition_point(|r| r.start_tick <= vp_end);

            FrameIndexEntry {
                note_offset: left as u32,
                note_count: right.saturating_sub(left) as u32,
            }
        })
        .collect();

    Ok(index)
}
