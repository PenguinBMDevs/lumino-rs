//! 工程走带视图实例构建 — 屏幕坐标版，二分查找加速
//!
//! 每帧重建全部实例，但通过对 MidiDocument 事件做二分查找
//! 将扫描范围限制在可见 tick 区间内，O(log N + K) 极速

use crate::editor::arrangement::ArrangementViewport;
use crate::editor::note::Note;
use lumino_gfx::ArrangementNoteInstance;
use std::collections::HashMap;

type TrackNotesMap = HashMap<usize, im::Vector<Note>>;

const AR_BG_COLOR: (f32, f32, f32) = (0.14, 0.14, 0.16);
const AR_LANE_EVEN_COLOR: (f32, f32, f32) = (0.16, 0.16, 0.18);
const AR_LANE_ODD_COLOR: (f32, f32, f32) = (0.13, 0.13, 0.15);
const AR_MEASURE_LINE_COLOR: (f32, f32, f32, f32) = (0.30, 0.30, 0.35, 1.0);
const AR_PLAYHEAD_COLOR: (f32, f32, f32, f32) = (1.0, 1.0, 1.0, 0.8);

/// 构建走带视图全部实例（背景 + lane + 网格线 + 音符 + 演奏指示线）
/// 屏幕坐标，二分查找加速 MidiDocument 事件扫描
pub fn build_arrangement_instances(
    out: &mut Vec<ArrangementNoteInstance>,
    viewport: &ArrangementViewport,
    track_order: &[usize],
    track_colors: &[[f32; 3]],
    track_visible: &[bool],
    midi_doc: Option<&lumino_core::midi::MidiDocument>,
    track_notes: &TrackNotesMap,
    playback_position: f32,
) {
    let w = viewport.canvas_size.x;
    let h = viewport.canvas_size.y;
    let lh = viewport.track_height;
    let ppu = viewport.zoom_x.max(0.001);
    let num_tracks = track_order.len();
    let cox = viewport.canvas_offset.x;
    let coy = viewport.canvas_offset.y;

    // 可见 tick 范围
    let tick_start = (viewport.scroll_x / ppu) as f64;
    let tick_end = ((viewport.scroll_x + w) / ppu) as f64;

    // ── 1. 背景 ──
    out.push(ArrangementNoteInstance::background(
        cox,
        coy,
        w,
        h,
        [AR_BG_COLOR.0, AR_BG_COLOR.1, AR_BG_COLOR.2],
    ));

    // ── 2. Lane 背景 + 音符 ──
    if num_tracks > 0 {
        // 可见音轨范围
        let (trk_first, trk_last) = visible_track_range(viewport, h, num_tracks);

        for (track_idx, track_id) in track_order.iter().enumerate() {
            if track_idx < trk_first || track_idx >= trk_last {
                continue;
            }
            if !track_visible.get(track_idx).copied().unwrap_or(true) {
                continue;
            }

            let color = track_colors
                .get(track_idx)
                .copied()
                .unwrap_or([0.5, 0.5, 0.5]);

            // Lane 背景
            let lane_y = track_y(viewport, track_idx) + coy;
            let col = lane_col(track_idx);
            out.push(ArrangementNoteInstance::lane(
                cox,
                lane_y,
                w,
                lh,
                [col.0, col.1, col.2],
            ));

            // 音符
            if let Some(notes) = track_notes.get(track_id) {
                collect_notes_from_cache(
                    out,
                    notes,
                    track_idx,
                    color,
                    ppu,
                    cox,
                    lane_y,
                    viewport.scroll_x,
                    tick_start,
                    tick_end,
                );
            } else if let Some(doc) = midi_doc {
                collect_notes_from_doc(
                    out, doc, *track_id, track_idx, color, ppu, cox, lane_y, tick_start, tick_end,
                );
            }
        }
    }

    // ── 3. 小节线 ──
    let ppq = 480.0_f64;
    let ticks_per_bar = ppq * 4.0;
    let first_bar = ((tick_start / ticks_per_bar).floor() as i32).max(0);
    let last_bar = (tick_end / ticks_per_bar).ceil() as i32;
    for bar in first_bar..=last_bar {
        let tick = bar as f64 * ticks_per_bar;
        let x = tick_to_x(viewport, tick);
        if x >= cox && x <= cox + w {
            out.push(ArrangementNoteInstance::grid_line(
                x,
                coy,
                1.0,
                h,
                [
                    AR_MEASURE_LINE_COLOR.0,
                    AR_MEASURE_LINE_COLOR.1,
                    AR_MEASURE_LINE_COLOR.2,
                    AR_MEASURE_LINE_COLOR.3,
                ],
                tick as u32,
            ));
        }
    }

    // ── 4. 演奏指示线 ──
    if playback_position > 0.0 {
        let cx = tick_to_x(viewport, playback_position as f64);
        if cx >= cox && cx <= cox + w {
            out.push(ArrangementNoteInstance::playhead(
                cx,
                coy,
                2.0,
                h,
                [
                    AR_PLAYHEAD_COLOR.0,
                    AR_PLAYHEAD_COLOR.1,
                    AR_PLAYHEAD_COLOR.2,
                    AR_PLAYHEAD_COLOR.3,
                ],
            ));
        }
    }
}

/// 从 track_notes 缓存读取音符
fn collect_notes_from_cache(
    out: &mut Vec<ArrangementNoteInstance>,
    notes: &im::Vector<Note>,
    _track_idx: usize,
    color: [f32; 3],
    ppu: f32,
    cox: f32,
    lane_y: f32,
    scroll_x: f32,
    tick_start: f64,
    tick_end: f64,
) {
    let key_height = 48.0 / 128.0;
    for note in notes {
        let s = note.tick as f64;
        let e = (note.tick + note.length) as f64;
        if s > tick_end || e < tick_start {
            continue;
        }
        let sx = cox + note.tick * ppu - scroll_x;
        let sw = note.length * ppu;
        let sy = lane_y + (127.0 - note.key as f32) * key_height;
        out.push(ArrangementNoteInstance::note(
            sx,
            sy,
            sw,
            4.0,
            color,
            note.velocity,
        ));
    }
}

/// 从 MidiDocument 读取音符 — 二分查找 + 零中间分配
fn collect_notes_from_doc(
    out: &mut Vec<ArrangementNoteInstance>,
    doc: &lumino_core::midi::MidiDocument,
    track_id: usize,
    track_idx: usize,
    color: [f32; 3],
    ppu: f32,
    cox: f32,
    lane_y: f32,
    tick_start: f64,
    tick_end: f64,
) {
    use lumino_midi::compact::EventKind;

    let (start, end) = doc.track_events_range(track_id as u16);
    if start >= end {
        return;
    }

    let events = &doc.events[start..end];
    let last_tick = events.last().map(|e| e.delta_tick()).unwrap_or(0);

    // 二分查找可见范围起点（回退1个事件以捕获跨范围 NoteOn）
    let search_begin = events
        .partition_point(|e| (e.delta_tick() as f64) < tick_start)
        .saturating_sub(1);
    let slice = &events[search_begin..];

    let key_h = 48.0 / 128.0; // 每键像素高度

    let mut active: [(u32, u8, u8, bool); 2048] = [(0, 0, 0, false); 2048];

    for ev in slice {
        let tick = ev.delta_tick() as f32;
        let key = ev.param1() as u8;
        let vel = ev.param2() as u8;
        let ch = ev.channel();
        let idx = (ch as usize) * 128 + (key as usize);

        match ev.kind() {
            EventKind::NoteOn if vel > 0 => {
                if active[idx].3 {
                    emit_note_screen(
                        out,
                        active[idx],
                        tick,
                        key,
                        track_idx,
                        color,
                        ppu,
                        cox,
                        lane_y,
                        key_h,
                        tick_start,
                        tick_end,
                    );
                }
                active[idx] = (tick as u32, vel, ch, true);
            }
            EventKind::NoteOn | EventKind::NoteOff if active[idx].3 => {
                emit_note_screen(
                    out,
                    active[idx],
                    tick,
                    key,
                    track_idx,
                    color,
                    ppu,
                    cox,
                    lane_y,
                    key_h,
                    tick_start,
                    tick_end,
                );
                active[idx].3 = false;
            }
            _ => {}
        }

        if (tick as f64) > tick_end {
            break;
        }
    }

    // 未关闭的音符
    if (last_tick as f64) > tick_start {
        for ch in 0..16u8 {
            for k in 0..=127u8 {
                let idx = (ch as usize) * 128 + (k as usize);
                if active[idx].3 {
                    emit_note_screen(
                        out,
                        active[idx],
                        last_tick as f32,
                        k,
                        track_idx,
                        color,
                        ppu,
                        cox,
                        lane_y,
                        key_h,
                        tick_start,
                        tick_end,
                    );
                }
            }
        }
    }
}

#[inline(always)]
fn emit_note_screen(
    out: &mut Vec<ArrangementNoteInstance>,
    note_state: (u32, u8, u8, bool),
    end_tick: f32,
    _key: u8,
    _track_idx: usize,
    color: [f32; 3],
    ppu: f32,
    cox: f32,
    lane_y: f32,
    _key_h: f32,
    tick_start: f64,
    tick_end: f64,
) {
    let s = (note_state.0 as f64).max(tick_start);
    let e = (end_tick as f64).min(tick_end);
    if s >= e {
        return;
    }

    let sx = cox + s as f32 * ppu;
    let sw = (e - s) as f32 * ppu;
    let sy = lane_y; // 所有音符在同一水平位置（音轨内）
    out.push(ArrangementNoteInstance::note(
        sx,
        sy,
        sw.max(2.0),
        4.0,
        color,
        note_state.1,
    ));
}

// ─── 辅助函数 ───

fn lane_col(track_idx: usize) -> (f32, f32, f32) {
    if track_idx % 2 == 0 {
        AR_LANE_EVEN_COLOR
    } else {
        AR_LANE_ODD_COLOR
    }
}

fn track_y(viewport: &ArrangementViewport, track_idx: usize) -> f32 {
    track_idx as f32 * viewport.track_height - viewport.scroll_y
}

fn visible_track_range(
    viewport: &ArrangementViewport,
    height: f32,
    num_tracks: usize,
) -> (usize, usize) {
    let first = ((viewport.scroll_y / viewport.track_height).floor() as usize)
        .min(num_tracks.saturating_sub(1));
    let count = (height / viewport.track_height).ceil() as usize + 1;
    (first, (first + count).min(num_tracks))
}

fn tick_to_x(viewport: &ArrangementViewport, tick: f64) -> f32 {
    viewport.canvas_offset.x + (tick as f32 * viewport.zoom_x) - viewport.scroll_x
}
