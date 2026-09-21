use super::*;

/// 导出路径端到端：`cull_prepare` → `render_gpu_driven_from_compact` ≡
/// `render_gpu_driven`（CPU 扫描 + 上传路径）。
///
/// 两条路径的差别只在活跃键/光晕来源（GPU COUNT 顺带聚合 vs CPU 全量扫描）
/// 与音符实例来源（常驻 compact 直绑 vs CPU 上传）。键色逐位一致、光晕系数
/// ULP 级差异（`powf(0.3)`）→ 允许极小像素差异（差异像素占比 <0.1%）。
#[test]
fn test_driven_from_compact_matches_upload_path() {
    let (_instance, device, queue) = test_device();
    let uniform = test_uniform();
    let notes = dense_scene();
    let (w, h) = (uniform.frame_width, uniform.frame_height);
    let tick_end = uniform.tick.saturating_add(miditrail_viewport_span(
        uniform.ppq,
        uniform.speed,
        uniform.z_far_distance,
    ));

    // A：导出路径（常驻 → cull compact 直绑 + GPU 活跃聚合）。
    let mut from_compact = MiditrailRenderer::new(&device);
    from_compact.seed_resident(&device, &queue, &notes);
    let prepared = from_compact
        .cull_prepare(
            &device,
            &queue,
            CullWindow {
                tick_start: uniform.tick,
                tick_end,
                key_count: uniform.key_count as usize,
            },
            crate::CullActiveParams {
                ticks_per_second: uniform.ticks_per_second,
                fps: uniform.fps,
            },
        )
        .expect("cull_prepare 应成功");
    assert!(prepared.total > 0, "窗口非空（测试前提）");
    let compact = from_compact
        .cull_compact_buffer()
        .cloned()
        .expect("FILL 后 compact 应就绪");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_compact_render"),
    });
    from_compact.render_gpu_driven_from_compact(
        &device,
        &queue,
        &mut encoder,
        &uniform,
        &compact,
        prepared.total,
        &prepared.active,
    );
    queue.submit(std::iter::once(encoder.finish()));
    let tex_a = from_compact.output_texture().expect("compact 路径应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_compact_readback"),
    });
    let px_a = readback_pixels(&device, &queue, encoder, tex_a, w, h);

    // B：上传路径（CPU 扫描活跃键 + 上传音符）。输入取与 compact 同集合同序
    //（画家序：白块→黑块 + 键内 [未来 start 降序、同 start 稳定][已开始升序]）
    //——顺序即 winner，否则绘制序不同会分叉。
    let window_notes =
        paint_order_window(&notes, uniform.tick, tick_end, uniform.key_count as usize);

    let mut upload = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_upload_render"),
    });
    upload.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &window_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let tex_b = upload.output_texture().expect("上传路径应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_upload_readback"),
    });
    let px_b = readback_pixels(&device, &queue, encoder, tex_b, w, h);

    let compare = |a: &[u8], b: &[u8], mode: &str| {
        let mut over8 = 0usize;
        let mut max_diff = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            let d = x.abs_diff(*y);
            max_diff = max_diff.max(d);
            if d > 8 {
                over8 += 1;
            }
        }
        let ratio = over8 as f64 / a.len() as f64;
        assert!(
            ratio < 0.001,
            "{mode} compact 路径与上传路径差异超标：{ratio:.5}（超8通道 {over8}，最大差 {max_diff}）"
        );
    };
    compare(&px_a, &px_b, "box");

    // 生产默认是平面（`3D音符` 默认关）：同法再验 flat（音符走 QUAD_RANGE，琴键不变）。
    from_compact.flat_notes = true;
    upload.flat_notes = true;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_compact_render_flat"),
    });
    from_compact.render_gpu_driven_from_compact(
        &device,
        &queue,
        &mut encoder,
        &uniform,
        &compact,
        prepared.total,
        &prepared.active,
    );
    queue.submit(std::iter::once(encoder.finish()));
    let tex_a = from_compact.output_texture().expect("compact 路径应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_compact_readback_flat"),
    });
    let px_a = readback_pixels(&device, &queue, encoder, tex_a, w, h);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_upload_render_flat"),
    });
    upload.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &window_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let tex_b = upload.output_texture().expect("上传路径应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("equiv_upload_readback_flat"),
    });
    let px_b = readback_pixels(&device, &queue, encoder, tex_b, w, h);
    compare(&px_a, &px_b, "flat");
}
