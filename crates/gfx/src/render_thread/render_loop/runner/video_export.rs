use std::sync::Arc;

use super::super::super::commands::FrameSender;
use super::super::super::export_pipeline::ExportPipeline;
use super::super::super::params::RenderParams;
use super::super::prepare::prepare_renderers;
use super::super::render_pass::execute_render_pass;
use super::super::textures::{OffscreenTextureResources, ensure_textures};
use super::context::{RenderContext, RenderFrameState, RenderThreadChannels};

pub(crate) mod common;
pub(crate) use common::{render_video_frame_command, start_video_export};

/// 推进视频导出 inflight 帧读回。
///
/// 即使没有新的 `RenderVideoFrame` 命令，也需要 `try_read` 已就绪的帧数据并发回 Runner，
/// 否则 inflight 满后 Runner 阻塞在 `frame_rx.recv()`，渲染线程也不再调用 `try_read`，形成死锁。
pub(super) fn advance_export_inflight(
    export_pipeline: &mut Option<ExportPipeline>,
    export_frame_tx: &Option<FrameSender>,
) {
    if let (Some(pipeline), Some(tx)) = (export_pipeline, export_frame_tx) {
        while let Some(data) = pipeline.try_read() {
            if tx.0.send(data).is_err() {
                tracing::warn!("视频帧发送失败：Runner 通道已关闭");
                break;
            }
        }
    }
}

/// 处理视频导出帧：离屏渲染 → copy 到 staging → submit → map_async
///
/// 流水线模式：不阻塞等待当前帧读回，而是利用四重缓冲让 GPU 渲染与 CPU 读回重叠。
/// - inflight 达到上限（4）时，阻塞读最早的一帧（此时 GPU 通常已完成 map_async）
/// - copy_and_submit 后立即返回，GPU 继续处理下一帧
/// - try_read 非阻塞读回已就绪的帧
///
/// 这会打破"每命令一帧"的语义：Runner 发 N 帧命令可能先收到 0~4 帧数据，
/// 剩余帧在 FinishVideoExport 或后续命令中读回。Runner 侧需用 param_queue 跟踪。
pub(super) fn handle_video_frame(
    ctx: &RenderContext,
    params: RenderParams,
    frame: &mut RenderFrameState,
    channels: &RenderThreadChannels,
) {
    // 首帧诊断 tracing：定位音符缺失问题在哪一层
    // 使用静态原子计数器，只输出前 3 帧，避免日志嘈杂
    static DIAG_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let diag_idx = DIAG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if diag_idx < 3 {
        tracing::info!(
            "视频帧诊断[{}]: note_instances={}, viewport={}x{}, scroll=({},{})",
            diag_idx,
            params.note_instances.len(),
            params.viewport_size.0,
            params.viewport_size.1,
            params.scroll.0,
            params.scroll.1,
        );
    }

    // 提前检查导出管线是否已初始化，避免后续重复判断
    let pipeline_ready = frame.export_pipeline.is_some() && frame.export_frame_tx.is_some();
    if !pipeline_ready {
        tracing::warn!("RenderVideoFrame 收到但导出管线未初始化，忽略");
        return;
    }

    let width = params.viewport_size.0.max(1);
    let height = params.viewport_size.1.max(1);

    // 瀑布流模式：使用 compute shader 全 GPU 渲染
    if params.is_waterfall_mode {
        handle_waterfall_frame(ctx, params, frame, width, height);
        return;
    }

    // Miditrail 模式：使用 3D wgpu 渲染管线
    if params.miditrail_enabled {
        handle_miditrail_frame(ctx, params, frame, width, height);
        return;
    }

    // 钢琴卷帘模式：正常 GPU 渲染管线

    // 1. 确保离屏纹理已创建（视频导出为纯 2D，禁用 depth）
    let mut tex_resources = OffscreenTextureResources {
        device: &ctx.device,
        texture_format: ctx.texture_format,
        width,
        height,
        current_size: frame.current_size,
        current_texture: frame.current_texture,
        depth_texture: frame.depth_texture,
        depth_texture_view: frame.depth_texture_view,
        texture_view: frame.texture_view,
        latest_texture_clone: frame.latest_texture_clone,
        params: &params,
    };
    ensure_textures(&mut tex_resources, false);

    // 视频导出始终使用音符矩形渲染模式：不上传 HiRes 贴图
    let hires_visible_coords: Vec<crate::TileCoord> = Vec::new();

    // 2. 上传视频导出帧的音符实例
    if !params.note_instances.is_empty() {
        frame
            .renderers
            .note
            .upload_instances(&params.note_instances, &ctx.device, &ctx.queue);
    }
    // 首帧诊断：上传后的 last_upload_count
    if diag_idx < 3 {
        tracing::info!(
            "视频帧诊断[{}]: 上传后 last_upload_count={}",
            diag_idx,
            frame.renderers.note.last_upload_count(),
        );
    }

    // 3. 检查离屏纹理是否就绪（clone Arc 断开与 frame 的借用链，
    //    使后续 execute_render_pass 可以再借 &mut frame）。
    let texture_opt = frame.current_texture.as_ref().map(Arc::clone);
    let Some(texture_arc) = texture_opt else {
        tracing::warn!("视频帧渲染：离屏纹理未就绪，跳过");
        return;
    };

    // 4. 创建编码器并执行渲染通道
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("video_export_render_encoder"),
        });

    prepare_renderers(
        frame.renderers,
        &params,
        &channels.note_events_rx,
        &ctx.device,
        &ctx.queue,
        true,
    );

    execute_render_pass(
        &mut encoder,
        ctx,
        &params,
        &hires_visible_coords,
        true,
        frame,
    );

    // 5. 流水线模式：inflight 达到上限时阻塞读最早的一帧（背压）
    // 此时从 frame 获取 pipeline / tx 引用，execute_render_pass 已经完成，
    // 不会再与 frame 的借用冲突。
    // pipeline_ready 已提前保证 export_pipeline / export_frame_tx 不为 None。
    let pipeline = frame
        .export_pipeline
        .as_mut()
        .expect("export_pipeline 应已通过 pipeline_ready 检查完成初始化，不应为 None");
    let tx = frame
        .export_frame_tx
        .as_ref()
        .expect("export_frame_tx 应已通过 pipeline_ready 检查完成初始化，不应为 None");
    pipeline.ensure_size(width, height);
    while !pipeline.can_write() {
        let data = pipeline.wait_read();
        if tx.0.send(data).is_err() {
            tracing::warn!("视频帧发送失败：Runner 通道已关闭");
            return;
        }
    }

    // 6. copy 离屏纹理到 staging buffer + submit + map_async（非阻塞，立即返回）
    pipeline.copy_and_submit(encoder, &texture_arc, &ctx.queue);

    // 6. 尝试非阻塞读回已就绪的帧（流水线推进，不阻塞下一帧渲染）
    while let Some(data) = pipeline.try_read() {
        if tx.0.send(data).is_err() {
            tracing::warn!("视频帧发送失败：Runner 通道已关闭");
            return;
        }
    }
}

/// 瀑布流帧渲染：使用 compute shader 全 GPU 渲染，写入 storage texture 后读回。
fn handle_waterfall_frame(
    ctx: &RenderContext,
    params: RenderParams,
    frame: &mut RenderFrameState,
    width: u32,
    height: u32,
) {
    use crate::{WaterfallRenderer, WaterfallUniformGpu};

    // 初始化瀑布流渲染器
    if frame.waterfall_renderer.is_none() {
        *frame.waterfall_renderer = Some(WaterfallRenderer::new(&ctx.device));
    }
    let renderer = frame
        .waterfall_renderer
        .as_mut()
        .expect("waterfall_renderer 应已初始化");

    // 键盘高度：帧高的 12%
    let kb_height = ((height as f64) * 0.12).round() as u32;
    let kb_height = kb_height.max(20).min(height / 3);

    // 构建 uniform 参数
    // 注意：使用 waterfall_current_tick（MIDI tick 值），而非 scroll.0（像素位置 = tick * zoom_x）
    let uniform = WaterfallUniformGpu {
        tick: params.waterfall_current_tick,
        ppq: params.ppq as u32,
        key_count: (params.max_key_index + 1.0) as u32,
        frame_width: width,
        frame_height: height,
        kb_height,
        speed: params.waterfall_speed.max(0.1),
        _padding: 0,
    };

    // 使用传入的瀑布流音符数据
    let notes = &params.waterfall_notes;

    // 构建活跃键颜色数组（128 个 u32，0 表示无高亮）
    let mut active_key_colors = [0u32; 128];
    for note in notes {
        let tick_u = params.waterfall_current_tick;
        if note.start_tick <= tick_u && note.end_tick > tick_u {
            let key = note.key as usize;
            if key < 128 {
                // 使用音符颜色作为活跃键高亮（混合 60% 透明度）
                // color_packed 为 0xRRGGBBAA（调色板颜色，由 pack_color 打包），
                // 与 shader 中 unpack_color 的 RRGGBBAA 解包保持一致
                let c = note.color_packed;
                let r = (c >> 24) & 0xFF;
                let g = (c >> 16) & 0xFF;
                let b = (c >> 8) & 0xFF;
                // 存储 0xRRGGBBAA，alpha=153 表示 60% 混合（与 shader 中 blend_key_color 的 alpha 参数匹配）
                active_key_colors[key] = (r << 24) | (g << 16) | (b << 8) | 153u32;
            }
        }
    }

    // 创建编码器
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("waterfall_encoder"),
        });

    // dispatch compute shader
    renderer.render(
        &ctx.device,
        &ctx.queue,
        &mut encoder,
        &uniform,
        notes,
        &params.waterfall_key_offsets,
        &active_key_colors,
    );

    // 获取输出纹理并拷贝到 staging buffer
    let pipeline = frame
        .export_pipeline
        .as_mut()
        .expect("export_pipeline 应已初始化");
    let tx = frame
        .export_frame_tx
        .as_ref()
        .expect("export_frame_tx 应已初始化");

    pipeline.ensure_size(width, height);
    while !pipeline.can_write() {
        let data = pipeline.wait_read();
        if tx.0.send(data).is_err() {
            tracing::warn!("瀑布流帧发送失败：Runner 通道已关闭");
            return;
        }
    }

    if let Some(tex) = renderer.output_texture() {
        pipeline.copy_and_submit(encoder, tex, &ctx.queue);
    } else {
        tracing::warn!("瀑布流输出纹理未就绪");
        // 即使没有输出纹理，也必须提交空编码器，否则 GPU 队列死锁
        ctx.queue.submit(std::iter::once(encoder.finish()));
        return;
    }

    // 非阻塞读回已就绪帧
    while let Some(data) = pipeline.try_read() {
        if tx.0.send(data).is_err() {
            tracing::warn!("瀑布流帧发送失败：Runner 通道已关闭");
            return;
        }
    }
}

/// Miditrail 3D 帧渲染：使用 3D 渲染管线写入颜色纹理后读回。
fn handle_miditrail_frame(
    ctx: &RenderContext,
    params: RenderParams,
    frame: &mut RenderFrameState,
    width: u32,
    height: u32,
) {
    use crate::{MiditrailRenderer, MiditrailUniformGpu};

    if frame.miditrail_renderer.is_none() {
        *frame.miditrail_renderer = Some(MiditrailRenderer::new(&ctx.device));
    }
    let renderer = frame
        .miditrail_renderer
        .as_mut()
        .expect("miditrail_renderer 应已初始化");

    let kb_height = ((height as f64) * 0.12).round() as u32;
    let kb_height = kb_height.max(20).min(height / 3);

    // 光晕环动画时间基准：导出参数已按当前 BPM 计算；
    // 0（未知，如默认参数）回退到 120 BPM（ppq × 2）。
    let ticks_per_second = if params.miditrail_ticks_per_second > 0.0 {
        params.miditrail_ticks_per_second
    } else {
        params.ppq.max(1.0) * 2.0
    };

    let uniform = MiditrailUniformGpu {
        tick: params.miditrail_current_tick,
        ppq: params.ppq as u32,
        key_count: (params.max_key_index + 1.0) as u32,
        frame_width: width,
        frame_height: height,
        kb_height,
        _reserved: 0,
        speed: params.miditrail_speed.max(0.1),
        param1: params.waterfall_speed.max(0.1),
        param2: 0.0,
        fps: params.fps.max(1.0),
        z_far_distance: params.miditrail_z_far.max(0.1),
        ticks_per_second,
        _padding1: 0,
    };

    let notes = &params.miditrail_notes;

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("miditrail_encoder"),
        });

    renderer.render(&ctx.device, &ctx.queue, &mut encoder, &uniform, notes);

    let pipeline = frame
        .export_pipeline
        .as_mut()
        .expect("export_pipeline 应已初始化");
    let tx = frame
        .export_frame_tx
        .as_ref()
        .expect("export_frame_tx 应已初始化");

    pipeline.ensure_size(width, height);
    while !pipeline.can_write() {
        let data = pipeline.wait_read();
        if tx.0.send(data).is_err() {
            tracing::warn!("Miditrail 帧发送失败：Runner 通道已关闭");
            return;
        }
    }

    if let Some(tex) = renderer.output_texture() {
        pipeline.copy_and_submit(encoder, tex, &ctx.queue);
    } else {
        tracing::warn!("Miditrail 输出纹理未就绪");
        ctx.queue.submit(std::iter::once(encoder.finish()));
        return;
    }

    while let Some(data) = pipeline.try_read() {
        if tx.0.send(data).is_err() {
            tracing::warn!("Miditrail 帧发送失败：Runner 通道已关闭");
            return;
        }
    }
}
