//! Miditrail 3D 帧渲染：使用 3D 渲染管线写入颜色纹理后读回。
//!
//! 首帧全量一次上传常驻共享缓冲，后续帧只发 uniforms；
//! 可见过滤与派生输入（`MiditrailNoteGpu`）读 CPU 镜像按需换算，不经过跨线程存储。

use super::common::{CopyDrainInput, copy_texture_and_drain, note_instances_to_miditrail};
use crate::render_thread::params::RenderParams;
use crate::render_thread::render_loop::runner::context::{RenderContext, RenderFrameState};
use crate::{
    MiditrailRenderer, MiditrailUniformGpu,
    miditrail_renderer::{MIDITRAIL_MAX_Z_FAR_DISTANCE, MIDITRAIL_SCENE_DEPTH},
};

/// Miditrail 3D 帧渲染：使用 3D 渲染管线写入颜色纹理后读回。
pub(super) fn handle_miditrail_frame(
    ctx: &RenderContext,
    params: RenderParams,
    frame: &mut RenderFrameState,
    width: u32,
    height: u32,
) {
    // 1. 首帧全量上传常驻共享缓冲；后续帧复用常驻数据，读镜像做可见过滤。
    if !params.note_instances.is_empty() {
        frame
            .renderers
            .note
            .upload_instances(&params.note_instances, &ctx.device, &ctx.queue);
    }
    let mirror = frame.renderers.note.shared_cpu_instances();
    if frame.miditrail_renderer.is_none() {
        *frame.miditrail_renderer = Some(MiditrailRenderer::new(&ctx.device));
    }
    // 不变式：上面 is_none 判断后必然已创建；release 下若异常缺失则跳过本帧而非崩溃
    let renderer = match frame.miditrail_renderer.as_mut() {
        Some(r) => r,
        None => {
            debug_assert!(false, "miditrail_renderer 应已初始化（is_none 分支已创建）");
            return;
        }
    };

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
        view_mode: params.miditrail_view_mode,
        ticks_per_second,
        _padding1: 0,
    };

    // 派生输入按需换算（函数内临时值，不经过跨线程存储）。
    // 可见窗口与旧生产侧同公式：按 Z 显示距离缩放，避免白收集数倍音符。
    let z_far_scale = (params.miditrail_z_far.max(0.1) / MIDITRAIL_SCENE_DEPTH).clamp(
        0.1 / MIDITRAIL_SCENE_DEPTH,
        MIDITRAIL_MAX_Z_FAR_DISTANCE / MIDITRAIL_SCENE_DEPTH,
    );
    let speed = params.miditrail_speed.max(0.1);
    let viewport_tick_span =
        ((params.ppq.max(1.0) as u32 * 4 * ((4.0 / speed).round()).max(1.0) as u32).max(1) as f32
            * z_far_scale) as u32;
    let tick_start = params.miditrail_current_tick;
    let tick_end = tick_start.saturating_add(viewport_tick_span.max(1));
    let notes = note_instances_to_miditrail(mirror, tick_start, tick_end);
    let notes = &notes;

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("miditrail_encoder"),
        });

    renderer.render(&ctx.device, &ctx.queue, &mut encoder, &uniform, notes);

    let pipeline = match frame.export_pipeline.as_mut() {
        Some(p) => p,
        None => {
            debug_assert!(false, "export_pipeline 应已初始化（pipeline_ready 已保证）");
            return;
        }
    };
    let tx = match frame.export_frame_tx.as_ref() {
        Some(t) => t,
        None => {
            debug_assert!(false, "export_frame_tx 应已初始化（pipeline_ready 已保证）");
            return;
        }
    };

    if let Some(tex) = renderer.output_texture() {
        copy_texture_and_drain(CopyDrainInput {
            pipeline,
            tx,
            encoder,
            texture: tex,
            queue: &ctx.queue,
            width,
            height,
            label: "Miditrail",
        });
    } else {
        tracing::warn!("Miditrail 输出纹理未就绪");
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }
}
