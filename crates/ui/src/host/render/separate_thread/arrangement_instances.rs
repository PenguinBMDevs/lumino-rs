//! 工程走带视图实例构建
//! 参考 yinhe 实现，在 CPU 端计算屏幕坐标并构建实例数据

use lumino_gfx::ArrangementNoteInstance;
use crate::editor::arrangement::ArrangementViewport;
use crate::editor::note::Note;
use std::collections::HashMap;

/// 音轨音符映射类型别名
type TrackNotesMap = HashMap<usize, im::Vector<Note>>;

/// 走带视图颜色配置
const AR_BG_COLOR: (f32, f32, f32) = (0.14, 0.14, 0.16);
const AR_LANE_EVEN_COLOR: (f32, f32, f32) = (0.16, 0.16, 0.18);
const AR_LANE_ODD_COLOR: (f32, f32, f32) = (0.13, 0.13, 0.15);
const AR_MEASURE_LINE_COLOR: (f32, f32, f32, f32) = (0.30, 0.30, 0.35, 1.0);
const AR_PLAYHEAD_COLOR: (f32, f32, f32, f32) = (1.0, 1.0, 1.0, 0.8);

/// 构建走带视图的所有实例数据
pub fn build_arrangement_instances(
    out: &mut Vec<ArrangementNoteInstance>,
    viewport: &ArrangementViewport,
    track_notes: &TrackNotesMap,
    track_order: &[usize],
    track_colors: &[[f32; 3]],
    track_visible: &[bool],
    playback_position: f32,
    _midi_document: Option<&lumino_core::midi::MidiDocument>,
) {
    let w = viewport.canvas_size.x;
    let h = viewport.canvas_size.y;
    let lh = viewport.track_height;
    let ppu = viewport.zoom_x;
    let num_tracks = track_order.len();

    // 1. 背景四边形（从 canvas_offset.x 开始，不是 0）
    let lb_w = viewport.canvas_offset.x;
    out.push(ArrangementNoteInstance::background(
        lb_w, 0.0, w - lb_w, h,
        [AR_BG_COLOR.0, AR_BG_COLOR.1, AR_BG_COLOR.2],
    ));

    // 2. 音轨 lane 背景（交替颜色）
    if num_tracks > 0 {
        let (trk_first, trk_last) = visible_track_range(viewport, h, num_tracks);
        for idx in trk_first..trk_last {
            if !track_visible.get(idx).copied().unwrap_or(true) {
                continue;
            }
            let y = lane_y(viewport, idx);
            let col = if idx % 2 == 0 { AR_LANE_EVEN_COLOR } else { AR_LANE_ODD_COLOR };
            out.push(ArrangementNoteInstance::lane(
                lb_w, y, w - lb_w, lh,
                [col.0, col.1, col.2],
            ));
        }
    }

    // 3. 网格线（小节线）
    let ppq = 480.0_f64; // 标准 PPQ
    let ticks_per_bar = ppq * 4.0; // 4/4 拍
    let (tick_start, tick_end) = visible_tick_range(viewport, w);
    let first_bar = ((tick_start / ticks_per_bar).floor() as i32).max(0);
    let last_bar = (tick_end / ticks_per_bar).ceil() as i32;

    for bar in first_bar..=last_bar {
        let tick = bar as f64 * ticks_per_bar;
        let x = tick_to_x(viewport, tick);
        if x >= lb_w && x <= w {
            out.push(ArrangementNoteInstance::grid_line(
                x, 0.0, 1.0, h,
                [AR_MEASURE_LINE_COLOR.0, AR_MEASURE_LINE_COLOR.1, 
                 AR_MEASURE_LINE_COLOR.2, AR_MEASURE_LINE_COLOR.3],
                tick as u32,
            ));
        }
    }

    // 4. 音符矩形
    let tick_pad = (w / ppu) as f64;
    let pad_start = (tick_start - tick_pad).max(0.0);
    let pad_end = tick_end + tick_pad;
    let (trk_first, trk_last) = visible_track_range(viewport, h, num_tracks);
    // 关键：x_offset = canvas_offset.x - scroll_x (参考 yinhe: lb_w - scroll_x)
    let x_offset = viewport.canvas_offset.x - viewport.scroll_x;
    let y_offset = -viewport.scroll_y;
    let lh_per_key = lh / 128.0;

    for (track_idx, track_id) in track_order.iter().enumerate() {
        if track_idx < trk_first || track_idx >= trk_last {
            continue;
        }
        if !track_visible.get(track_idx).copied().unwrap_or(true) {
            continue;
        }

        let color = track_colors.get(track_idx).copied().unwrap_or([0.5, 0.5, 0.5]);

        // 获取音符数据
        let notes = track_notes.get(track_id).cloned().unwrap_or_default();

        // 按音高分桶
        let mut key_buckets: [Vec<(f64, f64, u8)>; 128] = core::array::from_fn(|_| Vec::new());
        for note in &notes {
            let note_start = note.tick as f64;
            let note_end = (note.tick + note.length) as f64;
            if note_start > pad_end {
                continue;
            }
            if note_end < pad_start {
                continue;
            }
            key_buckets[note.key as usize].push((note_start, note_end, note.velocity));
        }

        // 为每个音高构建音符实例（带合并）
        let merge_gap_ticks = (1.0 / ppu as f64).ceil();
        for (key, key_notes) in key_buckets.iter().enumerate() {
            if key_notes.is_empty() {
                continue;
            }

            let key_y_base = y_offset + lh - (key as f32 + 1.0) * lh_per_key + track_idx as f32 * lh;

            let mut sorted_notes = key_notes.clone();
            sorted_notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            let mut merge_start = sorted_notes[0].0;
            let mut merge_end = sorted_notes[0].1;
            let mut merge_vel = sorted_notes[0].2;

            for (s, e, v) in &sorted_notes[1..] {
                if *s <= merge_end + merge_gap_ticks {
                    merge_end = merge_end.max(*e);
                    merge_vel = merge_vel.max(*v);
                } else {
                    flush_note_merge(out, x_offset, key_y_base, ppu, pad_start, pad_end, 
                                     merge_start, merge_end, merge_vel, color);
                    merge_start = *s;
                    merge_end = *e;
                    merge_vel = *v;
                }
            }
            flush_note_merge(out, x_offset, key_y_base, ppu, pad_start, pad_end, 
                             merge_start, merge_end, merge_vel, color);
        }
    }

    // 5. 演奏指示线
    if playback_position > 0.0 {
        let cx = tick_to_x(viewport, playback_position as f64);
        if cx >= lb_w && cx <= w {
            out.push(ArrangementNoteInstance::playhead(
                cx, 0.0, 2.0, h,
                [AR_PLAYHEAD_COLOR.0, AR_PLAYHEAD_COLOR.1, 
                 AR_PLAYHEAD_COLOR.2, AR_PLAYHEAD_COLOR.3],
            ));
        }
    }
}

/// 刷新音符合并
fn flush_note_merge(
    out: &mut Vec<ArrangementNoteInstance>,
    x_offset: f32,
    y: f32,
    ppu: f32,
    pad_start: f64,
    pad_end: f64,
    start: f64,
    end: f64,
    velocity: u8,
    color: [f32; 3],
) {
    let s = start.max(pad_start);
    let e = end.min(pad_end).max(start);
    if s >= e {
        return;
    }
    let nx = x_offset + (s as f32) * ppu;
    let nw = ((e - s) as f32 * ppu).max(2.0);
    out.push(ArrangementNoteInstance::note(nx, y, nw, 4.0, color, velocity));
}

/// 计算音轨 lane 的 y 坐标
fn lane_y(viewport: &ArrangementViewport, track_idx: usize) -> f32 {
    track_idx as f32 * viewport.track_height - viewport.scroll_y
}

/// 计算可见音轨范围
fn visible_track_range(viewport: &ArrangementViewport, height: f32, num_tracks: usize) -> (usize, usize) {
    let first = ((viewport.scroll_y / viewport.track_height).floor() as usize)
        .min(num_tracks.saturating_sub(1));
    let visible_count = (height / viewport.track_height).ceil() as usize + 1;
    let last = (first + visible_count).min(num_tracks);
    (first, last)
}

/// 将 tick 转换为屏幕 x 坐标
/// 公式：屏幕 x = canvas_offset.x + tick * zoom_x - scroll_x
fn tick_to_x(viewport: &ArrangementViewport, tick: f64) -> f32 {
    viewport.canvas_offset.x + (tick as f32 * viewport.zoom_x) - viewport.scroll_x
}

/// 计算可见 tick 范围
fn visible_tick_range(viewport: &ArrangementViewport, width: f32) -> (f64, f64) {
    let start = (viewport.scroll_x / viewport.zoom_x) as f64;
    let end = ((viewport.scroll_x + width) / viewport.zoom_x) as f64;
    (start, end)
}
