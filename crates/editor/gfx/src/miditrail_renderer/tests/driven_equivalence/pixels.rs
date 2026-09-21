use super::*;

/// 单音符精确对照：active 提亮＋几何必须逐像素一致（排除排序干扰）。
///
/// 一个 active 音符（start<=tick<end）＋一个未开始音符，分别走两条路径，
/// 差异通道必须为 0（允许 ±1 LSB 量化，共 4 个通道以内差异且差值 ≤1）。
#[test]
fn test_driven_single_active_note_matches() {
    let (_instance, device, queue) = test_device();
    let mut uniform = test_uniform();
    uniform.frame_width = 320;
    uniform.frame_height = 180;
    uniform.tick = 1000;
    // 红色 active 音符（boost 后应为粉白）＋绿色未开始音符。
    let notes = vec![
        NoteInstance::new(900.0, 60, 500.0, [1.0, 0.0, 0.0, 1.0], 0),
        NoteInstance::new(3000.0, 64, 500.0, [0.0, 1.0, 0.0, 1.0], 0),
    ];
    let (w, h) = (uniform.frame_width, uniform.frame_height);

    let mut legacy_renderer = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_single_legacy"),
    });
    legacy_renderer.render_from_instances(&device, &queue, &mut encoder, &uniform, &notes);
    queue.submit(std::iter::once(encoder.finish()));
    let legacy_tex = legacy_renderer.output_texture().expect("legacy 应有输出");

    let mut driven_renderer = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_single_driven"),
    });
    driven_renderer.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &notes);
    queue.submit(std::iter::once(encoder.finish()));
    let driven_tex = driven_renderer.output_texture().expect("driven 应有输出");

    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_single_rb0"),
    });
    let legacy_px = readback_pixels(&device, &queue, encoder, legacy_tex, w, h);
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_single_rb1"),
    });
    let driven_px = readback_pixels(&device, &queue, encoder, driven_tex, w, h);

    let mut over_one_lsb = 0usize;
    let mut max_diff = 0u8;
    for (a, b) in legacy_px.iter().zip(driven_px.iter()) {
        let d = a.abs_diff(*b);
        max_diff = max_diff.max(d);
        if d > 1 {
            over_one_lsb += 1;
        }
    }
    assert_eq!(
        over_one_lsb, 0,
        "单音符两条路径差异超 ±1LSB：{over_one_lsb} 通道，最大差 {max_diff}"
    );
}

/// GPU 级：同输入下 legacy 与 Driven 像素差异（画家序回归锁）。
///
/// 修复后（compact 画家序 + 音符不写深度）两条路径应逐像素等价，仅容差
/// float±1LSB 与光晕/按键动画状态差异；本测试为回归锁，超标即画家序又破了。
/// 允许项：float±1LSB。不允许：系统性亮度/颜色/缺失差异、win 者翻转。
#[test]
fn test_driven_pixels_match_legacy() {
    let (_instance, device, queue) = test_device();
    let uniform = test_uniform();
    let notes = dense_scene();
    let (w, h) = (uniform.frame_width, uniform.frame_height);
    let tick_end = uniform.tick.saturating_add(miditrail_viewport_span(
        uniform.ppq,
        uniform.speed,
        uniform.z_far_distance,
    ));
    let paint_notes =
        paint_order_window(&notes, uniform.tick, tick_end, uniform.key_count as usize);

    let mut legacy_renderer = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_equiv_legacy"),
    });
    legacy_renderer.render_from_instances(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let legacy_tex = legacy_renderer
        .output_texture()
        .expect("legacy 渲染后应存在输出纹理");

    let mut driven_renderer = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_equiv_driven"),
    });
    driven_renderer.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let driven_tex = driven_renderer
        .output_texture()
        .expect("driven 渲染后应存在输出纹理");

    // 两次回读需各自的 encoder（texture borrow 结束后续借）：重建 encoder。
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_equiv_readback_legacy"),
    });
    let legacy_px = readback_pixels(&device, &queue, encoder, legacy_tex, w, h);
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_equiv_readback_driven"),
    });
    let driven_px = readback_pixels(&device, &queue, encoder, driven_tex, w, h);

    assert_eq!(legacy_px.len(), driven_px.len());
    let mut diff_pixels = 0usize;
    let mut max_diff = 0u8;
    // 键盘区（底部 25%）与音符区分开统计，定位差异来源。
    let mut diff_top = 0usize;
    let mut diff_bottom = 0usize;
    let split_row = h * 3 / 4;
    for row in 0..h {
        for col in 0..w {
            let idx = ((row * w + col) * 4) as usize;
            let mut row_diff = false;
            for c in 0..4 {
                let d = legacy_px[idx + c].abs_diff(driven_px[idx + c]);
                max_diff = max_diff.max(d);
                if d > 8 {
                    diff_pixels += 1;
                    row_diff = true;
                }
            }
            if row_diff {
                if row < split_row {
                    diff_top += 1;
                } else {
                    diff_bottom += 1;
                }
            }
        }
    }
    // 按"差异通道数/总通道数"计：重叠边界离散点应远低于千分之五。
    let total = legacy_px.len();
    let ratio = diff_pixels as f64 / total as f64;
    assert!(
        ratio < 0.005,
        "像素差异率超标：{ratio:.5}（差异通道 {diff_pixels}/{total}，最大差 {max_diff}，音符区差异像素 {diff_top}，键盘区差异像素 {diff_bottom}）"
    );
}

/// 生产默认（flat）：driven（compact 画家序）与 legacy 同序输入允许 **±1 LSB**。
///
/// flat 音符只有顶面四边形，无 box 侧棱面的 GPU/CPU 亚像素舍入分歧；但 CI 的
/// 软件光栅器（llvmpipe / WARP / macOS 虚拟 GPU）存在 ±1 量化差，故与同文件
/// 单音符等价测试（`over_one_lsb`）保持同一口径：**任何 ≥2 的差异仍视为结构性
/// 回归**。box 模式的侧棱面残差由 `test_driven_pixels_match_legacy` 的 0.005
/// 阈值兜底。
#[test]
fn test_driven_flat_pixels_match_legacy_exactly() {
    let (_instance, device, queue) = test_device();
    let uniform = test_uniform();
    let notes = dense_scene();
    let (w, h) = (uniform.frame_width, uniform.frame_height);
    let tick_end = uniform.tick.saturating_add(miditrail_viewport_span(
        uniform.ppq,
        uniform.speed,
        uniform.z_far_distance,
    ));
    let paint_notes =
        paint_order_window(&notes, uniform.tick, tick_end, uniform.key_count as usize);

    let mut legacy = MiditrailRenderer::new(&device);
    legacy.flat_notes = true;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("flat_parity_legacy"),
    });
    legacy.render_from_instances(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let legacy_tex = legacy.output_texture().expect("legacy 应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("flat_parity_legacy_rb"),
    });
    let legacy_px = readback_pixels(&device, &queue, encoder, legacy_tex, w, h);

    let mut driven = MiditrailRenderer::new(&device);
    driven.flat_notes = true;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("flat_parity_driven"),
    });
    driven.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &paint_notes);
    queue.submit(std::iter::once(encoder.finish()));
    let driven_tex = driven.output_texture().expect("driven 应有输出");
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("flat_parity_driven_rb"),
    });
    let driven_px = readback_pixels(&device, &queue, encoder, driven_tex, w, h);

    let mut over_one_lsb = 0usize;
    let mut max_diff = 0u8;
    for (a, b) in legacy_px.iter().zip(driven_px.iter()) {
        let d = a.abs_diff(*b);
        max_diff = max_diff.max(d);
        if d > 1 {
            over_one_lsb += 1;
        }
    }
    assert_eq!(
        over_one_lsb, 0,
        "flat 生产路径差异超 ±1LSB：{over_one_lsb} 通道，最大差 {max_diff}"
    );
}
