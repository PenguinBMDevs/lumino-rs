use super::super::prepare::prepare_renderers;
use super::super::render_pass::execute_render_pass;
use super::super::textures::{OffscreenTextureResources, ensure_textures};
use super::context::{PreviewPassContext, PreviewUploadContext, RenderFrameState};
use super::hires::update_hires_viewport;

/// 确保离屏纹理已创建，并在主音符实例版本变化时上传。
pub(super) fn ensure_offscreen_textures_and_upload_notes(context: &mut PreviewUploadContext<'_>) {
    let width = context.params.viewport_size.0.max(1);
    let height = context.params.viewport_size.1.max(1);

    let mut tex_resources = OffscreenTextureResources {
        device: &context.ctx.device,
        texture_format: context.ctx.texture_format,
        width,
        height,
        current_size: &mut *context.current_size,
        current_texture: &mut *context.current_texture,
        depth_texture: &mut *context.depth_texture,
        depth_texture_view: &mut *context.depth_texture_view,
        texture_view: &mut *context.texture_view,
        latest_texture_clone: &context.channels.latest_texture_clone,
        params: context.params,
    };
    // 普通离屏渲染保留 depth attachment（UI 预览可能需要）
    ensure_textures(&mut tex_resources, true);

    let note_version = context.channels.note_instances_buffer.version();
    if note_version != *context.last_note_version {
        *context.last_note_version = note_version;

        // 三缓冲状态机轮换：UI 线程 swap 后 reading 槽位必须被消费，
        // 否则后续 swap 会反复交换同一槽位。
        let note_instances =
            unsafe { context.channels.note_instances_buffer.acquire_read_buffer() };
        // 将 UI 线程构建的可见音符实例上传到 GPU。
        // 分离渲染线程模式下，音符数据通过 SwappableBuffer 共享，
        // 渲染线程在此处负责实际 GPU 上传与剔除信息更新。
        context.renderers.note.upload_instances(
            note_instances,
            &context.ctx.device,
            &context.ctx.queue,
        );
    }
}

/// 执行离屏渲染通道（在离屏纹理就绪时）：准备渲染器、更新高精度视口、提交命令。
pub(super) fn render_offscreen_pass(context: &mut PreviewPassContext<'_>) {
    if let (Some(_texture), Some(_depth_view)) =
        (&*context.current_texture, &*context.depth_texture_view)
    {
        let mut encoder =
            context
                .ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("offscreen_render_encoder"),
                });

        prepare_renderers(
            &mut *context.renderers,
            context.params,
            &context.channels.note_events_rx,
            &context.ctx.device,
            &context.ctx.queue,
            false,
        );

        let hires_visible = update_hires_viewport(
            &mut *context.hires_renderer,
            &*context.hires_meta,
            &*context.hires_config,
            context.params,
            &context.ctx.device,
            &context.ctx.queue,
        );
        let hires_visible_coords: Vec<crate::WaterfallTileCoord> =
            hires_visible.iter().map(|(c, _)| *c).collect();

        let mut frame = RenderFrameState {
            renderers: context.renderers,
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
            waterfall_renderer: &mut None,
            miditrail_renderer: &mut None,
        };
        execute_render_pass(
            &mut encoder,
            context.ctx,
            context.params,
            &hires_visible_coords,
            true,
            &mut frame,
        );

        context.ctx.queue.submit(std::iter::once(encoder.finish()));
    }
}
