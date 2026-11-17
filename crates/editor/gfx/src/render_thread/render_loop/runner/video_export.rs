use std::sync::Arc;

use super::super::super::commands::{FrameSender, VideoFrameMessage};
use super::super::super::export_pipeline::ExportPipeline;
use super::super::super::params::RenderParams;
use super::super::prepare::prepare_renderers;
use super::super::render_pass::execute_render_pass;
use super::super::textures::{OffscreenTextureResources, ensure_textures};
use super::context::{RenderContext, RenderFrameState, RenderThreadChannels};
use crate::{CullActiveParams, CullWindow};

pub(crate) mod common;
mod miditrail;
mod waterfall;
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
        while let Some((data, key_colors)) = pipeline.try_read() {
            if tx.0.send(VideoFrameMessage { data, key_colors }).is_err() {
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
    _channels: &RenderThreadChannels,
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

    // REND-019 PR-D（R5-b：TDR 快速放弃）——device 已丢失时不再对死设备提交
    // 导出帧（离屏渲染 / copy / submit / map 均无意义且会毒化映射状态）；
    // 导出由 Runner 侧 cancel 标志以明确错误收场。
    if crate::device_status::device_lost() {
        tracing::warn!("RenderVideoFrame：device 已丢失（TDR），跳过本帧（导出将由 cancel 收场）");
        return;
    }

    let width = params.viewport_size.0.max(1);
    let height = params.viewport_size.1.max(1);

    // 瀑布流模式：使用 compute shader 全 GPU 渲染
    if params.is_waterfall_mode {
        waterfall::handle_waterfall_frame(ctx, params, frame, width, height);
        return;
    }

    // Miditrail 模式：使用 3D wgpu 渲染管线
    if params.miditrail_enabled {
        miditrail::handle_miditrail_frame(ctx, params, frame, width, height);
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

    // 视频导出始终使用 Lumino卷帘渲染模式：不上传 贴图瀑布流
    let waterfall_visible_coords: Vec<crate::WaterfallTileCoord> = Vec::new();

    // 2. 音符数据源（二选一，互斥，优先级：直绑 > 上传回退）：
    //    - 主缓冲就绪且尚未直绑 → 直绑 onion 常驻缓冲，零上传。含首帧竞态后的
    //      late-bind（首帧流式未完成则先回退，完成后切直绑；边框会从计算值变为
    //      1px，一次性）。
    //    - 未就绪 → 仅当导出缓冲全新且本帧带数据时回退上传（首帧全量）。
    //    直绑一旦落定不再切换；回退上传后仍可 late-bind（见上）。
    if !frame.renderers.note.is_external_bound() {
        if let Some((ref onion_buf, onion_count)) = frame.onion_source {
            frame.renderers.note.bind_external_source(
                &ctx.device,
                &ctx.queue,
                onion_buf,
                onion_count as usize,
            );
            // 常驻内容已变：活跃键聚合器下次重建桶（一次性 GPU 基数排序）
            if onion_count > 0 {
                frame.renderers.piano_key_cull.mark_resident_updated();
            }
        } else {
            let export_note_pristine = frame.renderers.note.gpu_instance_count() == 0
                && frame.renderers.note.last_upload_count() == 0;
            if export_note_pristine && !params.note_instances.is_empty() {
                frame.renderers.note.upload_instances(
                    &params.note_instances,
                    &ctx.device,
                    &ctx.queue,
                );
                frame.renderers.piano_key_cull.mark_resident_updated();
            }
        }
    }
    // 首帧诊断：数据源（直绑/回退/空）＋上传/直绑后的 last_upload_count。
    // 下次导出若怀疑"仍然上传"，看这三行：bound=直绑零上传，fallback=回退上传。
    if diag_idx < 3 {
        let source = if frame.renderers.note.is_external_bound() {
            "bound"
        } else if frame.onion_source.is_some() {
            "bound-empty"
        } else {
            "fallback"
        };
        tracing::info!(
            "视频帧诊断[{}]: 源={} last_upload_count={}",
            diag_idx,
            source,
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

    prepare_renderers(frame.renderers, &params, &ctx.device, &ctx.queue);

    execute_render_pass(
        &mut encoder,
        ctx,
        &params,
        &waterfall_visible_coords,
        true,
        frame,
    );

    // 4b. 钢琴卷帘键色 GPU 聚合：COUNT 顺带推导 128 键色（1KB 回读），
    // 随帧同槽排队发送，导出线程零 CPU 扫描。
    let current_key_colors = collect_piano_key_colors(ctx, &params, frame);

    // 5. 流水线模式：inflight 达到上限时阻塞读最早的一帧（背压）
    // 此时从 frame 获取 pipeline / tx 引用，execute_render_pass 已经完成，
    // 不会再与 frame 的借用冲突。
    // pipeline_ready 已提前保证 export_pipeline / export_frame_tx 不为 None
    let pipeline = match frame.export_pipeline.as_mut() {
        Some(p) => p,
        None => {
            debug_assert!(
                false,
                "export_pipeline 应已通过 pipeline_ready 检查完成初始化，不应为 None"
            );
            return;
        }
    };
    let tx = match frame.export_frame_tx.as_ref() {
        Some(t) => t,
        None => {
            debug_assert!(
                false,
                "export_frame_tx 应已通过 pipeline_ready 检查完成初始化，不应为 None"
            );
            return;
        }
    };
    pipeline.ensure_size(width, height);
    while !pipeline.can_write() {
        let (data, key_colors) = pipeline.wait_read();
        if tx.0.send(VideoFrameMessage { data, key_colors }).is_err() {
            tracing::warn!("视频帧发送失败：Runner 通道已关闭");
            return;
        }
    }

    // 6. copy 离屏纹理到 staging buffer + submit + map_async（非阻塞，立即返回）
    // 键色随当前帧写入 staging 槽，读回时同槽取出，保证乱序流水线不串帧。
    pipeline.copy_and_submit(encoder, texture_arc.inner(), &ctx.queue, current_key_colors);

    // 6. 尝试非阻塞读回已就绪的帧（流水线推进，不阻塞下一帧渲染）
    while let Some((data, key_colors)) = pipeline.try_read() {
        if tx.0.send(VideoFrameMessage { data, key_colors }).is_err() {
            tracing::warn!("视频帧发送失败：Runner 通道已关闭");
            return;
        }
    }
}

/// 钢琴卷帘当前帧活跃键色：GPU COUNT 顺带聚合 128 键色（1KB 回读）。
///
/// 返回 `None` 表示无键色可传（空常驻/聚合失败），导出侧按全 0 处理。
fn collect_piano_key_colors(
    ctx: &RenderContext,
    params: &RenderParams,
    frame: &mut RenderFrameState<'_>,
) -> Option<[u8; 1024]> {
    let zoom_x = params.zoom.0.max(f32::EPSILON);
    let tick = (params.scroll.0 / zoom_x).max(0.0) as u32;
    let tick_span = (params.viewport_size.0 as f32 / zoom_x).max(1.0) as u32;
    let key_count = (params.max_key_index + 1.0).max(1.0) as usize;
    // 直绑 onion 缓冲：注入其内容世代（识别等长原地改写，见 BucketSource::epoch 契约）；
    // 上传回退路径：清除外部世代，回落到内部 seq（上传时 mark_resident_updated）。
    let onion = frame.onion_source.clone();
    let (resident_buf, resident_count) = match onion {
        Some((buf, count)) => {
            frame
                .renderers
                .piano_key_cull
                .set_external_epoch(frame.onion_epoch);
            (buf, count)
        }
        None => {
            frame.renderers.piano_key_cull.clear_external_epoch();
            frame.renderers.note.gpu_note_buffer_for_sharing()
        }
    };
    if resident_count == 0 || key_count == 0 {
        return None;
    }
    let window = CullWindow {
        tick_start: tick,
        tick_end: tick.saturating_add(tick_span),
        key_count,
    };
    // 钢琴卷帘不使用光晕（`[128,256)` 段），仅传合理值避免除零/NaN
    let active_params = CullActiveParams {
        ticks_per_second: params.ppq.max(1.0) * 2.0,
        fps: params.fps.max(1.0),
    };
    match frame.renderers.piano_key_cull.extract_count(
        &ctx.device,
        &ctx.queue,
        &resident_buf,
        resident_count as usize,
        window,
        Some(active_params),
    ) {
        Ok(extract) => extract.active.map(|active| active_to_key_colors(&active)),
        Err(e) => {
            tracing::warn!("钢琴卷帘活跃键 GPU 聚合失败，本帧键色回退为空: {e}");
            None
        }
    }
}

/// 活跃键内核输出（`[0,128)` = `0xRRGGBBAA`）→ 128 键 RGBA 字节。
///
/// 与主 crate `composite_keyboard` 期望布局一致：每键 4 字节 RGBA，
/// alpha=0xFF 表示激活（`composite_keyboard` 再乘全局 overlay 透明度）。
fn active_to_key_colors(active: &[u32; crate::KEY_BUCKETS]) -> [u8; 1024] {
    let mut out = [0u8; 1024];
    for (index, &packed) in active.iter().take(128).enumerate() {
        let offset = index * 4;
        out[offset] = ((packed >> 24) & 0xFF) as u8;
        out[offset + 1] = ((packed >> 16) & 0xFF) as u8;
        out[offset + 2] = ((packed >> 8) & 0xFF) as u8;
        out[offset + 3] = (packed & 0xFF) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_to_key_colors_unpacks_rgba() {
        let mut active = [0u32; crate::KEY_BUCKETS];
        // key 60 激活：0xRRGGBBAA = 0x112233FF
        active[60] = 0x1122_33FF;
        let colors = active_to_key_colors(&active);
        assert_eq!(&colors[60 * 4..60 * 4 + 4], &[0x11, 0x22, 0x33, 0xFF]);
    }

    #[test]
    fn test_active_to_key_colors_ignores_aura_half() {
        let mut active = [0u32; crate::KEY_BUCKETS];
        // 光晕段 [128,256) 不应影响键色输出
        active[200] = 0xFFFF_FFFF;
        let colors = active_to_key_colors(&active);
        assert!(colors.iter().all(|&byte| byte == 0));
    }

    #[test]
    fn test_active_to_key_colors_zero_is_transparent() {
        let active = [0u32; crate::KEY_BUCKETS];
        let colors = active_to_key_colors(&active);
        assert_eq!(colors, [0u8; 1024]);
    }
}
