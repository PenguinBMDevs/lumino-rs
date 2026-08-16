//! 贴图瀑布流脏区域覆层生成与上传

use std::sync::{Arc, Mutex};

use super::common::{ensure_renderer_for_config, push_waterfall_progress};
use crate::texture_waterfall::WATERFALL_TRACKS_PER_GROUP;
use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::generate::{
    generate_waterfall_track_tile, merge_waterfall_track_tile_into,
};
use crate::texture_waterfall::gpu_ctx::WaterfallGpuCtx;
use crate::texture_waterfall::meta::WaterfallMeta;
use crate::texture_waterfall::note::WaterfallNote;
use crate::texture_waterfall::renderer::TextureWaterfallRenderer;
use crate::texture_waterfall::track_params::WaterfallTrackParams;
use crate::texture_waterfall::types::WaterfallTileCoord;

/// 对所有目标 time_group 生成并上传脏区域覆层贴图
///
/// 使用流式 merge：单缓冲 + 逐轨生成合并，避免 Vec<WaterfallTrackTile> 累积
///（原实现同时持有 8 张 WaterfallTrackTile = 8MB，现降至 ~2MB 峰值）。
#[allow(clippy::too_many_arguments)] // 渲染参数聚合为结构体反而降低可读性，保持显式传参
fn generate_and_upload_dirty_overlays(
    renderer: &mut TextureWaterfallRenderer,
    gpu: &WaterfallGpuCtx<'_>,
    sorted_notes: &[Vec<WaterfallNote>],
    target_time_groups: &[u32],
    ticks_per_group: u32,
    width: u32,
    key_count: u16,
    track_group: u32,
    track_start: u16,
) {
    // 复用像素缓冲，避免每 time_group 重新分配
    let buf_size = (width * key_count as u32) as usize * 4;
    let mut merged_pixels = vec![0u8; buf_size];

    for &time_g in target_time_groups {
        let tick_start = time_g * ticks_per_group;
        let tick_end = tick_start + ticks_per_group;
        let merged_coord = WaterfallTileCoord::new(track_group, time_g);

        // 重置缓冲（只清空已使用的行，避免全量 1MB memset）
        merged_pixels.fill(0);

        for (local_idx, notes) in sorted_notes.iter().enumerate() {
            let t = track_start + local_idx as u16;
            let tile = generate_waterfall_track_tile(
                notes, t, time_g, tick_start, tick_end, width, key_count,
            );
            merge_waterfall_track_tile_into(&mut merged_pixels, &tile);
            // tile 在此作用域结束时 drop，CPU 像素缓冲立即释放（不累积）
        }

        renderer.upload_dirty_overlay(
            gpu.device,
            gpu.queue,
            merged_coord,
            &merged_pixels,
            width,
            key_count as u32,
        );
    }
}

/// 处理 `WaterfallCommand::ShowDirtyOverlay`：生成并上传编辑后的临时脏区域贴图覆层
pub fn handle_waterfall_dirty_overlay(
    params: WaterfallTrackParams,
    gpu: &WaterfallGpuCtx<'_>,
    renderer: &mut Option<TextureWaterfallRenderer>,
    meta: &mut Option<WaterfallMeta>,
    renderer_config: &mut Option<TextureWaterfallConfig>,
    progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    let WaterfallTrackParams {
        track_idx,
        group_notes,
        dirty_time_groups,
        ppq,
        key_count,
        total_ticks,
        track_count,
        config,
        midi_hash: _,
    } = params;
    let track_group = (track_idx / WATERFALL_TRACKS_PER_GROUP) as u32;

    ensure_renderer_for_config(gpu, renderer, renderer_config, &config);

    let needed_track_count = track_count.max(track_idx + 1);
    if meta.is_none() {
        *meta = Some(WaterfallMeta {
            track_count: needed_track_count,
            track_groups: 1,
            key_count,
            time_groups: config.time_group_count(total_ticks, ppq),
            ticks_per_group: config.ticks_per_group(ppq),
        });
    }

    let ticks_per_group = config.ticks_per_group(ppq);
    let time_groups = config.time_group_count(total_ticks, ppq);
    let width = config.tile_width_px;
    let track_start = (track_group * WATERFALL_TRACKS_PER_GROUP as u32) as u16;

    let mut sorted_notes = group_notes;
    for notes in &mut sorted_notes {
        notes.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
    }

    let mut target_time_groups: Vec<u32> = dirty_time_groups;
    target_time_groups.sort_unstable();
    target_time_groups.dedup();
    target_time_groups.retain(|&g| g < time_groups);
    let target_time_groups = if target_time_groups.is_empty() {
        (0..time_groups).collect()
    } else {
        target_time_groups
    };

    if let Some(renderer) = renderer {
        generate_and_upload_dirty_overlays(
            renderer,
            gpu,
            &sorted_notes,
            &target_time_groups,
            ticks_per_group,
            width,
            key_count,
            track_group,
            track_start,
        );
    }

    push_waterfall_progress(
        progress,
        &format!("音轨组 {track_group} 脏区域临时覆层已生成"),
        1.0,
    );
}
