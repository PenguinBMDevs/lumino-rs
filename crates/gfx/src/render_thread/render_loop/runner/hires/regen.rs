use std::sync::{Arc, Mutex};

use crate::render_thread::HiResTrackParams;
use crate::{HiResConfig, HiResRenderer, TRACKS_PER_GROUP, generate_track_tile};
use lumino_onion_skin::OnionSkinNote;
use lumino_midiplayer::{CacheMeta, merge_track_tile_into, read_track_tile_cache};

use super::super::context::RenderContext;
use super::super::types::{HiResMeta, HiResStreamMsg};
use super::common::ensure_renderer_for_config;

/// 合并单个 time_group 中所有音轨组的像素缓冲到 `output` 中
///
/// `output` 长度必须为 `width × key_count × 4`，函数会用 `fill(0)` 重置。
fn regen_time_group_merged_pixels_into(
    output: &mut [u8],
    time_g: u32,
    ticks_per_group: u32,
    track_group: u32,
    all_track_groups: u32,
    sorted_notes: &[Vec<OnionSkinNote>],
    track_count: u16,
    track_start: u16,
    mh: &str,
    width: u32,
    key_count: u16,
    ppq: u16,
    measures_per_group: u32,
    cache_dir: &std::path::Path,
) {
    output.fill(0);
    let tick_start = time_g * ticks_per_group;
    let tick_end = tick_start + ticks_per_group;

    for tg in 0..all_track_groups {
        let tg_start = (tg * TRACKS_PER_GROUP as u32) as u16;
        let tg_end = ((tg + 1) * TRACKS_PER_GROUP as u32).min(track_count as u32) as u16;

        if tg == track_group {
            for (local_idx, notes) in sorted_notes.iter().enumerate() {
                let t = track_start + local_idx as u16;
                let tile =
                    generate_track_tile(notes, t, time_g, tick_start, tick_end, width, key_count);
                merge_track_tile_into(output, &tile);
            }
        } else {
            for t in tg_start..tg_end {
                let expected_meta = CacheMeta {
                    track_idx: t,
                    time_group: time_g,
                    width,
                    height: key_count as u32,
                    tick_start,
                    tick_end,
                    key_count,
                    ppq,
                    measures_per_group,
                };
                if let Ok(Some(tile)) =
                    read_track_tile_cache(cache_dir, mh, t, time_g, &expected_meta)
                {
                    merge_track_tile_into(output, &tile);
                }
            }
        }
    }
}

/// 处理 RegenerateHiResTrack：重生成指定音轨的高精度贴图
pub(crate) fn handle_regenerate_hires_track(
    params: HiResTrackParams,
    ctx: &RenderContext,
    hires_result_tx: &std::sync::mpsc::SyncSender<HiResStreamMsg>,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
) {
    let HiResTrackParams {
        track_idx,
        group_notes,
        dirty_time_groups: _,
        ppq,
        key_count,
        total_ticks,
        track_count,
        config,
        midi_hash,
    } = params;
    let track_group = (track_idx / TRACKS_PER_GROUP) as u32;

    ensure_renderer_for_config(ctx, hires_renderer, hires_config, &config);

    if hires_meta.is_none() {
        *hires_meta = Some(HiResMeta {
            track_count,
            track_groups: 1,
            key_count,
            time_groups: config.time_group_count(total_ticks, ppq),
            ticks_per_group: config.ticks_per_group(ppq),
        });
    }

    let (width, track_start) = (
        config.tile_width_px,
        (track_group * TRACKS_PER_GROUP as u32) as u16,
    );
    let (ticks_per_group, time_groups) = (
        config.ticks_per_group(ppq),
        config.time_group_count(total_ticks, ppq),
    );
    let (all_track_groups, measures_per_group) = (
        config.track_group_count(track_count),
        config.measures_per_group,
    );
    let (cache_dir, mh) = (config.cache_dir.clone(), midi_hash.clone());
    let tx = Arc::new(Mutex::new(hires_result_tx.clone()));

    std::thread::spawn(move || {
        let mut notes = group_notes;
        for n in &mut notes {
            n.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
        }

        let buf_size = (width * key_count as u32) as usize * 4;
        for time_g in 0..time_groups {
            let mut merged_pixels = vec![0u8; buf_size];
            regen_time_group_merged_pixels_into(
                &mut merged_pixels,
                time_g,
                ticks_per_group,
                track_group,
                all_track_groups,
                &notes,
                track_count,
                track_start,
                &mh,
                width,
                key_count,
                ppq,
                measures_per_group,
                &cache_dir,
            );
            if let Ok(guard) = tx.lock() {
                let _ = guard.send(HiResStreamMsg::TimeGroupMerged {
                    track_group,
                    time_group: time_g,
                    pixels: merged_pixels,
                    width,
                    height: key_count as u32,
                });
            }
        }

        if let Ok(guard) = tx.lock() {
            let _ = guard.send(HiResStreamMsg::ClearDirtyOverlay(track_group));
        }
        if let Ok(guard) = tx.lock() {
            let _ = guard.send(HiResStreamMsg::Finished);
        }
    });
}
