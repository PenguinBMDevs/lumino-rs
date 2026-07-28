use crate::render_thread::export_pipeline::ExportPipeline;
use crate::render_thread::render_loop::Renderers;
use crate::render_thread::render_loop::runner::context::{
    RenderFrameState, VideoExportFrameContext, VideoExportSetupContext,
};

use super::handle_video_frame;

/// 启动视频导出：初始化 GPU→CPU 读回管线与专用 2D 渲染器。
pub(crate) fn start_video_export(context: VideoExportSetupContext<'_>) {
    tracing::info!(
        "视频导出开始: {}x{}, 初始化 GPU→CPU 读回管线",
        context.width,
        context.height
    );
    let mut pipeline = ExportPipeline::new(&context.ctx.device, context.width, context.height);
    pipeline.set_recycle_receiver(context.recycle_rx);
    *context.export_pipeline = Some(pipeline);
    *context.export_frame_tx = Some(context.frame_tx);
    // 创建无 depth attachment 的视频导出专用渲染器
    *context.export_renderers = Some(Renderers::new_for_video_export(
        &context.ctx.device,
        &context.ctx.queue,
        context.ctx.texture_format,
    ));
}

/// 渲染一帧视频导出：构造每帧状态并交给对应渲染模式。
pub(crate) fn render_video_frame_command(context: VideoExportFrameContext<'_>) {
    // 视频导出帧使用纯 2D 渲染器，确保 pipeline 与无 depth 的 RenderPass 兼容。
    let video_renderers = context
        .export_renderers
        .as_mut()
        .unwrap_or(&mut *context.renderers);
    let mut frame = RenderFrameState {
        renderers: video_renderers,
        current_texture: context.current_texture,
        depth_texture: context.depth_texture,
        depth_texture_view: context.depth_texture_view,
        texture_view: context.texture_view,
        current_size: context.current_size,
        last_note_version: context.last_note_version,
        latest_texture_clone: &context.channels.latest_texture_clone,
        hires_renderer: context.hires_renderer,
        hires_meta: context.hires_meta,
        hires_config: context.hires_config,
        export_pipeline: context.export_pipeline,
        export_frame_tx: context.export_frame_tx,
        waterfall_renderer: context.waterfall_renderer,
        miditrail_renderer: context.miditrail_renderer,
    };
    handle_video_frame(context.ctx, context.params, &mut frame, context.channels);
}
