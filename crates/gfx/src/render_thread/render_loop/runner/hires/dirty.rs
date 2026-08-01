use std::sync::{Arc, Mutex};

use crate::render_thread::HiResTrackParams;
use crate::{HiResConfig, HiResRenderer, TRACKS_PER_GROUP, TileCoord, generate_track_tile};
use lumino_onion_skin::OnionSkinNote;

use super::super::context::RenderContext;
use super::super::types::HiResMeta;
use super::common::{ensure_renderer_for_config, push_onion_progress};

/// 对所有目标 time_group 生成并上传脏区域覆层贴图
fn generate_and_upload_dirty_overlays(
    renderer: &mut HiResRenderer,
    ctx: &RenderContext,
    sorted_notes: &[Vec<OnionSkinNote>],
    target_time_groups: &[u32],
    ticks_per_group: u32,
    width: u32,
    key_count: u16,
    track_group: u32,
    track_start: u16,
    track_end: u16,
    track_count: u16,
) {
    for &time_g in target_time_groups {
        let tick_start = time_g * ticks_per_group;
        let tick_end = tick_start + ticks_per_group;
        let merged_coord = TileCoord::new(track_group, time_g);

        let mut track_tiles = Vec::with_capacity(sorted_notes.len());
        for (local_idx, notes) in sorted_notes.iter().enumerate() {
            let t = track_start + local_idx as u16;
            let tile =
                generate_track_tile(notes, t, time_g, tick_start, tick_end, width, key_count);
            track_tiles.push(tile);
        }

        let group_tile = crate::merge_group_tiles(
            &track_tiles,
            merged_coord,
            tick_start,
            tick_end,
            width,
            key_count,
            (track_start, track_end),
        );

        renderer.upload_dirty_overlay(
            &ctx.device,
            &ctx.queue,
            merged_coord,
            &group_tile.pixels,
            group_tile.width,
            group_tile.height,
        );
    }
}

/// 处理 ShowHiResDirtyOverlay：生成并上传编辑后的临时脏区域贴图覆层
pub(crate) fn handle_show_dirty_overlay(
    params: HiResTrackParams,
    ctx: &RenderContext,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    let HiResTrackParams {
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
    let track_group = (track_idx / TRACKS_PER_GROUP) as u32;

    ensure_renderer_for_config(ctx, hires_renderer, hires_config, &config);

    let needed_track_count = track_count.max(track_idx + 1);
    if hires_meta.is_none() {
        *hires_meta = Some(HiResMeta {
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
    let track_start = (track_group * TRACKS_PER_GROUP as u32) as u16;
    let track_end = (track_start as u32 + group_notes.len() as u32).min(track_count as u32) as u16;

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

    if let Some(renderer) = hires_renderer {
        generate_and_upload_dirty_overlays(
            renderer,
            ctx,
            &sorted_notes,
            &target_time_groups,
            ticks_per_group,
            width,
            key_count,
            track_group,
            track_start,
            track_end,
            track_count,
        );
    }

    push_onion_progress(
        onion_progress,
        &format!("音轨组 {track_group} 脏区域临时覆层已生成"),
        1.0,
    );
}
