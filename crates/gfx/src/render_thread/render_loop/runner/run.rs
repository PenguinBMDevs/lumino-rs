use std::sync::{Arc, atomic::Ordering};
use std::time::Instant;

use super::super::super::commands::ControlCommand;
use super::super::super::export_pipeline::ExportPipeline;
use super::super::super::params::RenderParams;
use super::super::commands::process_commands;
use super::super::render_pass::update_stats;
use super::context::{
    DeferredCommandContext, PreviewPassContext, PreviewUploadContext, RenderContext,
    RenderThreadChannels,
};
use super::deferred::handle_deferred_command;
use super::hires::drain_hires_stream;
use super::preview::{ensure_offscreen_textures_and_upload_notes, render_offscreen_pass};
use super::types::HiResStreamMsg;
use super::video_export::advance_export_inflight;

/// 运行渲染线程主循环
pub fn run_render_thread(ctx: RenderContext, channels: RenderThreadChannels) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut renderers = super::super::Renderers::new(&ctx.device, &ctx.queue, ctx.texture_format);
    // 视频导出使用独立的纯 2D 渲染器，避免 depth-stencil 状态与普通预览不一致。
    let mut export_renderers: Option<super::super::Renderers> = None;

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);
    let mut last_note_version: u64 = 0;

    // 高精度洋葱皮渲染器状态
    let mut hires_renderer: Option<crate::HiResRenderer> = None;
    let mut hires_meta = None;
    let mut hires_config = None;
    let mut deferred: Vec<ControlCommand> = Vec::new();

    // 视频导出读回管线状态
    let mut export_pipeline: Option<ExportPipeline> = None;
    let mut export_frame_tx = None;

    // 视频导出专用 GPU 渲染器（跨帧复用，避免每帧重建 pipeline）
    let mut waterfall_renderer: Option<crate::WaterfallRenderer> = None;
    let mut miditrail_renderer: Option<crate::MiditrailRenderer> = None;

    // ★ 后台生成线程通过有界同步通道流式传回贴图（容量1，背压）★
    // sync_channel(1)：channel 满时 send 阻塞，强制后台等渲染线程消费，
    // 防止无界积压导致 CPU 内存峰值（对应"装袋期间工人等着"）
    let (hires_result_tx, hires_result_rx) = std::sync::mpsc::sync_channel::<HiResStreamMsg>(1);

    while channels.running.load(Ordering::Relaxed) {
        // 处理命令
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        let has_params = process_commands(
            &channels.command_receiver,
            &mut latest_params,
            &mut should_shutdown,
            &mut deferred,
        );

        // 处理延迟的控制命令
        process_deferred_commands(
            &mut DeferredCommandContext {
                ctx: &ctx,
                channels: &channels,
                renderers: &mut renderers,
                export_renderers: &mut export_renderers,
                current_texture: &mut current_texture,
                depth_texture: &mut depth_texture,
                depth_texture_view: &mut depth_texture_view,
                texture_view: &mut texture_view,
                current_size: &mut current_size,
                last_note_version: &mut last_note_version,
                hires_renderer: &mut hires_renderer,
                hires_meta: &mut hires_meta,
                hires_config: &mut hires_config,
                export_pipeline: &mut export_pipeline,
                export_frame_tx: &mut export_frame_tx,
                waterfall_renderer: &mut waterfall_renderer,
                miditrail_renderer: &mut miditrail_renderer,
                hires_result_tx: &hires_result_tx,
            },
            &mut deferred,
        );

        // 推进视频导出 inflight：即使没有新的 RenderVideoFrame 命令，
        // 也需要 try_read 已就绪的帧数据并发回 Runner，否则 inflight 满后
        // Runner 阻塞在 frame_rx.recv()，渲染线程也不再调用 try_read，形成死锁。
        advance_export_inflight(&mut export_pipeline, &export_frame_tx);

        // ★ 流式接收：每帧循环 try_recv，收到已合并像素立即 upload（GPU DMA，非阻塞）★
        drain_hires_stream(
            &hires_result_rx,
            &ctx,
            &mut hires_renderer,
            &channels.onion_progress,
        );

        if should_shutdown {
            break;
        }

        // 执行渲染（离屏纹理）
        if has_params && let Some(ref params) = latest_params {
            puffin::profile_scope!("wgpu_render_thread_frame");
            let frame_start = Instant::now();

            ensure_offscreen_textures_and_upload_notes(&mut PreviewUploadContext {
                ctx: &ctx,
                channels: &channels,
                renderers: &mut renderers,
                current_texture: &mut current_texture,
                depth_texture: &mut depth_texture,
                depth_texture_view: &mut depth_texture_view,
                texture_view: &mut texture_view,
                current_size: &mut current_size,
                last_note_version: &mut last_note_version,
                params,
            });

            render_offscreen_pass(&mut PreviewPassContext {
                ctx: &ctx,
                params,
                channels: &channels,
                renderers: &mut renderers,
                current_texture: &mut current_texture,
                depth_texture: &mut depth_texture,
                depth_texture_view: &mut depth_texture_view,
                texture_view: &mut texture_view,
                current_size: &mut current_size,
                last_note_version: &mut last_note_version,
                hires_renderer: &mut hires_renderer,
                hires_meta: &mut hires_meta,
                hires_config: &mut hires_config,
                export_pipeline: &mut export_pipeline,
                export_frame_tx: &mut export_frame_tx,
            });

            // 更新统计
            let frame_time = frame_start.elapsed();
            update_stats(
                &mut frame_count,
                &mut fps_update_time,
                frame_time,
                params,
                &channels.stats_clone,
            );
        }
    }

    tracing::info!("Render thread stopped");
}

/// 处理延迟队列中的控制命令（视频导出 / 高精度贴图 / HiRes 控制）。
fn process_deferred_commands(
    context: &mut DeferredCommandContext<'_>,
    deferred: &mut Vec<ControlCommand>,
) {
    for cmd in deferred.drain(..) {
        handle_deferred_command(cmd, context);
    }
}
