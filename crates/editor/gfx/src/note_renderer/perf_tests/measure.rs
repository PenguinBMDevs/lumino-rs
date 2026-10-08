//! 基准测量 — Row/measure/打印（从 perf_tests.rs 拆出）

use super::helpers::*;
use super::*;

/// 单档位测量结果（打印与断言共用）。
pub(super) struct Row {
    pub(super) count: usize,
    pub(super) visible: u32,
    pub(super) clear_ms: f64,
    pub(super) cull_ms: f64,
    pub(super) draw_ms: f64,
    pub(super) total_ms: f64,
    /// 亚像素档位：`prepare_direct`（只写相机）+ 点图元直绘
    pub(super) point_ms: f64,
}

impl Row {
    fn render_share(&self) -> f64 {
        if self.total_ms <= 0.0 {
            0.0
        } else {
            self.draw_ms / self.total_ms * 100.0
        }
    }
}

/// 测量一档实例数在给定相机下的三段耗时。
pub(super) fn measure(gpu: &BenchGpu, count: usize, zoomed: bool, iters: usize) -> Row {
    let notes = synth_notes(count);
    // 与生产一致：钢琴卷帘的全量音符层是 onion_skin 渲染器（新渲染器实例，隔离状态）
    let mut renderer = NoteRenderer::new_onion_skin(&gpu.device, &gpu.queue, gpu.format);
    // 主音轨 = track_enc 1；无静音轨（与「主轨可见」生产态一致）
    renderer.set_view_state(&gpu.queue, 1, &[]);
    renderer.upload_instances(&notes, &gpu.device, &gpu.queue);
    let camera = if zoomed {
        zoomed_camera()
    } else {
        panorama_camera()
    };

    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_perf_color"),
        size: wgpu::Extent3d {
            width: BENCH_W,
            height: BENCH_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_perf_depth"),
        size: wgpu::Extent3d {
            width: BENCH_W,
            height: BENCH_H,
            depth_or_array_layers: 1,
        },
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
    let color_attachment = || wgpu::RenderPassColorAttachment {
        view: &color_view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(clear_color),
            store: wgpu::StoreOp::Store,
        },
        depth_slice: None,
    };
    let depth_attachment = || wgpu::RenderPassDepthStencilAttachment {
        view: &depth_view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Discard,
        }),
        stencil_ops: None,
    };

    // ① 纯 clear（空白 attach 基线，用于判断小规模下的测量底噪）
    let clear_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_clear"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }));

    // ② cull pass（prepare_pass：重置 indirect + 写 viewport uniform + 全量 dispatch）
    let cull_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        renderer.prepare_pass(enc, camera, &gpu.queue);
    }));

    // 可见数回读：确认相机档位语义（全景应≈全部可见，放大应远小于总数）
    let visible = read_u32(
        &gpu.device,
        &gpu.queue,
        renderer.indirect_buffer.inner(),
        4, // DrawIndirectArgs: vertex_count(4B) 之后是 instance_count
    );

    // ③ render pass（用上一步 cull 产出的可见列表 + indirect 参数，单测绘制段）
    let draw_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_draw"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, true, None);
    }));

    // ④ cull + render 同一 encoder（真实帧形状）
    let total_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        renderer.prepare_pass(enc, camera, &gpu.queue);
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_total"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, true, None);
    }));

    // ⑤ 亚像素档位：点图元直绘（不跑 cull，可见性判定在 vs_point 内）
    let point_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        renderer.prepare_direct(camera, &gpu.queue);
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_point"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw_points(&mut pass, true, None);
    }));

    Row {
        count,
        visible,
        clear_ms,
        cull_ms,
        draw_ms,
        total_ms,
        point_ms,
    }
}

pub(super) fn env_counts(gpu: &BenchGpu) -> Vec<usize> {
    let cap = gpu.max_instances(4); // 可见索引缓冲 4B/实例
    if let Ok(raw) = std::env::var("LUMINO_PERF_NOTES") {
        let parsed: Vec<usize> = raw
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    DEFAULT_COUNTS
        .iter()
        .copied()
        .filter(|c| *c <= cap)
        .collect()
}

pub(super) fn env_iters() -> usize {
    std::env::var("ITERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v >= 2)
        .unwrap_or(DEFAULT_ITERS)
}

/// 打印一张表的表头（含硬件环境——卡片验收要求固化硬件口径）。
pub(super) fn print_header(gpu: &BenchGpu, scene: &str, iters: usize) {
    eprintln!(
        "\n=== {scene} · adapter={} · {iters} 轮中位数（前 2 轮预热丢弃）===",
        gpu.adapter_info
    );
    eprintln!(
        "{:>12} {:>12} {:>8} {:>8} {:>8} {:>8} {:>9} {:>8} {:>9}",
        "notes", "visible", "clear", "cull", "draw", "cull+draw", "total", "render%", "point"
    );
}

pub(super) fn print_row(r: &Row) {
    eprintln!(
        "{:>12} {:>12} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>9.2} {:>7.1}% {:>8.2}",
        r.count,
        r.visible,
        r.clear_ms,
        r.cull_ms,
        r.draw_ms,
        r.cull_ms + r.draw_ms,
        r.total_ms,
        r.render_share(),
        r.point_ms
    );
}
