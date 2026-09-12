//! GPU-Driven 与 legacy 路径等价性：同一输入、两条管线、像素级对比。
//!
//! 背景：真机 dense 帧发现键盘区亮度差异（f0/f100 一致，f300 有 wash 缺失），
//! 本模块把差分收敛为可复现的单元测试：CPU 融合扫描逐位对比＋GPU 像素对比。

use super::super::instances::{
    build_aura_instances, compute_active_and_aura_for_compact, compute_active_keys,
    emit_aura_instances, update_key_positions,
};
use super::super::*;
use super::paint_order_window;
use crate::{CullWindow, NoteInstance};
use futures::executor::block_on;
use wgpu::util::DeviceExt;

/// 高密度合成场景：128 键全覆盖、起始交错（active/未开始/已结束混合）、
///
/// 同键叠音（稳定性）与黑白键重叠（覆盖序），复刻真机 dense 帧的特征。
fn dense_scene() -> Vec<NoteInstance> {
    let mut notes = Vec::new();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    // tick=5000 处约 1/3 active、1/3 未开始、1/3 已结束（collect 语义只留 end>tick，
    // 此处故意混入已结束音符验证 legacy/Driven 过滤一致性——Driven 由 shader 剔除）。
    for i in 0..6000u32 {
        let key = (next() % 128) as u8;
        let start = (next() % 9000) as f32;
        let len = (50 + next() % 1500) as f32;
        let shade = (i % 5) as f32 * 0.2;
        notes.push(NoteInstance::new(
            start,
            key,
            len,
            [0.2 + shade, 0.9 - shade * 0.5, 0.3, 1.0],
            0,
        ));
    }
    notes
}

fn test_uniform() -> MiditrailUniformGpu {
    MiditrailUniformGpu {
        tick: 5000,
        ppq: 480,
        key_count: 128,
        frame_width: 640,
        frame_height: 360,
        kb_height: 43,
        _reserved: 0,
        speed: 1.0,
        param1: 0.0,
        param2: 0.0,
        fps: 60.0,
        z_far_distance: 7.5,
        view_mode: MiditrailViewMode::Normal,
        ticks_per_second: 960.0,
        _padding1: 0,
    }
}

fn test_device() -> (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("测试需要可用的 wgpu 适配器");
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("miditrail_driven_equiv_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败");
    (instance, device, queue)
}

/// CPU 级：融合扫描（active＋aura）与 legacy 两次扫描逐位一致。
#[test]
fn test_fused_scan_matches_legacy() {
    let notes = dense_scene();
    let uniform = test_uniform();

    // legacy 输入：与 `render_from_instances` 逐 op 一致的换算。
    let derived: Vec<MiditrailNoteGpu> = notes
        .iter()
        .map(|n| {
            let (key, rgb) = crate::unpack_key_color(n.key_color);
            let start = n.start_length[0].max(0.0) as u32;
            let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
            MiditrailNoteGpu {
                key: key as u32,
                start_tick: start,
                end_tick: end,
                color_packed: crate::miditrail_renderer::pack_color([rgb[0], rgb[1], rgb[2], 1.0]),
                track_idx: 0,
                velocity: 100,
                channel: 0,
                _padding: 0,
            }
        })
        .collect();

    let expected_active = compute_active_keys(uniform.tick, &derived);
    let (actual_active, aura_sizes) = compute_active_and_aura_for_compact(
        uniform.tick,
        uniform.ticks_per_second,
        uniform.fps,
        &notes,
    );
    assert_eq!(
        expected_active.pressed, actual_active.pressed,
        "pressed 必须逐键一致"
    );
    assert_eq!(
        expected_active.colors, actual_active.colors,
        "激活颜色必须逐键一致"
    );

    // aura 实例逐位对比（legacy 全量扫描 vs 融合预聚合＋emit）。
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);
    let mut expected_auras = Vec::new();
    build_aura_instances(
        &uniform,
        &derived,
        &expected_active,
        &positions,
        &widths,
        &mut expected_auras,
    );
    let mut actual_auras = Vec::new();
    emit_aura_instances(
        &actual_active,
        &aura_sizes,
        uniform.key_count as usize,
        &positions,
        &widths,
        &mut actual_auras,
    );
    assert_eq!(
        expected_auras.len(),
        actual_auras.len(),
        "aura 实例数必须一致"
    );
    for (i, (a, b)) in expected_auras.iter().zip(actual_auras.iter()).enumerate() {
        assert_eq!(a.size, b.size, "第 {i} 个 aura 尺寸不一致");
        assert_eq!(a.pos, b.pos, "第 {i} 个 aura 位置不一致");
        assert_eq!(a.color_packed, b.color_packed, "第 {i} 个 aura 颜色不一致");
    }
}

/// 回读一帧 RGBA（去 row padding）。
fn readback_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = (width * 4).next_multiple_of(align);
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_driven_equiv_staging"),
        contents: &vec![0u8; (padded * height) as usize],
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    });
    let mut encoder = encoder;
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).expect("map_async 回调发送失败");
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv()
        .expect("map_async 回调未收到")
        .expect("map_async 失败");
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let s = (row * padded) as usize;
        out.extend_from_slice(&data[s..s + (width * 4) as usize]);
    }
    drop(data);
    staging.unmap();
    out
}

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

/// 生产默认（flat）：driven（compact 画家序）与 legacy 同序输入必须逐位一致。
///
/// flat 音符只有顶面四边形，无 box 侧棱面的 GPU/CPU 亚像素舍入分歧，故要求
/// **全通道 0 差异**（box 模式的侧棱面残差由 `test_driven_pixels_match_legacy`
/// 的 0.005 阈值兜底）。这是导出视频观感与 legacy 完全一致的硬回归锁。
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

    let mut over = 0usize;
    let mut max_diff = 0u8;
    for (a, b) in legacy_px.iter().zip(driven_px.iter()) {
        let d = a.abs_diff(*b);
        max_diff = max_diff.max(d);
        if d > 0 {
            over += 1;
        }
    }
    assert_eq!(
        over, 0,
        "flat 生产路径必须逐位一致（差异通道 {over}，最大差 {max_diff}）"
    );
}

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

// ───────────────────────── 共面叠音闪烁回归（RCA 锁） ─────────────────────────
//
// 根因：driven 曾用真实深度排序（`depth_write=true`）。同键叠音顶面完全共面，
// 深度只差 ULP：随帧滑动逐像素 winner 在两种颜色间翻转 → 真机视频"疯狂闪烁"
//（修复前本测试实测 946 像素红↔绿翻转；legacy 0）。legacy 画家排序注释即为
// 此设计（见 `instances.rs::build_note_instances`）。
//
// 修复后（compact 画家序 + `depth_write=false`，绘制顺序即最终次序）：同键叠音
// 按稳定序绘制，连续两帧红↔绿翻转必须为 0。
#[test]
fn test_coplanar_overlap_no_flicker() {
    let (_instance, device, queue) = test_device();
    let mut uniform = test_uniform();
    let (w, h) = (uniform.frame_width, uniform.frame_height);
    let frames = [5000u32, 5016];

    let scene = || {
        vec![
            NoteInstance::new(4000.0, 60, 8000.0, [1.0, 0.0, 0.0, 1.0], 0),
            NoteInstance::new(4000.0, 60, 4800.0, [0.0, 1.0, 0.0, 1.0], 0),
        ]
    };
    // 红↔绿主导翻转计数（阈值 32 抗提亮与量化噪声）。
    let flip_count = |a: &[u8], b: &[u8]| -> usize {
        a.as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0.iter())
            .filter(|(pa, pb)| {
                let sa = pa[0] as i32 - pa[1] as i32;
                let sb = pb[0] as i32 - pb[1] as i32;
                (sa > 32 && sb < -32) || (sa < -32 && sb > 32)
            })
            .count()
    };

    // A：driven（生产默认平面模式）。
    let mut driven_frames: Vec<Vec<u8>> = Vec::new();
    let mut driven = MiditrailRenderer::new(&device);
    driven.flat_notes = true;
    for &tick in &frames {
        uniform.tick = tick;
        let notes = scene();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_driven"),
        });
        driven.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &notes);
        queue.submit(std::iter::once(encoder.finish()));
        let tex = driven.output_texture().expect("driven 应有输出");
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_driven_rb"),
        });
        driven_frames.push(readback_pixels(&device, &queue, encoder, tex, w, h));
    }

    // B：legacy 画家路径（对照组，理论只差边缘移动像素）。
    let mut legacy_frames: Vec<Vec<u8>> = Vec::new();
    let mut legacy = MiditrailRenderer::new(&device);
    legacy.flat_notes = true;
    for &tick in &frames {
        uniform.tick = tick;
        let notes = scene();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_legacy"),
        });
        legacy.render_from_instances(&device, &queue, &mut encoder, &uniform, &notes);
        queue.submit(std::iter::once(encoder.finish()));
        let tex = legacy.output_texture().expect("legacy 应有输出");
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_legacy_rb"),
        });
        legacy_frames.push(readback_pixels(&device, &queue, encoder, tex, w, h));
    }

    let driven_flip = flip_count(&driven_frames[0], &driven_frames[1]);
    let legacy_flip = flip_count(&legacy_frames[0], &legacy_frames[1]);
    assert_eq!(legacy_flip, 0, "legacy 画家路径基准不得翻转");
    assert_eq!(
        driven_flip, 0,
        "同键共面叠音不得跨帧翻转（真实深度 ULP 闪烁回归；修复前实测 946）"
    );
}
