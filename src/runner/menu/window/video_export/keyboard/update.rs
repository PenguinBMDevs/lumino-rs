use lumino_extras::palette::current_track_color_f32;
use lumino_midi_loader::MidiDocument;

use super::*;

/// 获取指定音轨的演奏高亮颜色（RGBA `[u8; 4]`）
///
/// 使用设置面板的调色板样式，保证琴键颜色与音符颜色一致。
fn track_color_rgba(track_idx: usize) -> [u8; 4] {
    let color_rgba = current_track_color_f32(track_idx);
    [
        (color_rgba[0] * 255.0).round() as u8,
        (color_rgba[1] * 255.0).round() as u8,
        (color_rgba[2] * 255.0).round() as u8,
        255,
    ]
}

/// 根据当前播放 tick 计算每个 key 的覆盖颜色
///
/// 直接从 `MidiDocument.notes` 读取，数据在 MIDI 导入时已按 track 分组并按
/// `start_tick` 升序排列。使用 `partition_point` 二分查找当前 tick 的活动音符。
///
/// # 性能策略（增量扫描）
///
/// - 正常播放：增量扫描新进入的音符 + retain 清理已结束音符，每帧 O(活跃音符数)。
/// - 回退 / 跳变：触发全量重建。
pub fn update_playback_key_colors(
    document: &MidiDocument,
    tick: u32,
    state: &mut PlaybackKeyColorState,
    out: &mut [u8; KEY_COLOR_BYTES],
) {
    let track_count = document.notes.len();

    // 检测是否需要全量重建：文档结构变化、tick 回退、大幅前跳
    let need_full_rebuild = state.scan_idx.len() != track_count
        || tick < state.last_tick
        || tick.saturating_sub(state.last_tick) > SEEK_THRESHOLD_TICKS;

    if need_full_rebuild {
        *state = PlaybackKeyColorState {
            last_tick: tick,
            scan_idx: vec![0; track_count],
            active_notes: Vec::new(),
        };

        for (track_idx, notes) in document.notes.iter().enumerate() {
            if notes.is_empty() {
                continue;
            }
            let color = track_color_rgba(track_idx);
            // ChunkedList::partition_point(tick+1) = 第一个 tick > tick 的索引
            let end = notes.partition_point(tick.wrapping_add(1));
            state.scan_idx[track_idx] = end;
            // `iter_window(0, end)` 经块偏移直接定位，与 `iter().take(end)` 同集合，
            // 规避从头平铺扫描（高数据量下每帧 O(前缀) 是已知热点）。
            for (_, n) in notes.iter_window(0, end) {
                if n.end_tick > tick {
                    state
                        .active_notes
                        .push((n.end_tick, (n.key as usize) * 4, color));
                }
            }
        }
    } else {
        if state.scan_idx.len() < track_count {
            state.scan_idx.resize(track_count, 0);
        }

        for (track_idx, notes) in document.notes.iter().enumerate() {
            if notes.is_empty() {
                continue;
            }
            let color = track_color_rgba(track_idx);
            let start = state.scan_idx[track_idx];
            // ChunkedList::partition_point(tick+1) = 第一个 tick > tick 的索引
            let end = notes.partition_point(tick.wrapping_add(1));
            state.scan_idx[track_idx] = end;
            // 增量区间 `[start, end)` 经块偏移直接定位：`iter().skip(start)` 会在
            // 窗口前平铺丢弃 O(start) 个元素（36% 进度下数百万/轨/帧），`iter_window`
            // 同集合但 O(log 块数 + 区间)。输出 active_notes 与旧路径逐元素一致。
            for (_, n) in notes.iter_window(start, end) {
                if n.end_tick > tick {
                    state
                        .active_notes
                        .push((n.end_tick, (n.key as usize) * 4, color));
                }
            }
        }

        state
            .active_notes
            .retain(|(end_tick, _, _)| *end_tick > tick);
    }

    state.last_tick = tick;

    out.fill(0);
    for (_, offset, color) in &state.active_notes {
        let off = *offset;
        if off + 4 <= out.len() {
            out[off..off + 4].copy_from_slice(color);
        }
    }
}

/// 根据当前播放 tick 和一组音符直接计算每个 key 的覆盖颜色
///
/// 与 [`update_playback_key_colors`] 行为一致，但不维护增量扫描状态。
/// 用于流式读取模式：每帧从硬盘读取的音符列表已经过滤到视口范围，
/// 直接遍历其中在当前 tick 活跃的音符着色即可。
///
/// `notes` 为元组 `(start_tick, end_tick, key, track_idx)`。
pub fn update_playback_key_colors_from_notes(
    notes: &[(u32, u32, u16, u16)],
    tick: u32,
    out: &mut [u8; KEY_COLOR_BYTES],
) {
    out.fill(0);
    for (start_tick, end_tick, key, track_idx) in notes {
        if *start_tick <= tick && *end_tick > tick {
            let color = track_color_rgba(*track_idx as usize);
            let off = (*key as usize) * 4;
            if off + 4 <= out.len() {
                out[off..off + 4].copy_from_slice(&color);
            }
        }
    }
}
