//! GPU 渲染路径：窗口收集 + 构建渲染参数 + 发送 `RenderVideoFrame` 命令。

use lumino_core::tr;

use super::*;

/// 构建本帧 GPU 渲染参数并送入渲染线程；返回 `true` 表示终止渲染循环。
pub(super) fn send_gpu_frame(ctx: &mut MemoryEnqueueCtx, tick: u32) -> bool {
    let collect_all = !*ctx.notes_uploaded;
    // 窗口收集 + 排序 + 打包（GPU 渲染模式的最大 CPU 段，按渲染模式区分）
    let Some(params) = crate::profiling::tracy_zone!("app.build_params", {
        video_export::build_video_export_render_params(video_export::RenderParamsInput {
            width: ctx.width,
            height: ctx.height,
            tick,
            document: ctx.document,
            ppq: ctx.ppq,
            key_count: ctx.key_count,
            render_mode: ctx.render_mode,
            waterfall_scroll_speed: ctx.waterfall_scroll_speed,
            miditrail_z_far: ctx.miditrail_z_far,
            miditrail_view_mode: ctx.miditrail_view_mode,
            miditrail_normal_speed: ctx.miditrail_normal_speed,
            miditrail_top_speed: ctx.miditrail_top_speed,
            miditrail_3d_notes: ctx.miditrail_3d_notes,
            fps: ctx.fps_f64 as f32,
            visible_notes: ctx.visible_note_buf,
            note_instances_out: ctx.note_instances_buf,
            collect_all,
            window_state: ctx.window_state,
        })
    }) else {
        send_export_error(
            ctx.progress_tx,
            tr!("导出失败：当前渲染模式不应进入此分支（内部错误）"),
        );
        return true;
    };
    // 首帧全量数据已随本帧发出，后续帧只发 uniforms，复用 GPU 常驻数据。
    *ctx.notes_uploaded = true;

    if ctx
        .cmd_sender
        .send(RenderCommand::Control(ControlCommand::RenderVideoFrame {
            params: Box::new(params),
        }))
        .is_err()
    {
        tracing::error!("发送 RenderVideoFrame 命令失败");
        send_export_error(ctx.progress_tx, tr!("导出失败：渲染线程通信错误"));
        return true;
    }
    false
}
