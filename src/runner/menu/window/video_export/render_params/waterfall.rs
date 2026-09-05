//! 瀑布流模式参数（产出统一 `note_instances` + 瀑布流 uniforms）
//!
//! 单一权威飞行格式：收集窗口可见音符并打包为 `NoteInstance`。
//! 注意：此处必须保留逐帧窗口过滤（而非首帧全量）——shader 桶内二分回溯
//! 上限（SEARCH_BUFFER=128）是按窗口过滤后的桶标定的；全量历史入桶会污染
//! 回溯预算，导致长音尾部在 dense 段丢失像素。窗口公式与 shader 侧一致。

use lumino_gfx::RenderParams;

use super::{SortableNote, WaterfallRenderInput, note_search_bounds, pack_note_instances};

/// 瀑布流模式参数
pub(crate) fn build_waterfall_render_params(input: WaterfallRenderInput) -> RenderParams {
    let WaterfallRenderInput {
        width,
        height,
        tick,
        document,
        ppq,
        key_count,
        waterfall_scroll_speed,
        visible_notes,
        note_instances_out,
    } = input;
    let waterfall_width = width.max(1) as f32;
    let waterfall_height = height.max(1) as f32;

    // 可见 tick 窗口与 shader 侧 `viewport_tick_span` 同公式（速度越高窗口越窄）。
    let speed = waterfall_scroll_speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1);
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span);

    // 每轨按 start_tick 有序 → 二分窗口定位，避免每帧 O(N) 全量遍历
    visible_notes.clear();
    note_instances_out.clear();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        if track_notes.is_empty() {
            continue;
        }
        let (_, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
        for n in track_notes.iter().take(search_end) {
            if n.end_tick > tick_start && n.start_tick < tick_end && n.key < key_count as u8 {
                visible_notes.push(SortableNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    length: n.end_tick.saturating_sub(n.start_tick),
                    track_idx: track_idx as u16,
                });
            }
        }
    }

    super::sort_visible_notes(visible_notes);
    // 边框仅钢琴卷帘矩形管线使用，瀑布流换算忽略，填 0。
    pack_note_instances(visible_notes, 0, note_instances_out);

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (waterfall_width, waterfall_height),
        scale_factor: 1.0,
        ppq: ppq as f32,
        max_key_index: (key_count.saturating_sub(1)) as f32,
        canvas_size: (waterfall_width, waterfall_height),
        is_waterfall_mode: true,
        waterfall_speed: waterfall_scroll_speed.max(0.1),
        waterfall_current_tick: tick,
        note_instances: std::mem::take(note_instances_out),
        time_signatures: document.time_signatures.clone(),
        ..Default::default()
    }
}
