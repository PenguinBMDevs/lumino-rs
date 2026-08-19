//! 瀑布流模式参数

use lumino_extras::palette::current_track_color_f32;
use lumino_gfx::{RenderParams, WaterfallNoteGpu, pack_color};

use super::{WaterfallRenderInput, collect_visible_notes_for_gpu};

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
    } = input;
    let waterfall_width = width.max(1) as f32;
    let waterfall_height = height.max(1) as f32;
    let mut notes = Vec::new();
    collect_visible_notes_for_gpu(
        document,
        tick,
        ppq,
        key_count,
        waterfall_scroll_speed,
        1.0,
        &mut notes,
    );

    let mut waterfall_notes = Vec::with_capacity(notes.len());
    for n in &notes {
        let color_packed = pack_color(current_track_color_f32(n.track_idx as usize));
        waterfall_notes.push(WaterfallNoteGpu {
            key: n.key as u32,
            start_tick: n.start_tick,
            end_tick: n.end_tick,
            color_packed,
        });
    }

    // 按 key 计数分桶（O(N)），替代 O(N log N) 全量排序：
    // 高密集度段落（单帧 10W+ 音符）排序是每帧 CPU 热点，分桶省去 log 因子。
    // 偏移表语义与原实现一致：`offsets[k]` = 第一个 `key >= k` 的音符索引，
    // 桶 k 的区间为 `[offsets[k], offsets[k+1])`，空桶区间自然为空。
    // 桶内按 start_tick 稳定排序（保持同轨收集顺序，叠音颜色与旧实现一致），
    // 满足 shader 桶内二分回溯的前提。
    let key_count_usize = key_count as usize;
    let mut counts = vec![0u32; key_count_usize];
    for n in &waterfall_notes {
        counts[n.key as usize] += 1;
    }
    let mut waterfall_key_offsets = vec![0u32; key_count_usize + 1];
    for k in 0..key_count_usize {
        waterfall_key_offsets[k + 1] = waterfall_key_offsets[k] + counts[k];
    }
    // 稳定分发（保持同轨内 start_tick 序），桶内再按 start_tick 稳定排序
    let mut sorted_notes = vec![
        WaterfallNoteGpu {
            key: 0,
            start_tick: 0,
            end_tick: 0,
            color_packed: 0,
        };
        waterfall_notes.len()
    ];
    let mut cursor = waterfall_key_offsets[..key_count_usize].to_vec();
    for n in &waterfall_notes {
        let k = n.key as usize;
        sorted_notes[cursor[k] as usize] = *n;
        cursor[k] += 1;
    }
    let mut seg_start = 0usize;
    for k in 0..key_count_usize {
        let seg_end = waterfall_key_offsets[k + 1] as usize;
        sorted_notes[seg_start..seg_end].sort_by_key(|n| n.start_tick);
        seg_start = seg_end;
    }
    waterfall_notes = sorted_notes;

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (waterfall_width, waterfall_height),
        scale_factor: 1.0,
        ppq: ppq as f32,
        max_key_index: (key_count.saturating_sub(1)) as f32,
        canvas_size: (waterfall_width, waterfall_height),
        is_waterfall_mode: true,
        waterfall_speed: waterfall_scroll_speed.max(0.1),
        waterfall_notes,
        waterfall_key_offsets,
        waterfall_current_tick: tick,
        time_signatures: document.time_signatures.clone(),
        ..Default::default()
    }
}
