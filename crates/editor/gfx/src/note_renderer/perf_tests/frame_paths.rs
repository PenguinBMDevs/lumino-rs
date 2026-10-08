//! 基准测量 — 三条渲染路径整帧对比（从 perf_tests.rs 拆出）

use super::helpers::*;
use super::*;

/// 面板瀑布流预览之外的另一种负载画像：**同 key 大量堆叠**（黑乐谱真实形态）。
///
/// 均匀铺满（`synth_notes`）测的是「图元数量」；本函数测的是「重叠深度」——
/// 每簇 `DENSE_CLUSTER_NOTES` 个同 key 音符按 `DENSE_TICK_SPACING` 递推铺开，
/// 全景缩放下它们会坍缩到同一两根像素柱上，形成几十~上百层叠加：
/// 这正是 PREF-005 所说「同 key 行音符在时间上大量重叠，同一屏幕像素柱叠加
/// 几十上百个 quad」的负载。
pub(super) const DENSE_CLUSTER_NOTES: usize = 128;
/// 同簇内相邻音符的 tick 间距（1/16 音符）
pub(super) const DENSE_TICK_SPACING: f32 = 120.0;

/// 生成稠密堆叠场景，返回 `(实例, 全曲 tick 跨度)`。
pub(super) fn synth_notes_dense(count: usize) -> (Vec<NoteInstance>, f32) {
    let mut notes = Vec::with_capacity(count);
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for i in 0..count {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // 每 128 个音符共享一个 key（簇内 tick 递推），簇间换 key —— 复刻黑乐谱
        // 「同一键位高速重复」+「跨键位密集跑动」两种叠加
        let cluster = i / DENSE_CLUSTER_NOTES;
        let within = i % DENSE_CLUSTER_NOTES;
        let key = 21 + ((cluster as u32).wrapping_mul(7) % 88);
        let tick =
            cluster as f32 * DENSE_CLUSTER_NOTES as f32 * 0.25 + within as f32 * DENSE_TICK_SPACING;
        notes.push(NoteInstance::new(
            tick,
            key as u8,
            NOTE_LEN,
            [0.2, 0.55, 1.0, 1.0],
            (1u32 << 16) | 1,
        ));
        let _ = state;
    }
    let span = notes
        .last()
        .map_or(NOTE_LEN, |n| n.start_length[0] + NOTE_LEN);
    (notes, span)
}

/// 全景相机（按给定全曲 tick 跨度把整首歌缩到屏内）。
pub(super) fn panorama_camera_for(total_ticks: f32) -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [
            (BENCH_W as f32 - KEYBOARD_W) / total_ticks.max(1.0),
            (BENCH_H as f32 - RULER_H) / KEY_COUNT as f32,
        ],
        viewport_size: [BENCH_W as f32, BENCH_H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [BENCH_W as f32, BENCH_H as f32],
        keyboard_width: KEYBOARD_W,
        ruler_height: RULER_H,
        max_key_index: (KEY_COUNT - 1) as f32,
        _padding: [0.0; 3],
    }
}

/// PREF-005 的三条待对比路径。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FramePath {
    /// 改动前基线：alpha 混合管线 + compute cull + 可见索引绘制
    CullBlended,
    /// 只去掉混合：不透明管线 + compute cull
    CullOpaque,
    /// 本卡路径：不透明管线 + VS 内自判可见 + 按实例序直绘（无 compute cull）
    DirectOpaque,
}

/// 测量三条路径的**整帧**耗时（含各自的准备阶段），返回 `[mixed, cull, direct]`（ms）。
pub(super) fn measure_frame_paths(
    gpu: &BenchGpu,
    notes: &[NoteInstance],
    camera: CameraUniform,
    iters: usize,
) -> [f64; 3] {
    let size = wgpu::Extent3d {
        width: BENCH_W,
        height: BENCH_H,
        depth_or_array_layers: 1,
    };
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vs_cull_ab_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vs_cull_ab_depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let clear_color = wgpu::Color {
        r: 0.05,
        g: 0.05,
        b: 0.05,
        a: 1.0,
    };

    let mut renderer = NoteRenderer::new_onion_skin(&gpu.device, &gpu.queue, gpu.format);
    renderer.set_view_state(&gpu.queue, 1, &[]);
    renderer.upload_instances(notes, &gpu.device, &gpu.queue);

    // **双向轮转取最小值**：单项测量会被顺序效应污染（第一项常测得偏慢——
    // 时钟态/冷缓存），单轮顺序采样得出的「A 比 B 快 40%」可能纯属顺序假象。
    // 正序 + 逆序各测一轮，取各路径两轮最小值，消除顺序偏置。
    let mut out = [f64::INFINITY; 3];
    for reversed in [false, true] {
        let mut order = [
            FramePath::CullBlended,
            FramePath::CullOpaque,
            FramePath::DirectOpaque,
        ];
        if reversed {
            order.reverse();
        }
        for path in order {
            let slot = match path {
                FramePath::CullBlended => 0,
                FramePath::CullOpaque => 1,
                FramePath::DirectOpaque => 2,
            };
            // 混合基线：把横向 quad 管线换成混合变体（复刻改动前状态）
            if path == FramePath::CullBlended {
                renderer.rebuild_blended_quad_pipeline_for_bench(&gpu.device, gpu.format);
            }
            let elapsed = ms(median_time(
                &gpu.device,
                &gpu.queue,
                iters,
                |enc| match path {
                    FramePath::CullBlended | FramePath::CullOpaque => {
                        renderer.prepare_pass(enc, camera, &gpu.queue);
                        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("vs_cull_ab_cull_path"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &color_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(clear_color),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &depth_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: wgpu::StoreOp::Discard,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        renderer.draw(&mut pass, true, None);
                    }
                    FramePath::DirectOpaque => {
                        renderer.prepare_direct(camera, &gpu.queue);
                        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("vs_cull_ab_direct_path"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &color_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(clear_color),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &depth_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: wgpu::StoreOp::Discard,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        renderer.draw_direct(&mut pass, true, None);
                    }
                },
            ));
            out[slot] = out[slot].min(elapsed);
        }
    }
    out
}
