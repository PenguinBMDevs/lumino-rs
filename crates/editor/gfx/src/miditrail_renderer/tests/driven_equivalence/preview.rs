use super::*;

// ───────────────────────── 预览图导出（手动运行） ─────────────────────────
//
// 用法：cargo test -p lumino-gfx --lib preview_keyboard -- --ignored --nocapture
// 产物：%TEMP%\lumino_miditrail_preview\ 下 legacy/driven 全帧 + 键盘条带 + 差异图。

/// 键盘预览场景：128 键全覆盖 + 近键盘音符（最大限度暴露琴键与音符的遮挡关系）。
fn keyboard_preview_scene(tick: u32) -> Vec<NoteInstance> {
    let mut notes = dense_scene();
    // 每键一枚从键盘平面附近开始的音符（active：按下高亮 + 立方体压住键盘区上沿）。
    for k in 0..128u32 {
        notes.push(NoteInstance::new(
            (tick + 20 * (k % 4)) as f32,
            k as u8,
            140.0 + (k % 5) as f32 * 60.0,
            [0.9, 0.25, 0.2, 1.0],
            0,
        ));
    }
    notes
}

/// 写 RGBA8 PNG（预览诊断用）。
fn write_rgba_png(path: &std::path::Path, px: &[u8], w: u32, h: u32) {
    let file = std::fs::File::create(path).expect("创建 PNG 文件应成功");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("写 PNG 头应成功");
    writer.write_image_data(px).expect("写 PNG 数据应成功");
}

/// 取行区间 [y0, y1) 的 RGBA 子图。
fn crop_rows(px: &[u8], w: u32, y0: u32, y1: u32) -> Vec<u8> {
    let stride = (w * 4) as usize;
    let mut out = Vec::with_capacity(stride * (y1 - y0) as usize);
    out.extend_from_slice(&px[stride * y0 as usize..stride * y1 as usize]);
    out
}

/// 两图逐通道绝对差（×4 增益，肉眼可辨）+ 差异统计；alpha 恒 255（差异图不透明）。
fn diff_amplified(a: &[u8], b: &[u8]) -> (Vec<u8>, usize, u8) {
    let mut out = Vec::with_capacity(a.len());
    let mut over8 = 0usize;
    let mut max = 0u8;
    for (pa, pb) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
        for c in 0..3 {
            let d = pa[c].abs_diff(pb[c]);
            max = max.max(d);
            if d > 8 {
                over8 += 1;
            }
            out.push((d as u32 * 4).min(255) as u8);
        }
        out.push(255);
    }
    (out, over8, max)
}

/// 预览四联图：legacy / driven 全帧 + 键盘条带 + 放大差异图，另打键盘区差异统计。
#[test]
#[ignore = "预览图导出，手动运行"]
fn test_preview_keyboard_legacy_vs_driven() {
    let (_instance, device, queue) = test_device();
    let mut uniform = test_uniform();
    uniform.frame_width = 960;
    uniform.frame_height = 540;
    uniform.kb_height = 65;
    let notes = keyboard_preview_scene(uniform.tick);
    let (w, h) = (uniform.frame_width, uniform.frame_height);
    let tick_end = uniform.tick.saturating_add(miditrail_viewport_span(
        uniform.ppq,
        uniform.speed,
        uniform.z_far_distance,
    ));
    // 上传路径须按画家序传实例（顺序即 winner；未排序输入会与 legacy 分叉）。
    let paint_notes =
        paint_order_window(&notes, uniform.tick, tick_end, uniform.key_count as usize);

    let mut legacy_renderer = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_legacy"),
    });
    legacy_renderer.render_from_instances(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let legacy_tex = legacy_renderer.output_texture().expect("legacy 应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_rb_legacy"),
    });
    let legacy_px = readback_pixels(&device, &queue, encoder, legacy_tex, w, h);

    let mut driven_renderer = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_driven"),
    });
    driven_renderer.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let driven_tex = driven_renderer.output_texture().expect("driven 应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_rb_driven"),
    });
    let driven_px = readback_pixels(&device, &queue, encoder, driven_tex, w, h);

    // 生产默认（flat）：box 侧棱面差异不应带入生产路径——单独量测 flat 对照。
    legacy_renderer.flat_notes = true;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_legacy_flat"),
    });
    legacy_renderer.render_from_instances(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let legacy_flat_tex = legacy_renderer
        .output_texture()
        .expect("legacy flat 应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_rb_legacy_flat"),
    });
    let legacy_flat_px = readback_pixels(&device, &queue, encoder, legacy_flat_tex, w, h);
    driven_renderer.flat_notes = true;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_driven_flat"),
    });
    driven_renderer.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let driven_flat_tex = driven_renderer
        .output_texture()
        .expect("driven flat 应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_rb_driven_flat"),
    });
    let driven_flat_px = readback_pixels(&device, &queue, encoder, driven_flat_tex, w, h);

    // 导出路径（cull compact 直绑 + GPU 活跃聚合）：真机导出实际走的路径。
    let mut export_renderer = MiditrailRenderer::new(&device);
    export_renderer.seed_resident(&device, &queue, &notes);
    let tick_end = uniform.tick.saturating_add(miditrail_viewport_span(
        uniform.ppq,
        uniform.speed,
        uniform.z_far_distance,
    ));
    let prepared = export_renderer
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
    let compact = export_renderer
        .cull_compact_buffer()
        .cloned()
        .expect("FILL 后 compact 应就绪");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_export"),
    });
    export_renderer.render_gpu_driven_from_compact(
        &device,
        &queue,
        &mut encoder,
        &uniform,
        &compact,
        prepared.total,
        &prepared.active,
    );
    queue.submit(std::iter::once(encoder.finish()));
    let export_tex = export_renderer.output_texture().expect("导出路径应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_rb_export"),
    });
    let export_px = readback_pixels(&device, &queue, encoder, export_tex, w, h);

    // 生产默认平面（`3D音符` 关）：同法再渲一帧，单独出图（用户默认观感）。
    export_renderer.flat_notes = true;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_export_flat"),
    });
    export_renderer.render_gpu_driven_from_compact(
        &device,
        &queue,
        &mut encoder,
        &uniform,
        &compact,
        prepared.total,
        &prepared.active,
    );
    queue.submit(std::iter::once(encoder.finish()));
    let export_flat_tex = export_renderer
        .output_texture()
        .expect("导出台面路径应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_rb_export_flat"),
    });
    let export_flat_px = readback_pixels(&device, &queue, encoder, export_flat_tex, w, h);

    // 键盘条带 = 底部 30%（琴键 + 其上方近场音符）。
    let kb_y0 = h * 70 / 100;
    let dir = std::env::temp_dir().join("lumino_miditrail_preview");
    std::fs::create_dir_all(&dir).expect("创建预览目录应成功");
    write_rgba_png(&dir.join("legacy_full.png"), &legacy_px, w, h);
    write_rgba_png(&dir.join("driven_full.png"), &driven_px, w, h);
    write_rgba_png(
        &dir.join("legacy_keyboard.png"),
        &crop_rows(&legacy_px, w, kb_y0, h),
        w,
        h - kb_y0,
    );
    write_rgba_png(
        &dir.join("driven_keyboard.png"),
        &crop_rows(&driven_px, w, kb_y0, h),
        w,
        h - kb_y0,
    );
    write_rgba_png(&dir.join("export_full.png"), &export_px, w, h);
    write_rgba_png(
        &dir.join("export_keyboard.png"),
        &crop_rows(&export_px, w, kb_y0, h),
        w,
        h - kb_y0,
    );
    write_rgba_png(&dir.join("export_flat_full.png"), &export_flat_px, w, h);
    write_rgba_png(
        &dir.join("export_flat_keyboard.png"),
        &crop_rows(&export_flat_px, w, kb_y0, h),
        w,
        h - kb_y0,
    );
    let (diff_full, over8, max_diff) = diff_amplified(&legacy_px, &driven_px);
    write_rgba_png(&dir.join("diff_full.png"), &diff_full, w, h);
    // 生产默认（flat）：同画家序输入下应逐位一致（0 = 完全一致）。
    let (flat_diff, flat_over8, flat_max) = diff_amplified(&legacy_flat_px, &driven_flat_px);
    write_rgba_png(&dir.join("diff_flat.png"), &flat_diff, w, h);
    println!("PREVIEW_FLAT over8={flat_over8} max={flat_max}");

    // 导出路径 vs legacy（键盘验收是重点）：仅统计键盘条带差异像素。
    let mut export_kb_diff = 0usize;
    let mut export_other_diff = 0usize;
    for row in 0..h {
        for col in 0..w {
            let idx = (row as usize * (w * 4) as usize) + (col * 4) as usize;
            let row_diff = (0..4).any(|c| legacy_px[idx + c].abs_diff(export_px[idx + c]) > 8);
            if row_diff {
                if row >= kb_y0 {
                    export_kb_diff += 1;
                } else {
                    export_other_diff += 1;
                }
            }
        }
    }

    // 键盘区差异统计。
    let stride = (w * 4) as usize;
    let mut kb_diff_px = 0usize;
    let mut other_diff_px = 0usize;
    for row in 0..h {
        for col in 0..w {
            let idx = (row as usize * stride) + (col * 4) as usize;
            let row_diff = (0..4).any(|c| legacy_px[idx + c].abs_diff(driven_px[idx + c]) > 8);
            if row_diff {
                if row >= kb_y0 {
                    kb_diff_px += 1;
                } else {
                    other_diff_px += 1;
                }
            }
        }
    }
    println!(
        "PREVIEW_STATS over8={over8} max={max_diff} kb_diff_px={kb_diff_px} other_diff_px={other_diff_px} export_kb_diff={export_kb_diff} export_other_diff={export_other_diff} dir={}",
        dir.display()
    );
}
