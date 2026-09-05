//! 瀑布流帧渲染：compute shader 全 GPU 渲染，写入 storage texture 后读回。
//!
//! 音符零拷贝：权威 `note_instances` 先落共享 `GpuNoteBuffer`
//! （与钢琴卷帘导出同一份显存），compute 直接绑定该缓冲，
//! 派生分桶偏移与活跃键色按需换算。

use super::common::{CopyDrainInput, copy_texture_and_drain, note_instances_to_key_offsets};
use crate::miditrail_renderer::pack_color;
use crate::render_thread::params::RenderParams;
use crate::render_thread::render_loop::runner::context::{RenderContext, RenderFrameState};
use crate::{WaterfallRenderer, WaterfallUniformGpu, unpack_key_color};

/// 处理视频导出瀑布流帧：共享缓冲上传 → compute dispatch → staging 读回。
pub(super) fn handle_waterfall_frame(
    ctx: &RenderContext,
    params: RenderParams,
    frame: &mut RenderFrameState,
    width: u32,
    height: u32,
) {
    // 1. 权威数据落共享缓冲（钢琴卷帘导出上传的是同一块显存，零第二份拷贝）。
    //    空帧跳过上传：偏移表全零，shader 无命中，与旧行为一致。
    if !params.note_instances.is_empty() {
        frame
            .renderers
            .note
            .upload_instances(&params.note_instances, &ctx.device, &ctx.queue);
    }
    let (shared_buffer, _) = frame.renderers.note.gpu_note_buffer_for_sharing();

    // 初始化瀑布流渲染器
    if frame.waterfall_renderer.is_none() {
        *frame.waterfall_renderer = Some(WaterfallRenderer::new(&ctx.device));
    }
    // 不变式：上面 is_none 判断后必然已创建；release 下若异常缺失则跳过本帧而非崩溃
    let renderer = match frame.waterfall_renderer.as_mut() {
        Some(r) => r,
        None => {
            debug_assert!(false, "waterfall_renderer 应已初始化（is_none 分支已创建）");
            return;
        }
    };

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

    // 派生分桶偏移（唯一的每帧派生数据，129 u32，可忽略）
    let key_count = (params.max_key_index + 1.0).max(1.0) as usize;
    let key_offsets = note_instances_to_key_offsets(&params.note_instances, key_count);

    // 构建活跃键颜色数组（128 个 u32，0 表示无高亮）
    let tick_u = params.waterfall_current_tick;
    let mut active_key_colors = [0u32; 128];
    for n in &params.note_instances {
        let (key, rgb) = unpack_key_color(n.key_color);
        let start = n.start_length[0].max(0.0) as u32;
        let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
        if start <= tick_u && end > tick_u && (key as usize) < 128 {
            // color_packed 为 0xRRGGBBAA，alpha=153 表示 60% 混合
            // （与 shader 中 blend_key_color 的 alpha 参数匹配）
            active_key_colors[key as usize] =
                pack_color([rgb[0], rgb[1], rgb[2], 1.0]) & 0xFFFF_FF00 | 153u32;
        }
    }

    // 创建编码器
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("waterfall_encoder"),
        });

    // dispatch compute shader（音符读共享缓冲，不再自有拷贝）
    renderer.render(
        &ctx.device,
        &ctx.queue,
        &mut encoder,
        &uniform,
        &shared_buffer,
        params.note_instances.len(),
        &key_offsets,
        &active_key_colors,
    );

    // 获取输出纹理并拷贝到 staging buffer（流水线模式由 pipeline_ready 保证已初始化）
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
            label: "瀑布流",
        });
    } else {
        tracing::warn!("瀑布流输出纹理未就绪");
        // 即使没有输出纹理，也必须提交空编码器，否则 GPU 队列死锁
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }
}
