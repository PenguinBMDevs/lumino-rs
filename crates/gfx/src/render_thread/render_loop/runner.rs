use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::SwappableBuffer;
use crate::{
    CacheMeta, GroupTile, HiResConfig, HiResProgressCallback, HiResRenderer, HiResUniform,
    TRACKS_PER_GROUP, TileCoord, generate_track_tile, merge_group_tiles, read_track_tile_cache,
};

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::execute_render_pass;
use super::render_pass::update_stats;
use super::textures::ensure_textures;

/// 高精度贴图元数据（无像素数据，用于视口计算）
#[allow(dead_code)]
struct HiResMeta {
    track_count: u16,
    key_count: u16,
    time_groups: u32,
    ticks_per_group: u32,
}

/// 后台生成结果：后台线程通过 channel 发送给渲染线程
struct HiResPendingResult {
    /// 按 (track_group, time_group) 索引的未合并贴图
    tiles: HashMap<TileCoord, GroupTile>,
    /// 贴图规格
    width: u32,
    height: u32,
}

/// 合并同时间组的所有音轨组贴图为一张全轨贴图
///
/// 所有 tiles 的 width/height 必须一致。
/// 后音轨组覆盖前音轨组的重叠区（alpha > 0 时覆盖）。
fn merge_track_groups(tiles: &[&GroupTile], width: u32, height: u32) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 4];

    for tile in tiles {
        debug_assert_eq!(tile.width, width);
        debug_assert_eq!(tile.height, height);
        for (i, chunk) in tile.pixels.chunks_exact(4).enumerate() {
            if chunk[3] > 0 {
                let offset = i * 4;
                pixels[offset] = chunk[0];
                pixels[offset + 1] = chunk[1];
                pixels[offset + 2] = chunk[2];
                pixels[offset + 3] = chunk[3];
            }
        }
    }

    pixels
}

/// 运行渲染线程主循环
#[allow(clippy::too_many_arguments)]
pub fn run_render_thread(
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    running: Arc<AtomicBool>,
    command_receiver: std::sync::mpsc::Receiver<RenderCommand>,
    latest_texture_clone: Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    stats_clone: Arc<Mutex<RenderStats>>,
    note_events_rx: std::sync::mpsc::Receiver<crate::NoteEvent>,
    note_instances_buffer: Arc<SwappableBuffer<crate::NoteInstance>>,
    onion_progress: Arc<Mutex<Vec<(String, f32)>>>,
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut grid_renderer = crate::GridRenderer::new(&device, texture_format);
    let mut note_renderer = crate::NoteRenderer::new(&device, &queue, texture_format);
    let mut ruler_renderer = crate::RulerRenderer::new(&device, texture_format);
    let mut arrangement_renderer = crate::ArrangementRenderer::new(&device, texture_format);
    let mut cc_bar_renderer = crate::CcBarRenderer::new(&device, texture_format);

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);
    let mut last_note_version: u64 = 0;

    // 高精度洋葱皮渲染器状态
    let mut hires_renderer: Option<HiResRenderer> = None;
    let mut hires_meta: Option<HiResMeta> = None;
    let mut hires_config: Option<HiResConfig> = None;
    let mut deferred: Vec<ControlCommand> = Vec::new();

    // ★ 后台生成线程通过 channel 传回结果 ★
    let (hires_result_tx, hires_result_rx) = std::sync::mpsc::channel::<HiResPendingResult>();

    while running.load(Ordering::Relaxed) {
        // 处理命令
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        let has_params = process_commands(
            &command_receiver,
            &mut latest_params,
            &mut should_shutdown,
            &mut deferred,
        );

        // 处理延迟的高精度洋葱皮控制命令
        for cmd in deferred.drain(..) {
            handle_hires_control(
                cmd,
                &device,
                &queue,
                &mut hires_renderer,
                &mut hires_meta,
                &mut hires_config,
                &hires_result_tx,
                &onion_progress,
                texture_format,
            );
        }

        // ★ 每帧检查后台生成是否完成，完成则合并+上传 ★
        if let Ok(pending) = hires_result_rx.try_recv() {
            push_onion_progress(&onion_progress, "正在合并并上传贴图到 GPU…", 0.0);
            if let Some(renderer) = &mut hires_renderer {
                // 按 time_group 分组所有贴图
                let mut by_time: HashMap<u32, Vec<&GroupTile>> = HashMap::new();
                for (coord, tile) in &pending.tiles {
                    by_time.entry(coord.time_group).or_default().push(tile);
                }

                let tw = pending.width;
                let th = pending.height;

                // 合并每个 time_group 的所有 track_group 贴图 → 一张全轨贴图
                for (time_g, group_tiles) in &by_time {
                    let merged_pixels = merge_track_groups(group_tiles, tw, th);
                    let coord = TileCoord::new(0, *time_g);
                    renderer.upload_tile(&device, &queue, coord, &merged_pixels, tw, th);
                }

                // 强制 DMA 完成
                let flush = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("hires_pending_flush"),
                });
                queue.submit(std::iter::once(flush.finish()));
            }
            // pending.tiles 在此 drop，释放 CPU 像素缓冲
            drop(pending);
            push_onion_progress(&onion_progress, "高精度洋葱皮贴图生成+上传完成", 1.0);
        }

        if should_shutdown {
            break;
        }

        // 执行渲染（离屏纹理）
        if has_params && let Some(ref params) = latest_params {
            puffin::profile_scope!("wgpu_render_thread_frame");
            let frame_start = Instant::now();

            let width = params.viewport_size.0.max(1);
            let height = params.viewport_size.1.max(1);

            // 确保离屏纹理已创建
            let mut tex_resources = super::textures::OffscreenTextureResources {
                device: &device,
                texture_format,
                width,
                height,
                current_size: &mut current_size,
                current_texture: &mut current_texture,
                depth_texture: &mut depth_texture,
                depth_texture_view: &mut depth_texture_view,
                latest_texture_clone: &latest_texture_clone,
                params,
            };
            ensure_textures(&mut tex_resources);

            // 仅检测主音符版本号变化后上传
            let note_version = note_instances_buffer.version();
            if note_version != last_note_version {
                last_note_version = note_version;

                puffin::profile_scope!("upload_note_instances_from_buffer");
                let notes = unsafe { note_instances_buffer.read_buffer() };

                note_renderer.upload_instances(notes, &device, &queue);
            }

            if let (Some(_texture), Some(_depth_view)) = (&current_texture, &depth_texture_view) {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("offscreen_render_encoder"),
                });

                // 准备渲染器
                prepare_renderers(
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut ruler_renderer,
                    &mut arrangement_renderer,
                    &mut cc_bar_renderer,
                    params,
                    &note_events_rx,
                    &device,
                    &queue,
                );

                // 高精度贴图视口驱动
                let hires_visible = update_hires_viewport(
                    &mut hires_renderer,
                    &hires_meta,
                    &hires_config,
                    params,
                    &device,
                    &queue,
                );
                let hires_visible_coords: Vec<TileCoord> =
                    hires_visible.iter().map(|(c, _)| *c).collect();

                // 执行渲染通道
                execute_render_pass(
                    &mut encoder,
                    &device,
                    &current_texture,
                    &depth_texture_view,
                    params,
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut ruler_renderer,
                    &mut arrangement_renderer,
                    &queue,
                    &mut cc_bar_renderer,
                    &hires_renderer,
                    &hires_visible_coords,
                );

                // 提交渲染指令
                queue.submit(std::iter::once(encoder.finish()));
            }

            // 更新统计
            let frame_time = frame_start.elapsed();
            update_stats(
                &mut frame_count,
                &mut fps_update_time,
                frame_time,
                params,
                &stats_clone,
            );
        }
    }

    tracing::info!("Render thread stopped");
}

/// 向共享进度缓冲推送一条进度（渲染线程 → UI 线程）
fn push_onion_progress(progress: &Arc<Mutex<Vec<(String, f32)>>>, msg: &str, value: f32) {
    if let Ok(mut buf) = progress.lock() {
        buf.push((msg.to_string(), value.clamp(0.0, 1.0)));
    }
}

/// 处理高精度洋葱皮控制命令
///
/// 核心策略：
/// - 全曲生成在后台线程进行，渲染线程每帧通过 channel 检查结果
/// - 收到结果后按 time_group 合并所有 track_group 贴图，只上传合并后的全轨贴图
/// - 音轨重生成（RegenerateHiResTrack）直接在渲染线程同步执行
#[allow(clippy::too_many_arguments)]
fn handle_hires_control(
    cmd: ControlCommand,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    hires_result_tx: &std::sync::mpsc::Sender<HiResPendingResult>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
    texture_format: wgpu::TextureFormat,
) {
    match cmd {
        ControlCommand::GenerateHiResOnionSkin {
            notes,
            ppq,
            key_count,
            total_ticks,
            config,
            midi_hash,
        } => {
            // 创建/重建高精度渲染器
            *hires_renderer = Some(HiResRenderer::new(device, config.clone(), texture_format));
            *hires_config = Some(config.clone());

            // ★ 元数据必须在后台线程启动前设置（regen 安全）★
            let track_count = notes.len() as u16;
            let time_groups = config.time_group_count(total_ticks, ppq);
            let ticks_per_group = config.ticks_per_group(ppq);
            *hires_meta = Some(HiResMeta {
                track_count,
                key_count,
                time_groups,
                ticks_per_group,
            });

            push_onion_progress(onion_progress, "正在后台生成高精度洋葱皮贴图…", 0.0);

            // ★ 后台线程生成，渲染线程继续跑 ★
            let progress_buf = onion_progress.clone();
            let tx = hires_result_tx.clone();
            std::thread::spawn(move || {
                let cb: HiResProgressCallback = Arc::new(move |msg, pct| {
                    if let Ok(mut buf) = progress_buf.lock() {
                        buf.push((msg.to_string(), pct.clamp(0.0, 1.0)));
                    }
                });

                let tiles = lumino_onion_skin_hires::generate_all_tiles(
                    &notes,
                    &config,
                    ppq,
                    key_count,
                    total_ticks,
                    &midi_hash,
                    Some(cb),
                );

                // 从任意一张贴图推断规格
                let (width, height) = tiles
                    .values()
                    .next()
                    .map(|t| (t.width, t.height))
                    .unwrap_or((config.tile_width_px, key_count as u32));

                // 通过 channel 发回渲染线程（未合并的原始贴图）
                let _ = tx.send(HiResPendingResult { tiles, width, height });
            });
        }
        ControlCommand::DisposeHiResOnionSkin => {
            *hires_renderer = None;
            *hires_meta = None;
            *hires_config = None;
            push_onion_progress(onion_progress, "高精度洋葱皮资源已释放", 1.0);
        }
        // 重生成指定音轨的高精度贴图（编辑后冷静期到期触发）
        ControlCommand::RegenerateHiResTrack {
            track_idx,
            notes,
            ppq,
            key_count,
            total_ticks,
            config,
            midi_hash,
        } => {
            let ticks_per_group = config.ticks_per_group(ppq);
            let time_groups = config.time_group_count(total_ticks, ppq);
            let track_group = (track_idx / TRACKS_PER_GROUP) as u32;
            let width = config.tile_width_px;

            // 从元数据推断音轨组的音轨范围
            let meta = hires_meta.as_ref().expect("元数据应在首次生成后存在");
            let track_start = (track_group * TRACKS_PER_GROUP as u32) as u16;
            let track_end =
                ((track_group + 1) * TRACKS_PER_GROUP as u32).min(meta.track_count as u32) as u16;
            let track_range = (track_start, track_end);

            push_onion_progress(
                onion_progress,
                &format!("重生音轨 {track_idx} 高精度贴图…"),
                0.0,
            );

            let renderer = hires_renderer.as_mut().expect("渲染器应在首次生成后存在");

            for time_g in 0..time_groups {
                let tick_start = time_g * ticks_per_group;
                let tick_end = tick_start + ticks_per_group;
                let coord = TileCoord::new(track_group, time_g);

                // 重新生成脏音轨的单音轨贴图
                let dirty_tile = generate_track_tile(
                    &notes, track_idx, time_g, tick_start, tick_end, width, key_count,
                );

                // 从缓存加载同组其他音轨的单音轨贴图
                let mut track_tiles = Vec::new();
                for t in track_range.0..track_range.1 {
                    if t == track_idx {
                        track_tiles.push(dirty_tile.clone());
                    } else {
                        let meta = CacheMeta::from_tile(
                            &dirty_tile,
                            key_count,
                            ppq,
                            config.measures_per_group,
                        );
                        match read_track_tile_cache(&config.cache_dir, &midi_hash, t, time_g, &meta)
                        {
                            Ok(Some(tile)) => track_tiles.push(tile),
                            Ok(None) => {
                                tracing::debug!(
                                    "重生成：音轨 {t} 时间组 {time_g} 缓存未命中，跳过"
                                );
                            }
                            Err(e) => {
                                tracing::warn!("重生成：音轨 {t} 缓存读取失败: {e}");
                            }
                        }
                    }
                }

                // 合并为新的整合组贴图（仅该 track_group 内）
                let group_tile = merge_group_tiles(
                    &track_tiles,
                    coord,
                    tick_start,
                    tick_end,
                    width,
                    key_count,
                    track_range,
                );

                // ★ 直接上传到 GPU，替换旧纹理 ★
                // 注意：这里只上传了该 track_group 的贴图，不是全轨合并后的
                // 后续需要全量重新生成才能得到正确的全轨贴图
                // 当前简化处理：用 track_group=0 的坐标覆盖，仅单 track_group 场景正确
                renderer.upload_tile(
                    device,
                    queue,
                    coord,
                    &group_tile.pixels,
                    group_tile.width,
                    group_tile.height,
                );

                let pct = (time_g as f32 + 1.0) / time_groups as f32;
                push_onion_progress(
                    onion_progress,
                    &format!("重生音轨 {track_idx}：{}/{}", time_g + 1, time_groups),
                    pct,
                );
            }

            // ★ 强制 DMA 完成 ★
            let flush = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("hires_regen_flush"),
            });
            queue.submit(std::iter::once(flush.finish()));

            push_onion_progress(onion_progress, "高精度贴图重生完成", 1.0);
        }
        _ => {}
    }
}

/// 高精度贴图视口驱动：准备 uniform
///
/// 贴图已在生成时合并为全轨贴图（每 time_group 一张），
/// 此处仅计算可见坐标与 uniform，不再遍历 track_group。
fn update_hires_viewport(
    renderer: &mut Option<HiResRenderer>,
    meta: &Option<HiResMeta>,
    _config: &Option<HiResConfig>,
    params: &RenderParams,
    _device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Vec<(TileCoord, HiResUniform)> {
    let mut visible: Vec<(TileCoord, HiResUniform)> = Vec::new();
    let (Some(renderer), Some(_config), Some(meta)) = (renderer, _config, meta) else {
        return visible;
    };
    if params.is_arrangement_mode {
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

    // ★ 只遍历 time_group，不再遍历 track_group ★
    for time_g in g_start..g_end {
        let coord = TileCoord::new(0, time_g);
        if renderer.has_tile(&coord) {
            let tick_start = time_g * ticks_per_group;
            let area_x = base_x + (tick_start as f32 * zoom_x - scroll_x) * scale;
            let area_w = ticks_per_group as f32 * zoom_x * scale;
            let uniform = HiResUniform::new(area_x, area_y, area_w, area_h, canvas_w, canvas_h);
            visible.push((coord, uniform));
        }
    }

    renderer.prepare(queue, &visible);
    visible
}
