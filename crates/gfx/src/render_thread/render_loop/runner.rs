use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::SwappableBuffer;
use crate::{
    CacheMeta, GroupTile, HiResConfig, HiResProgressCallback, HiResRenderer, HiResUniform, KeyMode,
    OnionSkinRenderer, TRACKS_PER_GROUP, TileCoord, generate_track_tile, merge_group_tiles,
    read_track_tile_cache,
};

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::execute_render_pass;
use super::render_pass::update_stats;
use super::textures::ensure_textures;

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

    // 洋葱皮渲染器状态：lazy 创建于首次 GenerateOnionSkin 命令
    let mut onion_skin_renderer: Option<OnionSkinRenderer> = None;
    let mut onion_key_mode: Option<KeyMode> = None;
    // 高精度洋葱皮渲染器状态：lazy 创建于首次 GenerateHiResOnionSkin 命令
    let mut hires_renderer: Option<HiResRenderer> = None;
    let mut hires_tiles: HashMap<TileCoord, GroupTile> = HashMap::new();
    let mut hires_config: Option<HiResConfig> = None;
    let mut deferred: Vec<ControlCommand> = Vec::new();

    while running.load(Ordering::Relaxed) {
        // 处理命令：先 drain 积压，然后如果没有 RenderCommand 则阻塞等待
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        let has_params = process_commands(
            &command_receiver,
            &mut latest_params,
            &mut should_shutdown,
            &mut deferred,
        );

        // 处理延迟的洋葱皮控制命令（需要 device/queue 上下文）
        for cmd in deferred.drain(..) {
            match &cmd {
                ControlCommand::GenerateHiResOnionSkin { .. }
                | ControlCommand::DisposeHiResOnionSkin
                | ControlCommand::RegenerateHiResTrack { .. } => handle_hires_control(
                    cmd,
                    &device,
                    &mut hires_renderer,
                    &mut hires_tiles,
                    &mut hires_config,
                    &onion_progress,
                    texture_format,
                ),
                _ => handle_onion_control(
                    &mut onion_skin_renderer,
                    &mut onion_key_mode,
                    &device,
                    &queue,
                    texture_format,
                    cmd,
                    &onion_progress,
                ),
            }
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

                // 洋葱皮：进度轮询 + 贴图上传 + uniform 更新（在 render pass 之前）
                if let Some(ref mut onion) = onion_skin_renderer {
                    if let Some(progress) = onion.poll_progress() {
                        push_onion_progress(
                            &onion_progress,
                            "正在生成洋葱皮概览贴图…",
                            progress.percent() / 100.0,
                        );
                    }
                    if let Some(vp) = params.onion_skin_viewport {
                        if onion.check_and_upload(&device, &queue) {
                            push_onion_progress(&onion_progress, "洋葱皮概览贴图生成完成", 1.0);
                        }
                        onion.update_uniform(&queue, vp);
                    }
                }

                // 高精度贴图视口驱动：上传可见贴图、准备 uniform，返回可见坐标列表
                let hires_visible = update_hires_viewport(
                    &mut hires_renderer,
                    &hires_tiles,
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
                    &onion_skin_renderer,
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

/// 处理洋葱皮控制命令（在渲染线程主循环中调用，拥有 device/queue 上下文）
#[allow(clippy::too_many_arguments)]
fn handle_onion_control(
    onion: &mut Option<OnionSkinRenderer>,
    onion_key_mode: &mut Option<KeyMode>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    cmd: ControlCommand,
    progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    match cmd {
        ControlCommand::GenerateOnionSkin {
            notes,
            duration_ms,
            key_mode,
        } => {
            // key_mode 变化时重建渲染器（贴图高度不同）
            if *onion_key_mode != Some(key_mode) {
                *onion = None; // Drop → dispose 旧渲染器
                let mut renderer = OnionSkinRenderer::new(device, queue, key_mode);
                renderer.update_render_format(device, texture_format);
                *onion = Some(renderer);
                *onion_key_mode = Some(key_mode);
            }
            if let Some(renderer) = onion.as_mut() {
                // tempo_table=None：以 tick 作为时间单位，与钢琴卷帘的 tick-线性映射对齐
                renderer.generate(device, queue, notes, duration_ms, None);
                push_onion_progress(progress, "正在生成洋葱皮概览贴图…", 0.0);
            }
        }
        ControlCommand::DisposeOnionSkin if onion.is_some() => {
            *onion = None; // Drop → dispose，取消后台生成线程
            *onion_key_mode = None;
            // 关闭可能仍开启的进度窗口
            push_onion_progress(progress, "洋葱皮资源已释放", 1.0);
        }
        // Resize / Shutdown 已在 process_commands 中处理，不会到达此处
        _ => {}
    }
}

/// 向共享进度缓冲推送一条进度（渲染线程 → UI 线程）
fn push_onion_progress(progress: &Arc<Mutex<Vec<(String, f32)>>>, msg: &str, value: f32) {
    if let Ok(mut buf) = progress.lock() {
        buf.push((msg.to_string(), value.clamp(0.0, 1.0)));
    }
}

/// 处理高精度洋葱皮控制命令（在渲染线程主循环中调用，拥有 device 上下文）
fn handle_hires_control(
    cmd: ControlCommand,
    device: &wgpu::Device,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_tiles: &mut HashMap<TileCoord, GroupTile>,
    hires_config: &mut Option<HiResConfig>,
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

            // 构造进度回调：把生成进度推入共享缓冲（UI 线程读取）
            let progress_buf = onion_progress.clone();
            let cb: HiResProgressCallback = Arc::new(move |msg, pct| {
                if let Ok(mut buf) = progress_buf.lock() {
                    buf.push((msg.to_string(), pct.clamp(0.0, 1.0)));
                }
            });

            push_onion_progress(onion_progress, "正在生成高精度洋葱皮贴图…", 0.0);

            // 同步生成全曲贴图（阻塞渲染线程，缓存命中时较快）
            let tiles = lumino_onion_skin_hires::generate_all_tiles(
                &notes,
                &config,
                ppq,
                key_count,
                total_ticks,
                &midi_hash,
                Some(cb),
            );
            *hires_tiles = tiles;

            push_onion_progress(onion_progress, "高精度洋葱皮贴图生成完成", 1.0);
        }
        ControlCommand::DisposeHiResOnionSkin => {
            *hires_renderer = None;
            hires_tiles.clear();
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

            // 从现有 tiles 推断音轨组的音轨范围
            let track_range = hires_tiles
                .values()
                .find(|t| t.coord.track_group == track_group)
                .map(|t| t.track_range)
                .unwrap_or((track_idx, track_idx + 1));

            push_onion_progress(
                onion_progress,
                &format!("重生音轨 {track_idx} 高精度贴图…"),
                0.0,
            );

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
                                // 缓存未命中，用空贴图（该音轨在该时间组无内容）
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

                // 合并为新的整合组贴图
                let group_tile = merge_group_tiles(
                    &track_tiles,
                    coord,
                    tick_start,
                    tick_end,
                    width,
                    key_count,
                    track_range,
                );

                // 更新内存缓冲
                hires_tiles.insert(coord, group_tile);

                // 如果该整合组已在 GPU，移除旧纹理（下一帧自动重新上传）
                if let Some(renderer) = hires_renderer {
                    renderer.remove_tile(&coord);
                }

                let pct = (time_g as f32 + 1.0) / time_groups as f32;
                push_onion_progress(
                    onion_progress,
                    &format!("重生音轨 {track_idx}：{}/{}", time_g + 1, time_groups),
                    pct,
                );
            }

            push_onion_progress(onion_progress, "高精度贴图重生完成", 1.0);
        }
        // 其它命令由 handle_onion_control 处理，不会到达此处
        _ => {}
    }
}

/// 高精度贴图视口驱动：上传可见贴图、淘汰超限贴图、准备 uniform
///
/// 返回当前帧可见贴图的坐标与 uniform 列表，供 `HiResRenderer::render` 使用。
fn update_hires_viewport(
    renderer: &mut Option<HiResRenderer>,
    tiles: &HashMap<TileCoord, GroupTile>,
    config: &Option<HiResConfig>,
    params: &RenderParams,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Vec<(TileCoord, HiResUniform)> {
    let mut visible: Vec<(TileCoord, HiResUniform)> = Vec::new();
    let (Some(renderer), Some(config)) = (renderer, config) else {
        return visible;
    };
    if params.is_arrangement_mode || tiles.is_empty() {
        return visible;
    }

    let scale = params.scale_factor;
    let zoom_x = params.zoom.0;
    let zoom_y = params.zoom.1;
    if zoom_x <= 0.0 || zoom_y <= 0.0 {
        return visible;
    }

    let ppq = params.ppq as u16;
    let ticks_per_group = config.ticks_per_group(ppq);
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

    // 音轨组数：从已生成贴图中推断最大音轨索引
    let track_count = tiles.values().map(|t| t.track_range.1).max().unwrap_or(0);
    let track_groups = config.track_group_count(track_count);

    // 全 key 高度（像素），从贴图高度推断（128 或 256）
    let key_count = tiles.values().next().map(|t| t.height).unwrap_or(128);

    // area 矩形（framebuffer 物理像素）：
    // - X：卷帘区域起点 + (tick_start*zoom_x - scroll_x) * scale
    // - Y：最高音 key 在 framebuffer 的 y = (canvas_offset.y + ruler_height - scroll_y) * scale
    // - W：ticks_per_group * zoom_x * scale
    // - H：全 key 范围高度 = key_count * zoom_y * scale
    let base_x = (params.canvas_offset.0 + params.keyboard_width) * scale;
    let scroll_y = params.scroll.1;
    let area_y = (params.canvas_offset.1 + params.ruler_height - scroll_y) * scale;
    let area_h = key_count as f32 * zoom_y * scale;
    let canvas_w = params.viewport_size.0 as f32;
    let canvas_h = params.viewport_size.1 as f32;

    for tg in 0..track_groups {
        for time_g in g_start..g_end {
            let coord = TileCoord::new(tg, time_g);
            // 上传尚未在 GPU 的可见贴图
            if !renderer.has_tile(&coord)
                && let Some(tile) = tiles.get(&coord)
            {
                renderer.upload_tile(device, queue, coord, &tile.pixels, tile.width, tile.height);
            }
            if renderer.has_tile(&coord) {
                let tick_start = time_g * ticks_per_group;
                let area_x = base_x + (tick_start as f32 * zoom_x - scroll_x) * scale;
                let area_w = ticks_per_group as f32 * zoom_x * scale;
                let uniform = HiResUniform::new(area_x, area_y, area_w, area_h, canvas_w, canvas_h);
                visible.push((coord, uniform));
            }
        }
    }

    // 显存超限时清空所有贴图（简化 LRU，下一帧重新上传可见的）
    if renderer.is_over_limit() {
        renderer.clear();
        visible.clear();
    }

    // 准备可见贴图的 uniform buffer
    renderer.prepare(queue, &visible);
    visible
}
