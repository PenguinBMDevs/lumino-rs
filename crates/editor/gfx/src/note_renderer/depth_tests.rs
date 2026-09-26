//! 音符深度编码验证：精度预算（CPU 孪生）与「绘制顺序无关」的像素证据。
//!
//! 背景：cull.wgsl 每个 workgroup 由线程 0 抢占式 `atomicAdd` 输出槽位，可见实例的
//! **输出顺序**由 GPU 调度决定、帧间不稳定；而音符管线是 `LessEqual` +
//! `depth_write_enabled=true`，同深度「后画者胜」——赢家随可见缓冲顺序逐帧随机，
//! 表现为重叠区描边闪烁。修复手段是让深度只由「跨帧稳定的 chunk 内源索引」派生。
//!
//! 本模块给出两类可运行证据：
//!   1. 精度预算（CPU 孪生，无需 GPU）：深度严格单调、不侵占相邻轨道层、不越远平面；
//!   2. 像素证据（GPU）：同一场景、同一源数据，仅改变可见索引的**输出顺序**
//!      （复刻 cull 的随机槽位分配），连续 64 帧回读像素必须逐位一致。
//!
//! 已知边界（本卡范围外）：预览音符固定在预览层 `depth = 0.0`（恒覆盖主轨），
//! 预览层内部多个预览音符之间仍共享同一深度；预览批次为绘制/hover/i2m 单音符或
//! 拖拽选区，重叠叠音需再引入预览层微深度（见卡片关联问题）。

use super::NoteRenderer;
use super::types::{CameraUniform, DrawIndirectArgs};
use crate::constants::rendering::DEPTH_FORMAT;

// ═══ 1. 精度预算：shader `tie_break_depth` 的 CPU 孪生 ═══════════════════════
//
// 下列常量/函数必须与 4 个音符 shader（note / note_vertical / onion_note /
// onion_note_vertical）中的同名定义逐字对应——`shader_sources_share_depth_contract`
// 测试守住这份契约。

/// 相邻轨道深度间隔（2^-16）——对应 shader `TRACK_DEPTH_STEP`
const TRACK_DEPTH_STEP: f32 = 1.0 / 65536.0;
/// 主音轨基深度（2^-17）——对应 shader `MAIN_TRACK_DEPTH_BASE`
const MAIN_TRACK_DEPTH_BASE: f32 = 1.0 / 131072.0;
/// 轨内微深度索引上界（2^23 - 1）——对应 shader `TIE_BREAK_INDEX_LIMIT`
const TIE_BREAK_INDEX_LIMIT: u32 = 8_388_607;

/// shader `tie_break_depth` 的 CPU 孪生：把稳定源索引注入基深度的尾数低位。
fn tie_break_depth(base: f32, visible_index: u32) -> f32 {
    let layer_gap = (base + TRACK_DEPTH_STEP).to_bits() - base.to_bits();
    let room_to_far = if base >= 1.0 {
        0
    } else {
        1.0f32.to_bits() - base.to_bits()
    };
    let budget = layer_gap.min(room_to_far);
    if budget < 2 {
        return base;
    }
    let k = visible_index.min(((budget - 1) / 2).min(TIE_BREAK_INDEX_LIMIT));
    f32::from_bits(base.to_bits() + k)
}

/// 洋葱皮轨道编码 `track_enc`（= `track_idx + 1`）的基深度。
fn onion_track_base(track_enc: u32) -> f32 {
    (track_enc + 1) as f32 * TRACK_DEPTH_STEP
}

/// 轨道编码 → 基深度（0 = 主音轨，其余为洋葱皮轨道编码）
fn track_base_for(track_enc: u32) -> f32 {
    if track_enc == 0 {
        MAIN_TRACK_DEPTH_BASE
    } else {
        onion_track_base(track_enc)
    }
}

/// 该基深度下轨内微深度可用的最大索引（饱和上界）。
fn index_cap(base: f32) -> u32 {
    let layer_gap = (base + TRACK_DEPTH_STEP).to_bits() - base.to_bits();
    let room_to_far = if base >= 1.0 {
        0
    } else {
        1.0f32.to_bits() - base.to_bits()
    };
    let budget = layer_gap.min(room_to_far);
    if budget < 2 {
        0
    } else {
        ((budget - 1) / 2).min(TIE_BREAK_INDEX_LIMIT)
    }
}

/// 采样覆盖轨道的轨道编码（含主音轨、首末洋葱皮轨道与若干中间值）。
const SAMPLED_TRACKS: [u32; 10] = [0, 1, 2, 3, 63, 64, 1024, 65533, 65534, 65535];

/// 采样索引（含 0、饱和上界附近、饱和之后与 u32 上界），升序去重。
fn probe_indices(base: f32) -> Vec<u32> {
    let cap = index_cap(base);
    let mut out = vec![
        0u32,
        1,
        2,
        3,
        255,
        4096,
        1_048_575,
        cap.saturating_sub(1),
        cap,
        cap.saturating_add(1),
        u32::MAX - 1,
        u32::MAX,
    ];
    out.sort_unstable();
    out.dedup();
    out
}

/// 主音轨基深度处的微深度步长恰好是一个 f32 ulp（2^-40）——即卡片要求的
/// `depth += index × EPSILON, EPSILON = 2^-40` 在主轨上逐位等价。
#[test]
fn test_main_track_micro_step_is_2_pow_minus_40() {
    let step = tie_break_depth(MAIN_TRACK_DEPTH_BASE, 1) - MAIN_TRACK_DEPTH_BASE;
    assert_eq!(
        step,
        2f32.powi(-40),
        "主音轨微深度步长应为 2^-40（f32 在 2^-17 处的一个 ulp）"
    );
    assert_eq!(
        tie_break_depth(MAIN_TRACK_DEPTH_BASE, 0),
        MAIN_TRACK_DEPTH_BASE,
        "索引 0 不应产生任何偏移"
    );
}

/// 预览层（0.0）恒在主音轨之前，主音轨恒在洋葱皮之前。
#[test]
fn test_layer_order_preview_before_main_before_onion() {
    const { assert!(0.0f32 < MAIN_TRACK_DEPTH_BASE) };
    assert!(
        MAIN_TRACK_DEPTH_BASE < onion_track_base(1),
        "主音轨基深度必须小于首个洋葱皮轨道层"
    );
    // 主音轨在轨内微深度拉满后仍不得越过首个洋葱皮轨道
    let main_max = tie_break_depth(MAIN_TRACK_DEPTH_BASE, index_cap(MAIN_TRACK_DEPTH_BASE));
    assert!(
        main_max < onion_track_base(1),
        "主音轨最大深度 {main_max} 侵占了洋葱皮首层 {}",
        onion_track_base(1)
    );
}

/// 轨内微深度严格单调：索引大者深度大（绘制顺序无关的确定性裁决）。
#[test]
fn test_in_track_depth_strictly_increasing_within_cap() {
    for track in SAMPLED_TRACKS {
        let base = track_base_for(track);
        let cap = index_cap(base);
        let mut prev = tie_break_depth(base, 0);
        for idx in 1..=cap.min(4096) {
            let depth = tie_break_depth(base, idx);
            assert!(
                depth > prev,
                "轨道 {track} 索引 {idx} 深度 {depth} 未严格大于前一深度 {prev}"
            );
            prev = depth;
        }
        // 饱和上界内任意两个采样索引必须可区分
        let samples: Vec<u32> = probe_indices(base)
            .into_iter()
            .filter(|&i| i <= cap)
            .collect();
        for (a, b) in samples.iter().zip(samples.iter().skip(1)) {
            assert!(
                tie_break_depth(base, *a) < tie_break_depth(base, *b),
                "轨道 {track} 索引 {a} 与 {b} 深度未严格递增（cap={cap}）"
            );
        }
    }
}

/// 超出饱和上界的索引（极端黑乐谱场景）必须饱和到上界深度，
/// 既不越层、也不越远平面——退化为确定性（不再随机）。
#[test]
fn test_indices_beyond_cap_saturate_deterministically() {
    for track in SAMPLED_TRACKS {
        let base = track_base_for(track);
        let cap = index_cap(base);
        let saturated = tie_break_depth(base, cap);
        for idx in [cap.saturating_add(1), u32::MAX - 1, u32::MAX] {
            assert_eq!(
                tie_break_depth(base, idx),
                saturated,
                "轨道 {track} 索引 {idx} 未饱和到上界深度"
            );
        }
        assert!(saturated - base < TRACK_DEPTH_STEP);
    }
}

/// 精度预算硬约束：同轨最大偏移 < 相邻轨道深度间隔（2^-16）。
#[test]
fn test_in_track_max_offset_below_track_step() {
    for track in SAMPLED_TRACKS {
        let base = track_base_for(track);
        for idx in probe_indices(base) {
            let offset = tie_break_depth(base, idx) - base;
            assert!(
                (0.0..TRACK_DEPTH_STEP).contains(&offset),
                "轨道 {track} 索引 {idx} 偏移 {offset} 越过轨道层间隔 {TRACK_DEPTH_STEP}"
            );
        }
    }
}

/// 跨轨道叠压关系不回归：任意轨道的深度区间不与相邻轨道区间相交。
#[test]
fn test_adjacent_track_layers_do_not_overlap() {
    let mut previous_max = tie_break_depth(MAIN_TRACK_DEPTH_BASE, index_cap(MAIN_TRACK_DEPTH_BASE));
    for track in 1..=65535u32 {
        if track > 4096 && track < 65533 {
            continue; // 中间轨道逐个穷举无必要，首末段已覆盖
        }
        let base = onion_track_base(track);
        let min = tie_break_depth(base, 0);
        assert!(
            min > previous_max,
            "轨道 {track} 的最小深度 {min} 不大于前一轨道的最大深度 {previous_max}"
        );
        previous_max = tie_break_depth(base, index_cap(base));
    }
}

/// 任意轨道深度都必须落在 NDC 远平面（z=1）之内，否则会被裁剪导致音符缺失。
#[test]
fn test_all_track_depths_stay_inside_far_plane() {
    for track in SAMPLED_TRACKS {
        let base = track_base_for(track);
        for idx in probe_indices(base) {
            let depth = tie_break_depth(base, idx);
            assert!(
                (0.0..=1.0).contains(&depth),
                "轨道 {track} 索引 {idx} 深度 {depth} 越出 [0, 1]"
            );
        }
    }
}

/// 4 个音符 shader 必须共享同一份深度契约（常量与函数签名逐字一致），
/// 并已把静音轨裁剪从「NDC z=2.0 深度裁剪」改为与 depth attachment 无关的退化几何。
#[test]
fn test_shader_sources_share_depth_contract() {
    const SOURCES: [(&str, &str); 4] = [
        ("note", include_str!("../shaders/note.wgsl")),
        (
            "note_vertical",
            include_str!("../shaders/note_vertical.wgsl"),
        ),
        ("onion_note", include_str!("../shaders/onion_note.wgsl")),
        (
            "onion_note_vertical",
            include_str!("../shaders/onion_note_vertical.wgsl"),
        ),
    ];
    const CONTRACT: [&str; 4] = [
        "const TRACK_DEPTH_STEP: f32 = 1.0 / 65536.0;",
        "const MAIN_TRACK_DEPTH_BASE: f32 = 1.0 / 131072.0;",
        "const TIE_BREAK_INDEX_LIMIT: u32 = 8388607u;",
        "fn tie_break_depth(base: f32, visible_index: u32) -> f32 {",
    ];
    for (label, source) in SOURCES {
        for needle in CONTRACT {
            assert!(
                source.contains(needle),
                "{label}.wgsl 缺少深度契约片段：{needle}"
            );
        }
        assert!(
            !source.contains("vec4<f32>(0.0, 0.0, 2.0, 1.0)"),
            "{label}.wgsl 仍在使用依赖 depth attachment 的 NDC z=2.0 静音裁剪"
        );
    }
}

// ═══ 2. 像素证据：绘制顺序无关（GPU）════════════════════════════════════════

const TEST_W: u32 = 256;
const TEST_H: u32 = 144;
/// 连续渲染帧数（验收要求 ≥ 60 帧像素逐位一致）
const ORDER_FRAMES: u32 = 64;

/// 测试用相机：x 方向 2 px/tick，y 方向 10 px/key，key 60 落在 y ∈ [0, 10)。
fn test_camera() -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [2.0, 10.0],
        viewport_size: [TEST_W as f32, TEST_H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [TEST_W as f32, TEST_H as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: 60.0,
        _padding: [0.0; 3],
    }
}

/// 主音轨（track_enc = 1，也是 current_track）+ 两条洋葱皮轨道的重叠叠音场景。
///
/// 主音轨三音同色（真实场景：主轨统一蓝色）且互相重叠——修复前三者深度相同，
/// 谁在最前由可见缓冲顺序决定，描边随之逐帧闪烁；修复后由源索引稳定裁决
/// （索引小者在最前）：n1 的右边框应被 n0 填充覆盖。
fn onion_scene() -> Vec<crate::NoteInstance> {
    // border_width 高 16 位 = track_enc，低 16 位 = 边框像素宽
    let encode = |track_enc: u32, border: u32| (track_enc << 16) | border;
    vec![
        crate::NoteInstance::new(0.0, 60, 100.0, [0.2, 0.55, 1.0, 1.0], encode(1, 2)),
        crate::NoteInstance::new(0.0, 60, 40.0, [0.2, 0.55, 1.0, 1.0], encode(1, 2)),
        crate::NoteInstance::new(20.0, 60, 60.0, [0.2, 0.55, 1.0, 1.0], encode(1, 2)),
        // 洋葱皮轨道内同样存在叠音：不同调色板色，顺序敏感
        crate::NoteInstance::new(0.0, 58, 100.0, [1.0, 0.2, 0.2, 1.0], encode(2, 2)),
        crate::NoteInstance::new(10.0, 58, 40.0, [0.2, 1.0, 0.2, 1.0], encode(2, 2)),
        crate::NoteInstance::new(0.0, 56, 100.0, [0.2, 0.2, 1.0, 1.0], encode(3, 2)),
    ]
}

/// 预览音符（哨兵 border_width）：与主音轨重叠，必须恒显示在最上层。
fn preview_scene() -> Vec<crate::NoteInstance> {
    vec![crate::NoteInstance::new_preview(
        88.0,
        60,
        6.0,
        [1.0, 1.0, 1.0, 1.0],
    )]
}

/// 确定性伪随机置换：复刻 cull workgroup 抢占 `atomicAdd` 槽位的随机输出顺序。
/// 第 0 帧为原序（基线），后续每帧使用不同置换。
fn permuted_order(len: usize, frame: u32) -> Vec<u32> {
    let mut order: Vec<u32> = (0..len as u32).collect();
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ (u64::from(frame) + 1);
    for i in (1..order.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

/// 回读 RGBA8 纹理（256×144，行距 1024 天然满足 256B 对齐，无 padding）。
fn readback_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = TEST_W * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("note_depth_order_staging"),
        size: (bytes_per_row * TEST_H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_depth_order_readback"),
    });
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
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(TEST_H),
            },
        },
        wgpu::Extent3d {
            width: TEST_W,
            height: TEST_H,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Ok(result) = rx.try_recv() {
            result.expect("深度顺序测试回读 map 失败");
            let data = slice.get_mapped_range();
            let out = data.to_vec();
            drop(data);
            staging.unmap();
            return out;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "深度顺序测试回读超时（10s）"
        );
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
    }
}

/// 取像素（RGBA8）
fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * TEST_W + x) * 4) as usize;
    [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]
}

/// 测试用清屏色（与音符颜色明显不同，便于做覆盖判定）
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.05,
    g: 0.05,
    b: 0.05,
    a: 1.0,
};

fn make_color_texture(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_depth_test_color"),
        size: wgpu::Extent3d {
            width: TEST_W,
            height: TEST_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn make_depth_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_depth_test_depth"),
        size: wgpu::Extent3d {
            width: TEST_W,
            height: TEST_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(CLEAR_COLOR),
            store: wgpu::StoreOp::Store,
        },
        depth_slice: None,
    }
}

fn depth_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Store,
        }),
        stencil_ops: None,
    }
}

/// 直接写相机 uniform（绕开 `prepare_pass` 的 cull，自行控制可见顺序）
fn write_camera(renderer: &NoteRenderer, queue: &wgpu::Queue) {
    queue.write_buffer(
        renderer.viewport_buffer.inner(),
        0,
        bytemuck::cast_slice(&[test_camera()]),
    );
}

/// 直接写可见索引顺序 + 间接绘制参数（复刻 cull 的输出，用于控制/置换绘制顺序）
fn write_visible_order(renderer: &NoteRenderer, queue: &wgpu::Queue, order: &[u32]) {
    queue.write_buffer(
        renderer.visible_instance_buffer.inner(),
        0,
        bytemuck::cast_slice(order),
    );
    let args = DrawIndirectArgs {
        vertex_count: 4,
        instance_count: order.len() as u32,
        first_vertex: 0,
        first_instance: 0,
        _padding: [0; 4],
    };
    queue.write_buffer(
        renderer.indirect_buffer.inner(),
        0,
        bytemuck::bytes_of(&args),
    );
}

/// 覆盖掩码：像素是否被音符绘制（与清屏色不同）。
///
/// 背景参考取画面右下角——测试场景的音符全部落在 x < 200（tick ≤ 100）与
/// y < 50（key ≥ 56）之内，该点必然只有清屏色；同时避开 sRGB 目标格式下
/// 清屏色编码后的精确取值问题。
fn coverage_mask(pixels: &[u8]) -> Vec<bool> {
    let background = background_pixel(pixels);
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| px != &background)
        .collect()
}

/// 背景像素（画面右下角，场景内无音符覆盖）
fn background_pixel(pixels: &[u8]) -> [u8; 4] {
    pixel(pixels, TEST_W - 1, TEST_H - 1)
}

/// 核心回归测试：同一场景、同一源数据，仅改变可见索引的输出顺序，
/// 连续 64 帧像素必须逐位一致；并锁定确定性的叠压赢家与预览层级。
#[test]
fn test_overlap_pixels_are_draw_order_independent() {
    let (device, queue) = crate::pipeline::test_device();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    let onion_notes = onion_scene();
    let preview_notes = preview_scene();

    let mut onion = NoteRenderer::new_onion_skin(&device, &queue, format);
    // track_enc = 1 为主音轨（主轨蓝 + 主轨基深度）
    onion.set_view_state(&queue, 1, &[]);
    onion.upload_instances(&onion_notes, &device, &queue);

    let mut note = NoteRenderer::new(&device, &queue, format);
    note.upload_instances(&preview_notes, &device, &queue);

    write_camera(&onion, &queue);
    write_camera(&note, &queue);

    let color_texture = make_color_texture(&device, format);
    let depth_texture = make_depth_texture(&device);
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut baseline: Option<Vec<u8>> = None;
    for frame in 0..ORDER_FRAMES {
        write_visible_order(&onion, &queue, &permuted_order(onion_notes.len(), frame));
        write_visible_order(
            &note,
            &queue,
            &permuted_order(preview_notes.len(), frame.wrapping_mul(7)),
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("note_depth_order_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("note_depth_order_pass"),
                color_attachments: &[Some(color_attachment(&color_view))],
                depth_stencil_attachment: Some(depth_attachment(&depth_view)),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // 与生产同序：洋葱皮（含主音轨）先画，预览音符后画
            onion.draw(&mut pass, true, None);
            note.draw(&mut pass, true, None);
        }
        queue.submit(Some(encoder.finish()));

        let pixels = readback_pixels(&device, &queue, &color_texture);
        match baseline.as_ref() {
            None => {
                // 叠压赢家锁定：索引最小（depth 最小）的 n0 恒在最前，
                // n1 的右边框（x ∈ [76, 80)）必须被 n0 的填充覆盖
                let y = 5;
                let overlap = pixel(&pixels, 78, y);
                let plain_fill = pixel(&pixels, 194, y);
                assert_eq!(
                    overlap, plain_fill,
                    "帧 {frame}：重叠区应显示最前音符（索引最小者）的纯填充而非后画者描边"
                );
                // 预览层级：预览音符（白色 70% alpha）恒覆盖主音轨填充
                let preview_pixel = pixel(&pixels, 182, y);
                assert_ne!(
                    preview_pixel, plain_fill,
                    "帧 {frame}：预览音符未覆盖主音轨（预览层未生效）"
                );
                baseline = Some(pixels);
            }
            Some(expected) => {
                assert_eq!(
                    pixels.len(),
                    expected.len(),
                    "帧 {frame}：回读像素数量不一致"
                );
                if let Some(diff) = pixels.iter().zip(expected.iter()).position(|(a, b)| a != b) {
                    let x = (diff as u32 / 4) % TEST_W;
                    let y = (diff as u32 / 4) / TEST_W;
                    panic!(
                        "帧 {frame}：绘制顺序改变导致像素差异 @({x},{y})：{:?} vs {:?}（重叠区描边不应随 cull 输出顺序变化）",
                        pixel(&pixels, x, y),
                        pixel(expected, x, y)
                    );
                }
            }
        }
    }

    assert!(
        baseline.is_some(),
        "至少需要渲染一帧基线；ORDER_FRAMES 不得为 0"
    );
}

/// 视频导出（无 depth attachment）回归：音符不得缺失或被错误裁剪——
/// 同一场景在有 depth / 无 depth 两条管线下的覆盖掩码必须完全一致。
#[test]
fn test_depthless_pass_keeps_note_coverage() {
    let (device, queue) = crate::pipeline::test_device();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let notes = onion_scene();
    let order: Vec<u32> = (0..notes.len() as u32).collect();

    let mut with_depth = NoteRenderer::new(&device, &queue, format);
    with_depth.upload_instances(&notes, &device, &queue);
    write_camera(&with_depth, &queue);
    write_visible_order(&with_depth, &queue, &order);

    let mut no_depth = NoteRenderer::new_without_depth(&device, &queue, format);
    no_depth.upload_instances(&notes, &device, &queue);
    write_camera(&no_depth, &queue);
    write_visible_order(&no_depth, &queue, &order);

    let color_with_depth = make_color_texture(&device, format);
    let depth_texture = make_depth_texture(&device);
    let view_with_depth = color_with_depth.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let color_no_depth = make_color_texture(&device, format);
    let view_no_depth = color_no_depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_depthless_regression_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_with_depth_pass"),
            color_attachments: &[Some(color_attachment(&view_with_depth))],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        with_depth.draw(&mut pass, true, None);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_no_depth_pass"),
            color_attachments: &[Some(color_attachment(&view_no_depth))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        no_depth.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));

    let pixels_with_depth = readback_pixels(&device, &queue, &color_with_depth);
    let pixels_no_depth = readback_pixels(&device, &queue, &color_no_depth);
    let mask_with_depth = coverage_mask(&pixels_with_depth);
    let mask_no_depth = coverage_mask(&pixels_no_depth);

    let covered = mask_with_depth.iter().filter(|c| **c).count();
    assert!(covered > 0, "有 depth 参照渲染未画出任何音符");
    assert_eq!(
        mask_no_depth.iter().filter(|c| **c).count(),
        covered,
        "无 depth 路径的音符覆盖像素数与有 depth 路径不一致（音符缺失/被错误裁剪）"
    );
    if let Some(diff) = mask_with_depth
        .iter()
        .zip(mask_no_depth.iter())
        .position(|(a, b)| a != b)
    {
        let x = (diff as u32) % TEST_W;
        let y = (diff as u32) / TEST_W;
        panic!("无 depth 路径覆盖掩码与有 depth 路径不一致 @({x},{y})");
    }
}

/// 洋葱皮无 depth 变体必须与无 depth 的 RenderPass 兼容：
/// 修复前 `new_onion_skin` 硬编码 needs_depth=true，在无 depth pass 中
/// `set_pipeline` 会触发 wgpu 校验错误（整条命令缓冲被丢弃）。
#[test]
fn test_onion_skin_depthless_pipeline_is_pass_compatible() {
    let (device, queue) = crate::pipeline::test_device();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let notes = onion_scene();

    let mut onion = NoteRenderer::new_onion_skin_without_depth(&device, &queue, format);
    onion.set_view_state(&queue, 1, &[]);
    onion.upload_instances(&notes, &device, &queue);
    write_camera(&onion, &queue);
    write_visible_order(&onion, &queue, &(0..notes.len() as u32).collect::<Vec<_>>());
    assert!(
        onion.last_upload_count() > 0,
        "洋葱皮渲染器必须实际上传实例，否则 draw 提前返回、校验错误不会暴露"
    );

    let color_texture = make_color_texture(&device, format);
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_onion_depthless_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_onion_depthless_pass"),
            color_attachments: &[Some(color_attachment(&color_view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        onion.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    let error = futures::executor::block_on(device.pop_error_scope());
    assert!(
        error.is_none(),
        "无 depth RenderPass 中绘制洋葱皮触发了 wgpu 校验错误：{error:?}"
    );

    // 静音轨（非主音轨）必须完全不产生片元：覆盖掩码为空
    let mut muted_onion = NoteRenderer::new_onion_skin_without_depth(&device, &queue, format);
    // current_track = 1（主音轨 = track_enc 1）；轨道索引 1/2 静音
    // → track_enc 2（key 58）/ track_enc 3（key 56）必须被裁剪，仅主音轨可见
    muted_onion.set_view_state(&queue, 1, &[1, 2]);
    muted_onion.upload_instances(&notes, &device, &queue);
    write_camera(&muted_onion, &queue);
    write_visible_order(
        &muted_onion,
        &queue,
        &(0..notes.len() as u32).collect::<Vec<_>>(),
    );
    let muted_texture = make_color_texture(&device, format);
    let muted_view = muted_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_onion_muted_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_onion_muted_pass"),
            color_attachments: &[Some(color_attachment(&muted_view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        muted_onion.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    let muted_pixels = readback_pixels(&device, &queue, &muted_texture);
    let clear = background_pixel(&muted_pixels);
    // 洋葱皮轨道（key 58 → y ∈ [20, 30)、key 56 → y ∈ [40, 50)）在无 depth 路径下
    // 也必须被静音裁剪掉，不得因「z=2.0 无 depth attachment 不裁剪」而误绘
    for y in 20..50u32 {
        for x in 0..TEST_W {
            assert_eq!(
                pixel(&muted_pixels, x, y),
                clear,
                "静音轨在无 depth 路径下仍被绘制 @({x},{y})"
            );
        }
    }
}
