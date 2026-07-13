use std::sync::{Arc, Mutex};

use crate::{
    GroupTile, HiResConfig, HiResProgressCallback, HiResRenderMode, HiResRenderer, HiResUniform,
    TRACKS_PER_GROUP, TileCoord, generate_track_tile,
};
use lumino_onion_skin_hires::{CacheMeta, merge_track_tile_into, read_track_tile_cache};

use super::super::super::commands::{ControlCommand, HiResTrackParams};
use super::super::super::params::RenderParams;
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

/// 处理高精度洋葱皮控制命令
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_hires_control(
    cmd: ControlCommand,
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    hires_result_tx: &std::sync::mpsc::SyncSender<HiResStreamMsg>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
    texture_format: wgpu::TextureFormat,
) {
    match cmd {
        ControlCommand::GenerateHiResOnionSkin {
            mut notes,
            ppq,
            key_count,
            total_ticks,
            config,
            midi_hash,
        } => {
            // 创建/重建高精度渲染器
            *hires_renderer = Some(HiResRenderer::new(device, config.clone(), texture_format));
            *hires_config = Some(config.clone());

            // 元数据必须在后台线程启动前设置（regen 安全）
            // track_groups 固定为 1：流式生成已将全部 track_group 合并为一张全轨贴图
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
            // sync_channel(1) 有界背压：send 满了阻塞，等渲染线程消费后才继续下一个 time_group
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

                // time_group 回调：每生成一张整合组贴图立即发送，渲染线程上传到自己的坐标位置。
                // sync_channel(1) 有界：send 阻塞等渲染线程消费，背压防止 CPU 内存堆积。
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
        ControlCommand::DisposeHiResOnionSkin => {
            *hires_renderer = None;
            *hires_meta = None;
            *hires_config = None;
            push_onion_progress(onion_progress, "高精度洋葱皮资源已释放", 1.0);
        }
        // 重生成指定音轨的高精度贴图（编辑后冷静期到期触发）
        ControlCommand::RegenerateHiResTrack(params) => {
            let HiResTrackParams {
                track_idx,
                mut group_notes,
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
            // 若尚未创建高精度渲染器（干净启动 / 新建工程），用当前配置初始化
            if hires_renderer.is_none() {
                tracing::debug!("[onion-render] RegenerateHiResTrack: 初始化 HiResRenderer");
                *hires_renderer = Some(HiResRenderer::new(device, config.clone(), texture_format));
            }
            *hires_config = Some(config.clone());

            // 若元数据不存在（未执行过全曲生成），用命令参数重建元数据
            // track_groups 固定为 1：与流式生成的合并策略一致
            if hires_meta.is_none() {
                tracing::debug!("[onion-render] RegenerateHiResTrack: 初始化 hires_meta");
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

            // 跨 track_group 全轨合并：与流式生成一致，生成一张全轨合并贴图
            // 生成完成后通过已有 hires_result_tx 传回 (track_group=0)，渲染线程仅做 GPU upload。
            // 使用 Host 提供的 group_notes 生成修改音轨组，其他音轨组读取硬盘缓存。
            let all_track_groups = config.track_group_count(track_count);
            let measures_per_group = config.measures_per_group;
            let cache_dir = config.cache_dir.clone();
            let mh = midi_hash.clone();
            let tx = Arc::new(Mutex::new(hires_result_tx.clone()));
            std::thread::spawn(move || {
                for notes in &mut group_notes {
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
                            for (local_idx, notes) in group_notes.iter().enumerate() {
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
                                // 缓存未命中：跳过（该轨在当前时间组无数据）
                            }
                        }
                    }

                    // 全轨合并贴图以 track_group=0 发送
                    if let Ok(guard) = tx.lock() {
                        let _ = guard.send(HiResStreamMsg::TimeGroupMerged {
                            track_group: 0,
                            time_group: time_g,
                            pixels: merged_pixels,
                            width,
                            height: key_count as u32,
                        });
                    }

                    let pct = (time_g as f32 + 1.0) / time_groups as f32;
                    tracing::debug!(
                        "[onion-render] RegenerateHiResTrack 进度: {}/{} ({:.1}%)",
                        time_g + 1,
                        time_groups,
                        pct * 100.0
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
        // 显示编辑后的临时脏区域贴图覆层（切换音轨前立即触发）
        ControlCommand::ShowHiResDirtyOverlay(params) => {
            let HiResTrackParams {
                track_idx,
                mut group_notes,
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
            // 若尚未创建高精度渲染器，用当前配置初始化
            if hires_renderer.is_none() {
                tracing::debug!("[onion-render] ShowHiResDirtyOverlay: 初始化 HiResRenderer");
                *hires_renderer = Some(HiResRenderer::new(device, config.clone(), texture_format));
            }
            *hires_config = Some(config.clone());

            // 干净启动 / 新建工程时元数据可能为空，必须初始化
            // track_groups 固定为 1：与流式生成的全轨合并策略一致
            let needed_track_count = track_count.max(track_idx + 1);
            if hires_meta.is_none() {
                tracing::debug!("[onion-render] ShowHiResDirtyOverlay: 初始化 hires_meta");
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

            // 仅在渲染线程生成修改音轨组的贴图覆层（快速：8 轨，无磁盘 I/O）
            // 脏覆层上传到 (0, time_g) 以匹配全轨合并模型的坐标。
            // base tile (全轨合并) 始终绘制，dirty_overlay 通过 Alpha 混合叠加其上，
            // 未修改音轨的透明像素让 base tile 透出，已修改音轨的不透明像素覆盖 base tile。
            for notes in &mut group_notes {
                notes.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
            }

            if let Some(renderer) = hires_renderer {
                // 输出 group_notes 的统计信息，诊断空像素问题
                let note_counts: Vec<usize> = group_notes.iter().map(|v| v.len()).collect();
                tracing::info!(
                    "[onion-render] ShowHiResDirtyOverlay: track={}, note_counts={:?}, track_start={}, track_count={}, time_groups={}",
                    track_idx,
                    note_counts,
                    track_start,
                    track_count,
                    time_groups,
                );

                let uploaded = time_groups;
                for time_g in 0..time_groups {
                    let tick_start = time_g * ticks_per_group;
                    let tick_end = tick_start + ticks_per_group;
                    let merged_coord = TileCoord::new(0, time_g);

                    let mut track_tiles = Vec::with_capacity(group_notes.len());
                    for (local_idx, notes) in group_notes.iter().enumerate() {
                        let t = track_start + local_idx as u16;
                        let tile = generate_track_tile(
                            notes, t, time_g, tick_start, tick_end, width, key_count,
                        );
                        track_tiles.push(tile);
                    }

                    // 合并为整合组贴图（仅该 track_group 内）
                    let group_tile = crate::merge_group_tiles(
                        &track_tiles,
                        merged_coord,
                        tick_start,
                        tick_end,
                        width,
                        key_count,
                        track_range,
                    );

                    // 检查是否有非空像素
                    let has_non_empty = group_tile.pixels.iter().any(|&b| b != 0);
                    tracing::info!(
                        "[onion-render] ShowHiResDirtyOverlay: time_g={}/{}, pixels={}, has_non_empty={}",
                        time_g,
                        time_groups,
                        group_tile.pixels.len(),
                        has_non_empty
                    );

                    // 上传为脏覆层到 (0, time_g)
                    renderer.upload_dirty_overlay(
                        device,
                        _queue,
                        merged_coord,
                        &group_tile.pixels,
                        group_tile.width,
                        group_tile.height,
                    );
                }
                tracing::debug!(
                    "[onion-render] ShowHiResDirtyOverlay: 已上传 {} 个覆层贴图 (track_group={}), time_groups={}",
                    uploaded,
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
        // Resize / Shutdown 已在命令分发阶段处理，此处无需重复处理
        _ => {}
    }
}

/// 上传视频导出预生成的高精度贴图，并初始化渲染器与元数据
#[allow(clippy::too_many_arguments)]
pub(super) fn upload_hires_video_tiles(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    tiles: Vec<GroupTile>,
    config: HiResConfig,
    track_count: u16,
    key_count: u16,
    total_ticks: u32,
    ppq: u16,
    texture_format: wgpu::TextureFormat,
) {
    let mut renderer = HiResRenderer::new(device, config.clone(), texture_format);
    for tile in tiles {
        renderer.upload_tile(
            device,
            queue,
            tile.coord,
            &tile.pixels,
            tile.width,
            tile.height,
        );
    }

    let time_groups = config.time_group_count(total_ticks, ppq);
    let ticks_per_group = config.ticks_per_group(ppq);
    let track_groups = config.track_group_count(track_count);

    tracing::info!(
        "视频导出 HiRes 贴图上传完成: {} 张, track_groups={}, time_groups={}",
        renderer.tile_count(),
        track_groups,
        time_groups
    );

    *hires_renderer = Some(renderer);
    *hires_config = Some(config);
    *hires_meta = Some(HiResMeta {
        track_count,
        track_groups,
        key_count,
        time_groups,
        ticks_per_group,
    });
}

/// 高精度贴图视口驱动：准备 uniform
///
/// 遍历可见范围内的所有 (track_group, time_group) 组合。
/// 正常贴图与临时脏区域覆层使用同一套 uniform，覆层在正常贴图之后绘制。
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
                        // 原生模式：贴图以原生分辨率渲染，按正确速度均匀滚动
                        // texture_zoom = 贴图像素 / tick 数（原生像素每 tick）
                        let texture_zoom = config.tile_width_px as f32 / ticks_per_group as f32;
                        // scroll_x / zoom_x 将滚动偏移从逻辑像素转换为 tick
                        let tick_offset = scroll_x / zoom_x;
                        let area_x =
                            base_x + (tick_start as f32 - tick_offset) * texture_zoom * scale;
                        let area_w = config.tile_width_px as f32 * scale;
                        (area_x, area_w)
                    }
                    HiResRenderMode::Stretch => {
                        // 拉伸模式：贴图随 zoom_x 拉伸填充视口（当前默认行为）
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
