//! GPU 可见音符收集（瀑布流 / MIDITrail 共用）

use lumino_midi_loader::MidiDocument;

use super::{GpuVisibleNote, note_search_bounds};

/// 收集 GPU 渲染所需的可见音符
pub(crate) fn collect_visible_notes_for_gpu(
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    key_count: u16,
    waterfall_scroll_speed: f32,
    viewport_scale: f32,
    out: &mut Vec<GpuVisibleNote>,
) {
    out.clear();
    let speed = waterfall_scroll_speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span =
        (ticks_per_measure * visible_measure_count).max(1) as f32 * viewport_scale;
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);

    // 每轨按 start_tick 有序 → 二分窗口定位，避免每帧 O(N) 全量遍历
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        if track_notes.is_empty() {
            continue;
        }
        let (_, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
        for n in track_notes.iter().take(search_end) {
            if n.end_tick > tick_start && n.start_tick < tick_end && n.key < key_count as u8 {
                out.push(GpuVisibleNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    end_tick: n.end_tick,
                    track_idx: track_idx as u16,
                    velocity: n.velocity,
                });
            }
        }
    }
}
