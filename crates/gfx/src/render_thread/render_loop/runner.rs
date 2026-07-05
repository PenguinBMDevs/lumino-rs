use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::SwappableBuffer;
use crate::{
    GroupTile, HiResConfig, HiResProgressCallback, HiResRenderer, HiResUniform, TRACKS_PER_GROUP,
    TileCoord, generate_track_tile, merge_group_tiles,
};

use super::super::commands::{ControlCommand, HiResTrackParams, RenderCommand};
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::Renderers;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::execute_render_pass;
use super::render_pass::update_stats;
use super::textures::ensure_textures;

/// 高精度贴图元数据（无像素数据，用于视口计算）
#[allow(dead_code)]
struct HiResMeta {
    track_count: u16,
    /// 音轨组数（= ceil(track_count / TRACKS_PER_GROUP)），用于判断 time_group 贴图是否收齐
    track_groups: u32,
    key_count: u16,
    time_groups: u32,
    ticks_per_group: u32,
}

/// 后台生成线程流式输出的消息
///
/// 按 time_group 同步推进：所有音轨组线程并行生成同一个 time_group 的贴图，
/// 后台线程合并为单张全轨像素缓冲后发送。渲染线程收到后仅做 GPU 上传（DMA，非阻塞），
/// 避免渲染线程被像素合并阻塞导致掉帧/窗口未响应。
///
/// 使用 `sync_channel(1)` 有界通道：channel 满时后台 send 阻塞，
/// 强制后台线程等待渲染线程消费——背压机制，防止无界积压导致 CPU 内存峰值。
enum HiResStreamMsg {
    /// 某个 (track_group, time_group) 已合并的整合组像素缓冲（width × height × 4 字节）
    TimeGroupMerged {
        track_group: u32,
        time_group: u32,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
    },
    /// 所有贴图生成完毕
    Finished,
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
    let mut renderers = Renderers {
        grid: crate::GridRenderer::new(&device, texture_format),
        note: crate::NoteRenderer::new(&device, &queue, texture_format),
        ruler: crate::RulerRenderer::new(&device, texture_format),
        arrangement: crate::ArrangementRenderer::new(&device, texture_format),
        cc_bar: crate::CcBarRenderer::new(&device, texture_format),
    };

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

    // ★ 后台生成线程通过有界同步通道流式传回贴图（容量1，背压）★
    // sync_channel(1)：channel 满时 send 阻塞，强制后台等渲染线程消费，
    // 防止无界积压导致 CPU 内存峰值（对应"装袋期间工人等着"）
    let (hires_result_tx, hires_result_rx) = std::sync::mpsc::sync_channel::<HiResStreamMsg>(1);

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

        // ★ 流式接收：每帧循环 try_recv，收到已合并像素立即 upload（GPU DMA，非阻塞）★
        loop {
            match hires_result_rx.try_recv() {
                Ok(HiResStreamMsg::TimeGroupMerged {
                    track_group,
                    time_group,
                    pixels,
                    width,
                    height,
                }) => {
                    // ★ merge 已在后台线程完成，渲染线程仅做 GPU 上传（DMA 异步，非阻塞）★
                    tracing::debug!(
                        "[onion-render] 收到并上传整合组贴图: track_group={}, time_group={}, pixels={}",
                        track_group,
                        time_group,
                        pixels.len()
                    );
                    if let Some(renderer) = &mut hires_renderer {
                        let coord = TileCoord::new(track_group, time_group);
                        renderer.upload_tile(&device, &queue, coord, &pixels, width, height);
                    }
                    // pixels 在此 drop，释放 CPU 像素缓冲
                }
                Ok(HiResStreamMsg::Finished) => {
                    // 后台生成全部完毕：flush DMA
                    if hires_renderer.is_some() {
                        let flush =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("hires_stream_flush"),
                            });
                        queue.submit(std::iter::once(flush.finish()));
                    }
                    push_onion_progress(&onion_progress, "高精度洋葱皮贴图流式生成+上传完成", 1.0);
                }
                Err(_) => break, // 无更多消息，退出本帧接收
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

                renderers.note.upload_instances(notes, &device, &queue);
            }

            if let (Some(_texture), Some(_depth_view)) = (&current_texture, &depth_texture_view) {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("offscreen_render_encoder"),
                });

                // 准备渲染器
                prepare_renderers(&mut renderers, params, &note_events_rx, &device, &queue);

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
                    &mut renderers,
                    &queue,
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
/// - 音轨重生成（RegenerateHiResTrack）也移到后台线程，避免阻塞渲染主循环
#[allow(clippy::too_many_arguments)]
fn handle_hires_control(
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

            // ★ 元数据必须在后台线程启动前设置（regen 安全）★
            let track_count = notes.len() as u16;
            let track_groups = config.track_group_count(track_count);
            let time_groups = config.time_group_count(total_ticks, ppq);
            let ticks_per_group = config.ticks_per_group(ppq);
            *hires_meta = Some(HiResMeta {
                track_count,
                track_groups,
                key_count,
                time_groups,
                ticks_per_group,
            });

            push_onion_progress(onion_progress, "正在后台生成高精度洋葱皮贴图…", 0.0);

            // ★ 后台线程流式生成（time_group 同步推进），merge 在后台完成，渲染线程仅 upload ★
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

                // time_group 回调：该 time_group 的所有 track_group 贴图已收齐。
                // 按 track_group 分别发送，渲染线程上传到自己的坐标位置，避免全曲重合并。
                let time_group_cb = {
                    let tx = tx.clone();
                    let tw = tile_width;
                    let th = tile_height;
                    move |time_group: u32, tiles: Vec<GroupTile>| {
                        // sync_channel(1) 有界：逐个 group 发送，背压等渲染线程消费
                        if let Ok(guard) = tx.lock() {
                            for tile in tiles {
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
                midi_hash: _,
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
            if hires_meta.is_none() {
                tracing::debug!("[onion-render] RegenerateHiResTrack: 初始化 hires_meta");
                let track_groups = config.track_group_count(track_count);
                let time_groups = config.time_group_count(total_ticks, ppq);
                let ticks_per_group = config.ticks_per_group(ppq);
                *hires_meta = Some(HiResMeta {
                    track_count,
                    track_groups,
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

            // 音轨重生成改为完全后台静默执行，不再推送进度窗口消息。
            tracing::debug!(
                "[onion-render] RegenerateHiResTrack 启动后台静默重生: track_group={}, time_groups={}",
                track_group,
                time_groups
            );

            // ★ 重生成移到后台线程，避免阻塞渲染线程 ★
            // 生成完成后通过已有 hires_result_tx 传回，渲染线程仅做 GPU upload。
            // 使用 Host 提供的 group_notes 重新合并整个 track_group，
            // 避免读取可能过期的硬盘缓存导致同组其他音轨被覆盖为旧数据。
            let tx = Arc::new(Mutex::new(hires_result_tx.clone()));
            std::thread::spawn(move || {
                // generate_track_tile 要求音符按 start_ms 升序排列
                for notes in &mut group_notes {
                    notes.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
                }
                tracing::debug!(
                    "[onion-render] RegenerateHiResTrack 后台线程启动: track_group={}, time_groups={}",
                    track_group,
                    time_groups
                );

                for time_g in 0..time_groups {
                    let tick_start = time_g * ticks_per_group;
                    let tick_end = tick_start + ticks_per_group;
                    let coord = TileCoord::new(track_group, time_g);

                    // 重新生成该 group 内每轨的单音轨贴图
                    let mut track_tiles = Vec::with_capacity(group_notes.len());
                    for (local_idx, notes) in group_notes.iter().enumerate() {
                        let t = track_start + local_idx as u16;
                        let tile = generate_track_tile(
                            notes, t, time_g, tick_start, tick_end, width, key_count,
                        );
                        if time_g == 0 && local_idx == 0 {
                            tracing::debug!(
                                "[onion-render] RegenerateHiResTrack 生成首个贴图: track={}, coord={:?}, pixels={}",
                                t,
                                coord,
                                tile.pixels.len()
                            );
                        }
                        track_tiles.push(tile);
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

                    // 传回渲染线程执行 GPU 上传
                    if let Ok(guard) = tx.lock() {
                        let _ = guard.send(HiResStreamMsg::TimeGroupMerged {
                            track_group,
                            time_group: time_g,
                            pixels: group_tile.pixels,
                            width: group_tile.width,
                            height: group_tile.height,
                        });
                    }

                    let pct = (time_g as f32 + 1.0) / time_groups as f32;
                    tracing::debug!(
                        "[onion-render] RegenerateHiResTrack 进度: track_group={}, {}/{} ({:.1}%)",
                        track_group,
                        time_g + 1,
                        time_groups,
                        pct * 100.0
                    );
                }

                if let Ok(guard) = tx.lock() {
                    let _ = guard.send(HiResStreamMsg::Finished);
                }
                tracing::debug!(
                    "[onion-render] RegenerateHiResTrack 后台线程完成: track_group={}",
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

            // ★ 干净启动 / 新建工程时元数据可能为空，必须初始化，否则视口遍历找不到覆层 ★
            let needed_track_count = track_count.max(track_idx + 1);
            let needed_track_groups = config.track_group_count(needed_track_count);
            if hires_meta.is_none() {
                tracing::debug!("[onion-render] ShowHiResDirtyOverlay: 初始化 hires_meta");
                let time_groups = config.time_group_count(total_ticks, ppq);
                let ticks_per_group = config.ticks_per_group(ppq);
                *hires_meta = Some(HiResMeta {
                    track_count: needed_track_count,
                    track_groups: needed_track_groups,
                    key_count,
                    time_groups,
                    ticks_per_group,
                });
            } else if let Some(meta) = hires_meta {
                // 若已有元数据但音轨组范围不足，扩展范围以确保覆层可被遍历到
                if meta.track_groups < needed_track_groups {
                    tracing::debug!(
                        "[onion-render] ShowHiResDirtyOverlay: 扩展 track_groups {} -> {}",
                        meta.track_groups,
                        needed_track_groups
                    );
                    meta.track_groups = needed_track_groups;
                    meta.track_count = meta.track_count.max(needed_track_count);
                }
            }

            let ticks_per_group = config.ticks_per_group(ppq);
            let time_groups = config.time_group_count(total_ticks, ppq);
            let width = config.tile_width_px;
            let track_start = (track_group * TRACKS_PER_GROUP as u32) as u16;
            let track_end =
                (track_start as u32 + group_notes.len() as u32).min(track_count as u32) as u16;
            let track_range = (track_start, track_end);

            // 直接在渲染线程生成临时覆层贴图（音符少，耗时可控）
            for notes in &mut group_notes {
                notes.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
            }

            if let Some(renderer) = hires_renderer {
                for time_g in 0..time_groups {
                    let tick_start = time_g * ticks_per_group;
                    let tick_end = tick_start + ticks_per_group;
                    let coord = TileCoord::new(track_group, time_g);

                    // 重新生成该 group 内每轨的单音轨贴图
                    let mut track_tiles = Vec::with_capacity(group_notes.len());
                    for (local_idx, notes) in group_notes.iter().enumerate() {
                        let t = track_start + local_idx as u16;
                        let tile = generate_track_tile(
                            notes, t, time_g, tick_start, tick_end, width, key_count,
                        );
                        track_tiles.push(tile);
                    }

                    // 合并为新的整合组贴图覆层（仅该 track_group 内）
                    let group_tile = merge_group_tiles(
                        &track_tiles,
                        coord,
                        tick_start,
                        tick_end,
                        width,
                        key_count,
                        track_range,
                    );

                    renderer.upload_dirty_overlay(
                        device,
                        _queue,
                        coord,
                        &group_tile.pixels,
                        group_tile.width,
                        group_tile.height,
                    );
                }
                tracing::debug!(
                    "[onion-render] ShowHiResDirtyOverlay: 已上传 {} 个覆层贴图 (track_group={})",
                    time_groups,
                    track_group
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

/// 高精度贴图视口驱动：准备 uniform
///
/// 遍历可见范围内的所有 (track_group, time_group) 组合。
/// 正常贴图与临时脏区域覆层使用同一套 uniform，覆层在正常贴图之后绘制。
fn update_hires_viewport(
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
                let area_x = base_x + (tick_start as f32 * zoom_x - scroll_x) * scale;
                let area_w = ticks_per_group as f32 * zoom_x * scale;
                let uniform = HiResUniform::new(area_x, area_y, area_w, area_h, canvas_w, canvas_h);
                visible.push((coord, uniform));
            }
        }
    }

    renderer.prepare(queue, &visible);
    renderer.prepare_dirty_overlays(queue, &visible);
    visible
}
