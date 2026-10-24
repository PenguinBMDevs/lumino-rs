//! splat 全链路基准：点图元路径 vs 像素聚合（clear + compute + 合成）。
//!
//! 运行：
//! `LUMINO_PERF_NOTES=1000000,8000000,16000000,64000000 ITERS=5 \
//!  cargo test --profile fast-release -p lumino-gfx --lib bench_splat_full_render \
//!  -- --ignored --nocapture --test-threads=1`

use super::helpers::*;
use super::measure::{env_counts, env_iters};
use super::*;
use crate::note_renderer::NoteRenderer;

#[test]
#[ignore = "GPU probe：--ignored 手动运行"]
fn bench_splat_full_render() {
    let gpu = BenchGpu::new();
    let counts = env_counts(&gpu);
    let iters = env_iters();
    let camera = panorama_camera();
    let size = wgpu::Extent3d {
        width: BENCH_W,
        height: BENCH_H,
        depth_or_array_layers: 1,
    };

    eprintln!(
        "\n=== splat full render vs point · adapter={} · {iters} 轮中位数 ===",
        gpu.adapter_info
    );
    eprintln!(
        "{:>12} {:>12} {:>12} {:>12} {:>10}",
        "notes", "point_cold", "splat", "point_warm", "speedup"
    );

    for count in counts {
        let notes = synth_notes(count);
        let mut renderer = NoteRenderer::new_onion_skin(
            &gpu.device,
            &gpu.queue,
            gpu.format,
            crate::GpuEpoch::INITIAL,
        );
        renderer.set_view_state(&gpu.queue, 1, &[]);
        renderer.upload_instances(&notes, &gpu.device, &gpu.queue);
        renderer.prepare_direct(camera, &gpu.queue);

        let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("splat_bench_color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("splat_bench_depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::constants::rendering::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let point_cold =
            measure_point(&gpu, &mut renderer, camera, &color_view, &depth_view, iters);
        let splat = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
            assert!(
                renderer.prepare_splat_for_test(
                    enc,
                    &gpu.device,
                    &gpu.queue,
                    camera,
                    (BENCH_W, BENCH_H),
                ),
                "splat 准备失败"
            );
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("splat_bench_splat"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            assert!(renderer.draw_splat(&mut pass, None), "splat 绘制失败");
        }));
        let point_warm =
            measure_point(&gpu, &mut renderer, camera, &color_view, &depth_view, iters);

        eprintln!(
            "{:>12} {:>12.3} {:>12.3} {:>12.3} {:>10.2}x",
            count,
            point_cold,
            splat,
            point_warm,
            point_warm / splat.max(0.001)
        );
    }
}

fn measure_point(
    gpu: &BenchGpu,
    renderer: &mut NoteRenderer,
    camera: crate::note_renderer::types::CameraUniform,
    color_view: &wgpu::TextureView,
    depth_view: &wgpu::TextureView,
    iters: usize,
) -> f64 {
    ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        renderer.prepare_direct(camera, &gpu.queue);
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("splat_bench_point"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw_points(&mut pass, true, None);
    }))
}
