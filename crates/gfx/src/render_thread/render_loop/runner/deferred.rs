use super::super::super::commands::ControlCommand;
use super::context::{
    DeferredCommandContext, HiResUploadContext, UploadHiResTileParams, VideoExportFrameContext,
    VideoExportSetupContext,
};
use super::hires::{handle_hires_control, upload_hires_video_tiles_command};
use super::video_export::{render_video_frame_command, start_video_export};

/// 处理单个延迟控制命令（视频导出 / 高精度贴图 / HiRes 控制）。
pub(super) fn handle_deferred_command(
    cmd: ControlCommand,
    context: &mut DeferredCommandContext<'_>,
) {
    match cmd {
        // ── 视频导出命令：在此内联处理（需要 GPU 资源 + 离屏纹理）──
        ControlCommand::StartVideoExport {
            width,
            height,
            frame_tx,
            recycle_rx,
        } => {
            start_video_export(VideoExportSetupContext {
                ctx: context.ctx,
                width,
                height,
                frame_tx,
                recycle_rx,
                export_pipeline: context.export_pipeline,
                export_frame_tx: context.export_frame_tx,
                export_renderers: context.export_renderers,
            });
        }
        ControlCommand::RenderVideoFrame { params } => {
            render_video_frame_command(VideoExportFrameContext {
                ctx: context.ctx,
                channels: context.channels,
                params: *params,
                export_renderers: context.export_renderers,
                renderers: context.renderers,
                current_texture: context.current_texture,
                depth_texture: context.depth_texture,
                depth_texture_view: context.depth_texture_view,
                texture_view: context.texture_view,
                current_size: context.current_size,
                last_note_version: context.last_note_version,
                hires_renderer: context.hires_renderer,
                hires_meta: context.hires_meta,
                hires_config: context.hires_config,
                export_pipeline: context.export_pipeline,
                export_frame_tx: context.export_frame_tx,
                waterfall_renderer: context.waterfall_renderer,
                miditrail_renderer: context.miditrail_renderer,
            });
        }
        ControlCommand::UploadHiResVideoTiles {
            tiles,
            config,
            track_count,
            key_count,
            total_ticks,
            ppq,
        } => {
            upload_hires_video_tiles_command(
                &mut HiResUploadContext {
                    ctx: context.ctx,
                    hires_renderer: context.hires_renderer,
                    hires_meta: context.hires_meta,
                    hires_config: context.hires_config,
                },
                UploadHiResTileParams {
                    tiles,
                    config,
                    track_count,
                    key_count,
                    total_ticks,
                    ppq,
                },
            );
        }
        ControlCommand::FinishVideoExport => {
            tracing::info!("视频导出完成，释放读回管线");
            *context.export_pipeline = None;
            *context.export_frame_tx = None;
            *context.export_renderers = None;
        }
        // ── HiRes 命令走原路径 ──
        cmd => {
            handle_hires_control(
                cmd,
                context.ctx,
                context.hires_result_tx,
                &context.channels.onion_progress,
                context.hires_renderer,
                context.hires_meta,
                context.hires_config,
            );
        }
    }
}
