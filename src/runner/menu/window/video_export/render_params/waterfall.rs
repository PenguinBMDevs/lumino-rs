//! 瀑布流模式参数（产出统一 `note_instances` + 瀑布流 uniforms）
//!
//! 单一权威飞行格式：收集窗口可见音符并打包为 `NoteInstance`。
//! 注意：此处必须保留逐帧窗口过滤（而非首帧全量）——shader 桶内二分回溯
//! 上限（SEARCH_BUFFER=128）是按窗口过滤后的桶标定的；全量历史入桶会污染
//! 回溯预算，导致长音尾部在 dense 段丢失像素。窗口公式与 shader 侧一致。

use lumino_gfx::RenderParams;

use super::{WaterfallRenderInput, collect_window_notes, pack_note_instances};

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
        window_state,
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

    // 滑动窗口收集（O(窗口变化量)，tick 单调递增下游标只推进；输出与旧逐帧全前缀扫描一致）。
    let t_collect = std::time::Instant::now();
    collect_window_notes(
        document,
        tick_start,
        tick_end,
        key_count,
        window_state,
        visible_notes,
    );
    let collect_us = t_collect.elapsed().as_micros() as u64;

    let t_sort = std::time::Instant::now();
    super::sort_visible_notes(visible_notes, &mut window_state.sort_scratch);
    let sort_us = t_sort.elapsed().as_micros() as u64;
    // 边框仅钢琴卷帘矩形管线使用，瀑布流换算忽略，填 0。
    let t_pack = std::time::Instant::now();
    pack_note_instances(visible_notes, 0, note_instances_out);
    let pack_us = t_pack.elapsed().as_micros() as u64;
    super::diag_window_collect(
        "waterfall",
        collect_us,
        sort_us,
        pack_us,
        visible_notes.len(),
    );

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
