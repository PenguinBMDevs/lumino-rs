use std::sync::{Arc, Mutex};

use crate::{
    GroupTile, HiResConfig, HiResProgressCallback, HiResRenderMode, HiResRenderer, HiResUniform,
    TRACKS_PER_GROUP, TileCoord, generate_track_tile,
};
use lumino_onion_skin::OnionSkinNote;
use lumino_onion_skin_hires::{CacheMeta, merge_track_tile_into, read_track_tile_cache};

use super::super::super::commands::{ControlCommand, HiResTrackParams};
use super::super::super::params::RenderParams;
use super::context::{RenderContext, UploadHiResTileParams};
use super::types::{HiResMeta, HiResStreamMsg};

/// 向共享进度缓冲推送一条进度（渲染线程 → UI 线程）
pub(super) fn push_onion_progress(
    progress: &Arc<Mutex<Vec<(String, f32)>>>,
    msg: &str,
    value: f32,
) {
    if let Ok(mut buf) = progress.lock() {
        buf.push((msg.to_string(), value.clamp(0.0, 1.0)));
    }
}

/// 确保高精度渲染器与配置已初始化（懒初始化）。
fn ensure_renderer_for_config(
    ctx: &RenderContext,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_config: &mut Option<HiResConfig>,
    config: &HiResConfig,
) {
    if hires_renderer.is_none() {
        *hires_renderer = Some(HiResRenderer::new(
            &ctx.device,
            config.clone(),
            ctx.texture_format,
        ));
    }
    *hires_config = Some(config.clone());
}

/// 处理高精度洋葱皮控制命令（分发器，各命令逻辑在独立函数中）
pub(super) fn handle_hires_control(
    cmd: ControlCommand,
    ctx: &RenderContext,
    hires_result_tx: &std::sync::mpsc::SyncSender<HiResStreamMsg>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
) {
    match cmd {
        ControlCommand::GenerateHiResOnionSkin {
            notes,
            ppq,
            key_count,
            total_ticks,
            config,
            midi_hash,
        } => handle_generate_hires(
            notes, ppq, key_count, total_ticks, config, midi_hash,
            ctx, hires_result_tx, onion_progress,
            hires_renderer, hires_meta, hires_config,
        ),
        ControlCommand::DisposeHiResOnionSkin => handle_dispose_hires(
            hires_renderer, hires_meta, hires_config, onion_progress,
        ),
        ControlCommand::RegenerateHiResTrack(params) => handle_regenerate_hires_track(
            params,
            ctx, hires_result_tx,
            hires_renderer, hires_meta, hires_config,
        ),
        ControlCommand::ShowHiResDirtyOverlay(params) => handle_show_dirty_overlay(
            params,
            ctx,
            hires_renderer, hires_meta, hires_config, onion_progress,
        ),
        // Resize / Shutdown 已在命令分发阶段处理，此处无需重复处理
        _ => {}
    }
}

// ── 后台流式全轨生成 ──────────────────────────────────────

/// 处理 GenerateHiResOnionSkin：后台线程流式生成高精度洋葱皮贴图
fn handle_generate_hires(
    mut notes: Vec<Vec<OnionSkinNote>>,
    ppq: u16,
    key_count: u16,
    total_ticks: u32,
    config: HiResConfig,
    midi_hash: String,
    ctx: &RenderContext,
    hires_result_tx: &std::sync::mpsc::SyncSender<HiResStreamMsg>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
) {
    // 创建/重建高精度渲染器
    ensure_renderer_for_config(ctx, hires_renderer, hires_config, &config);

    // 元数据必须在后台线程启动前设置（regen 安全）
    let track_count = notes.len() as u16;
    let time_groups = config.time_group_count(total_ticks, ppq);
    let ticks_per_group = config.ticks_per_group(ppq);
    *hires_meta = Some(HiResMeta {
        track_count,
        track_groups: 1,
        key_count,
        time_groups,
        ticks_per_group,
    });

    push_onion_progress(onion_progress, "正在后台生成高精度洋葱皮贴图\u{2026}", 0.0);

    // 后台线程流式生成（time_group 同步推进），merge 在后台完成，渲染线程仅 upload
    let progress_buf = onion_progress.clone();
    let tx = Arc::new(Mutex::new(hires_result_tx.clone()));
    let tile_width = config.tile_width_px;
    let tile_height = key_count as u32;
    std::thread::spawn(move || {
        let cb: HiResProgressCallback = Arc::new(move |msg, pct| {
            if let Ok(mut buf) = progress_buf.lock() {
                buf.push((msg.to_string(), pct.clamp(0.0, 1.0)));
            }
        });

        let time_group_cb = {
            let tx = tx.clone();
            let tw = tile_width;
            let th = tile_height;
            move |time_group: u32, tile: GroupTile| {
                if let Ok(guard) = tx.lock() {
                    let track_group = tile.coord.track_group;
                    let _ = guard.send(HiResStreamMsg::TimeGroupMerged {
                        track_group,
                        time_group,
                        pixels: tile.pixels,
                        width: tw,
                        height: th,
                    });
                }
            }
        };
        lumino_onion_skin_hires::generate_all_tiles_streaming(
            &mut notes,
            &config,
            ppq,
            key_count,
            total_ticks,
            &midi_hash,
            Some(cb),
            &time_group_cb,
        );

        // 全部生成完毕
        if let Ok(guard) = tx.lock() {
            let _ = guard.send(HiResStreamMsg::Finished);
        }
    });
}

// ── 释放高精度洋葱皮资源 ──────────────────────────────────

/// 处理 DisposeHiResOnionSkin：释放高精度渲染器与元数据
fn handle_dispose_hires(
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    *hires_renderer = None;
    *hires_meta = None;
    *hires_config = None;
    push_onion_progress(onion_progress, "高精度洋葱皮资源已释放", 1.0);
}

// ── 音轨重生成 ────────────────────────────────────────────

/// 处理 RegenerateHiResTrack：重生成指定音轨的高精度贴图（编辑后冷静期到期触发）
fn handle_regenerate_hires_track(
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
        ppq,
        key_count,
        total_ticks,
        track_count,
        config,
        midi_hash,
    } = params;
    let track_group = (track_idx / TRACKS_PER_GROUP) as u32;
    tracing::debug!(
        "[onion-render] RegenerateHiResTrack: track={}, track_group={}, group_tracks={}, track_count={}, meta_exists={}",
        track_idx,
        track_group,
        group_notes.len(),
        track_count,
        hires_meta.is_some()
    );

    ensure_renderer_for_config(ctx, hires_renderer, hires_config, &config);

    // 若元数据不存在（未执行过全曲生成），用命令参数重建元数据
    if hires_meta.is_none() {
        let time_groups = config.time_group_count(total_ticks, ppq);
        let ticks_per_group = config.ticks_per_group(ppq);
        *hires_meta = Some(HiResMeta {
            track_count,
            track_groups: 1,
            key_count,
            time_groups,
            ticks_per_group,
        });
    }

    let ticks_per_group = config.ticks_per_group(ppq);
    let time_groups = config.time_group_count(total_ticks, ppq);
    let width = config.tile_width_px;
    let track_start = (track_group * TRACKS_PER_GROUP as u32) as u16;

    // 跨 track_group 全轨合并：其他音轨组读取硬盘缓存
    let all_track_groups = config.track_group_count(track_count);
    let measures_per_group = config.measures_per_group;
    let cache_dir = config.cache_dir.clone();
    let mh = midi_hash.clone();
    let tx = Arc::new(Mutex::new(hires_result_tx.clone()));
    std::thread::spawn(move || {
        let mut sorted_notes = group_notes;
        for notes in &mut sorted_notes {
            notes.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
        }
        tracing::debug!(
            "[onion-render] RegenerateHiResTrack 后台线程启动: track_group={}, all_groups={}, time_groups={}",
            track_group,
            all_track_groups,
            time_groups
        );

        for time_g in 0..time_groups {
            let tick_start = time_g * ticks_per_group;
            let tick_end = tick_start + ticks_per_group;
            let buf_size = (width * key_count as u32) as usize * 4;
            let mut merged_pixels = vec![0u8; buf_size];

            for tg in 0..all_track_groups {
                let tg_start = (tg * TRACKS_PER_GROUP as u32) as u16;
                let tg_end =
                    ((tg + 1) * TRACKS_PER_GROUP as u32).min(track_count as u32) as u16;

                if tg == track_group {
                    // 修改音轨组：使用内存中的最新音符生成
                    for (local_idx, notes) in sorted_notes.iter().enumerate() {
                        let t = track_start + local_idx as u16;
                        let tile = generate_track_tile(
                            notes, t, time_g, tick_start, tick_end, width, key_count,
                        );
                        merge_track_tile_into(&mut merged_pixels, &tile);
                    }
                } else {
                    // 其他音轨组：读取硬盘缓存
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
                        if let Ok(Some(tile)) = read_track_tile_cache(
                            &cache_dir,
                            &mh,
                            t,
                            time_g,
                            &expected_meta,
                        ) {
                            merge_track_tile_into(&mut merged_pixels, &tile);
                        }
                    }
                }
            }

            if let Ok(guard) = tx.lock() {
                let _ = guard.send(HiResStreamMsg::TimeGroupMerged {
                    track_group: 0,
                    time_group: time_g,
                    pixels: merged_pixels,
                    width,
                    height: key_count as u32,
                });
            }

            tracing::debug!(
                "[onion-render] RegenerateHiResTrack 进度: {}/{} ({:.1}%)",
                time_g + 1,
                time_groups,
                (time_g as f32 + 1.0) / time_groups as f32 * 100.0
            );
        }

        if let Ok(guard) = tx.lock() {
            let _ = guard.send(HiResStreamMsg::Finished);
        }
        tracing::debug!(
            "[onion-render] RegenerateHiResTrack 后台全轨合并完成: track_group={}",
            track_group
        );
    });
}

// ── 脏区域临时覆层 ────────────────────────────────────────

/// 处理 ShowHiResDirtyOverlay：生成并上传编辑后的临时脏区域贴图覆层
fn handle_show_dirty_overlay(
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
        ppq,
        key_count,
        total_ticks,
        track_count,
        config,
        midi_hash: _,
    } = params;
    let track_group = (track_idx / TRACKS_PER_GROUP) as u32;
    tracing::debug!(
        "[onion-render] ShowHiResDirtyOverlay: track={}, track_group={}, group_tracks={}, meta_exists={}",
        track_idx,
        track_group,
        group_notes.len(),
        hires_meta.is_some()
    );

    ensure_renderer_for_config(ctx, hires_renderer, hires_config, &config);

    // 干净启动 / 新建工程时元数据可能为空，必须初始化
    let needed_track_count = track_count.max(track_idx + 1);
    if hires_meta.is_none() {
        let time_groups = config.time_group_count(total_ticks, ppq);
        let ticks_per_group = config.ticks_per_group(ppq);
        *hires_meta = Some(HiResMeta {
            track_count: needed_track_count,
            track_groups: 1,
            key_count,
            time_groups,
            ticks_per_group,
        });
    }

    let ticks_per_group = config.ticks_per_group(ppq);
    let time_groups = config.time_group_count(total_ticks, ppq);
    let width = config.tile_width_px;
    let track_start = (track_group * TRACKS_PER_GROUP as u32) as u16;
    let track_end =
        (track_start as u32 + group_notes.len() as u32).min(track_count as u32) as u16;
    let track_range = (track_start, track_end);

    // 排序音符（保证 time_group 内的合并顺序稳定）
    let mut sorted_notes = group_notes;
    for notes in &mut sorted_notes {
        notes.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
    }

    if let Some(renderer) = hires_renderer {
        for time_g in 0..time_groups {
            let tick_start = time_g * ticks_per_group;
            let tick_end = tick_start + ticks_per_group;
            let merged_coord = TileCoord::new(0, time_g);

            let mut track_tiles = Vec::with_capacity(sorted_notes.len());
            for (local_idx, notes) in sorted_notes.iter().enumerate() {
                let t = track_start + local_idx as u16;
                let tile = generate_track_tile(
                    notes, t, time_g, tick_start, tick_end, width, key_count,
                );
                track_tiles.push(tile);
            }

            let group_tile = crate::merge_group_tiles(
                &track_tiles,
                merged_coord,
                tick_start,
                tick_end,
                width,
                key_count,
                track_range,
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
        tracing::debug!(
            "[onion-render] ShowHiResDirtyOverlay: 已上传 {} 个覆层贴图 (track_group={}), time_groups={}",
            time_groups,
            track_group,
            time_groups,
        );
    }

    push_onion_progress(
        onion_progress,
        &format!("音轨组 {track_group} 脏区域临时覆层已生成"),
        1.0,
    );
}

/// 上传视频导出预生成的高精度贴图，并初始化渲染器与元数据
pub(super) fn upload_hires_video_tiles(
    ctx: &RenderContext,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    params: UploadHiResTileParams,
) {
    let mut renderer = HiResRenderer::new(&ctx.device, params.config.clone(), ctx.texture_format);
    for tile in params.tiles {
        renderer.upload_tile(
            &ctx.device,
            &ctx.queue,
            tile.coord,
            &tile.pixels,
            tile.width,
            tile.height,
        );
    }

    let time_groups = params
        .config
        .time_group_count(params.total_ticks, params.ppq);
    let ticks_per_group = params.config.ticks_per_group(params.ppq);
    let track_groups = params.config.track_group_count(params.track_count);

    tracing::info!(
        "视频导出 HiRes 贴图上传完成: {} 张, track_groups={}, time_groups={}",
        renderer.tile_count(),
        track_groups,
        time_groups
    );

    *hires_renderer = Some(renderer);
    *hires_config = Some(params.config);
    *hires_meta = Some(HiResMeta {
        track_count: params.track_count,
        track_groups,
        key_count: params.key_count,
        time_groups,
        ticks_per_group,
    });
}

/// 高精度贴图视口驱动：准备 uniform
pub(super) fn update_hires_viewport(
    renderer: &mut Option<HiResRenderer>,
    meta: &Option<HiResMeta>,
    config: &Option<HiResConfig>,
    params: &RenderParams,
    _device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Vec<(TileCoord, HiResUniform)> {
    let mut visible: Vec<(TileCoord, HiResUniform)> = Vec::new();
    let (Some(renderer), Some(config), Some(meta)) = (renderer, config, meta) else {
        return visible;
    };
    if !config.enabled || params.is_arrangement_mode {
        return visible;
    }

    let scale = params.scale_factor;
    let zoom_x = params.zoom.0;
    let zoom_y = params.zoom.1;
    if zoom_x <= 0.0 || zoom_y <= 0.0 {
        return visible;
    }

    let ticks_per_group = meta.ticks_per_group;
    if ticks_per_group == 0 {
        return visible;
    }

    // 可见 tick 范围 → 时间组索引范围
    let scroll_x = params.scroll.0;
    let canvas_w_logical = params.canvas_size.0;
    let t_start = (scroll_x / zoom_x).max(0.0) as u32;
    let t_end = ((scroll_x + canvas_w_logical) / zoom_x) as u32;

    let g_start = t_start / ticks_per_group;
    let g_end = (t_end / ticks_per_group).saturating_add(1);

    let key_count = meta.key_count;

    let base_x = (params.canvas_offset.0 + params.keyboard_width) * scale;
    let scroll_y = params.scroll.1;
    let area_y = (params.canvas_offset.1 + params.ruler_height - scroll_y) * scale;
    let area_h = key_count as f32 * zoom_y * scale;
    let canvas_w = params.viewport_size.0 as f32;
    let canvas_h = params.viewport_size.1 as f32;

    // 遍历所有音轨组与时间组，收集有正常贴图或临时覆层的可见坐标
    for track_g in 0..meta.track_groups {
        for time_g in g_start..g_end {
            let coord = TileCoord::new(track_g, time_g);
            if renderer.has_tile_or_dirty_overlay(&coord) {
                let tick_start = time_g * ticks_per_group;
                let (area_x, area_w) = match config.render_mode {
                    HiResRenderMode::Native => {
                        let texture_zoom = config.tile_width_px as f32 / ticks_per_group as f32;
                        let tick_offset = scroll_x / zoom_x;
                        let area_x =
                            base_x + (tick_start as f32 - tick_offset) * texture_zoom * scale;
                        let area_w = config.tile_width_px as f32 * scale;
                        (area_x, area_w)
                    }
                    HiResRenderMode::Stretch => {
                        let area_x = base_x + (tick_start as f32 * zoom_x - scroll_x) * scale;
                        let area_w = ticks_per_group as f32 * zoom_x * scale;
                        (area_x, area_w)
                    }
                };
                let uniform = HiResUniform::new(area_x, area_y, area_w, area_h, canvas_w, canvas_h);
                visible.push((coord, uniform));
            }
        }
    }

    renderer.prepare(queue, &visible);
    renderer.prepare_dirty_overlays(queue, &visible);
    visible
}
