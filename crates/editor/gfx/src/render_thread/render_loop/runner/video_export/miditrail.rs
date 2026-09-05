//! Miditrail 3D 帧渲染：使用 3D 渲染管线写入颜色纹理后读回。
//!
//! 窗口集直达 3D 渲染器（24M 级文档下全量常驻意味着 390MB 显存＋
//! 390MB 镜像＋每帧两次全量扫描，约 100ms/帧，已实测；窗口传输按可见集收敛）。
//! 派生输入（`MiditrailNoteGpu`）由渲染器内部换算（跨帧复用暂存，不经过跨线程存储）。
//! 注意：钢琴卷帘共享缓冲（`frame.renderers.note`）此处不碰——3D 管线自有实例缓冲，
//! 往共享缓冲上传是纯死工作（memcpy＋write_buffer＋cull bind group 重建）。

use super::common::{CopyDrainInput, copy_texture_and_drain};
use crate::MiditrailRenderer;
use crate::MiditrailUniformGpu;
use crate::render_thread::params::RenderParams;
use crate::render_thread::render_loop::runner::context::{RenderContext, RenderFrameState};

/// Miditrail 3D 帧渲染：使用 3D 渲染管线写入颜色纹理后读回。
pub(super) fn handle_miditrail_frame(
    ctx: &RenderContext,
    params: RenderParams,
    frame: &mut RenderFrameState,
    width: u32,
    height: u32,
) {
    // 3D 管线自有实例缓冲，不经过钢琴共享缓冲（旧 `upload_instances` 是死工作，已删）。
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

    // 换算＋实例构建＋上传＋绘制全在渲染器内部（暂存跨帧复用，零每帧大分配）。
    let t_render = std::time::Instant::now();
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("miditrail_encoder"),
        });

    // 回退到 legacy CPU 路径（2026-09-05 driven 实验前的原始状态）：
    // UI 实测键盘顶层/前面层异常＋提速不达预期，先恢复最后已知良好状态，
    // 根因查明前 Normal 也不走 driven（`render_gpu_driven` 保留供 example 继续验证）。
    renderer.render_from_instances(
        &ctx.device,
        &ctx.queue,
        &mut encoder,
        &uniform,
        &params.note_instances,
    );
    let render_us = t_render.elapsed().as_micros() as u64;
    // 渲染侧分段打点（首 3 帧 + 每 300 帧）：与 UI 侧收集打点对齐，
    // 即可确认 recv 里 CPU 计算 vs GPU 绘制/读回各占多少。
    {
        static RENDER_DIAG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = RENDER_DIAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < 3 || n.is_multiple_of(300) {
            tracing::info!(
                "miditrail渲染打点[{n}]: render={render_us}us notes={}",
                params.note_instances.len()
            );
        }
    }

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
