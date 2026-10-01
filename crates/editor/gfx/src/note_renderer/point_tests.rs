//! 亚像素档位点图元直绘的正确性验证（PREF-004 P1）
//!
//! 点路径（`onion_note.wgsl` 的 `vs_point` / `fs_point`）绕开了 cull 的可见
//! 索引列表，可见性判定移入顶点着色器。本模块锁定三条不可退让的契约：
//!
//! 1. **1 像素以下音符仍必须被绘制**（`cull.wgsl` 里的用户硬约束：不得做 LOD
//!    剔除）——点路径下每个音符必须至少落 1 个像素；
//! 2. **静音轨不得出现**——`vs_point` 的静音判定必须与 `vs_main` 同口径；
//! 3. **长度 ≤ 0 的音符不得出现**——与 `cull.wgsl` 的 `length > 0.0` 同口径；
//! 4. **点管线缺失时必须回退 quad 路径**，不得静默不画（预览层与导出无 depth
//!    变体都没有点管线）。
//!
//! 深度契约由 `depth_tests.rs::test_shader_sources_share_depth_contract` 覆盖
//! （`vs_point` 与 `vs_main` 共用同一个 `region_depth` 与同一 `global_index` 语义）。

use crate::NoteInstance;
use crate::note_renderer::NoteRenderer;
use crate::note_renderer::types::CameraUniform;

const W: u32 = 320;
const H: u32 = 240;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.02,
    g: 0.02,
    b: 0.02,
    a: 1.0,
};
/// 音符颜色（与清屏色差异极大，便于覆盖判定）
const NOTE_COLOR: [f32; 4] = [1.0, 0.2, 0.2, 1.0];
/// 全曲 tick 跨度（配合亚像素缩放：120 tick 音符在 320px 视口下 < 0.1px）
const TOTAL_TICKS: f32 = 1_000_000.0;

/// 亚像素相机：全曲 tick + 全 128 键可见 ⇒ 任意音符 quad 宽度 << 1px。
fn subpixel_camera() -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [(W as f32) / TOTAL_TICKS, (H as f32) / 128.0],
        viewport_size: [W as f32, H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [W as f32, H as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: 127.0,
        _padding: [0.0; 3],
    }
}

/// 常规相机（quad 清晰可见），用于回退路径验证。
fn normal_camera() -> CameraUniform {
    CameraUniform {
        zoom: [2.0, 10.0],
        viewport_size: [W as f32, H as f32],
        canvas_size: [W as f32, H as f32],
        max_key_index: 127.0,
        ..subpixel_camera()
    }
}

/// 轨道编码（高 16 位）+ 描边像素宽（低 16 位）
fn encode(track_enc: u32, border: u32) -> u32 {
    (track_enc << 16) | border
}

fn make_textures(
    device: &wgpu::Device,
    label: &str,
) -> (
    wgpu::Texture,
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::TextureView,
) {
    let size = wgpu::Extent3d {
        width: W,
        height: H,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("point_test_depth"),
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
    (color, depth, color_view, depth_view)
}

/// 颜色 attachment（清屏色固定；depth attachment 用 `depth_attachment`，由调用点内联，
/// 因为 `RenderPassDescriptor` 的附件切片必须比调用存活）。
fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(CLEAR),
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

/// 用点图元路径渲染一帧并回读像素。
fn render_points(notes: &[NoteInstance], current_track: u32, muted: &[usize]) -> Vec<u8> {
    let (device, queue) = crate::test_gpu::shared_device();

    let mut renderer = NoteRenderer::new_onion_skin(&device, &queue, FORMAT);
    renderer.set_view_state(&queue, current_track, muted);
    renderer.upload_instances(notes, &device, &queue);
    renderer.prepare_direct(subpixel_camera(), &queue);

    let (color, _depth, color_view, depth_view) = make_textures(&device, "point_test_color");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("point_test_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("point_test_pass"),
            color_attachments: &[Some(color_attachment(&color_view))],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw_points(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    read_pixels(&device, &queue, &color)
}

/// 回读整张 RGBA8 纹理（非阻塞轮询 + 有界等待，禁止无限 poll）。
fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = W * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("point_test_staging"),
        size: (bytes_per_row * H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("point_test_readback"),
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
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(e)) => panic!("回读 map 失败: {e:?}"),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(std::time::Instant::now() < deadline, "回读 30s 未就绪");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!("回读回调丢失"),
        }
    }
    let data = slice.get_mapped_range().to_vec();
    staging.unmap();
    data
}

/// 被音符覆盖（非清屏色）的像素数
fn covered_pixels(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
        .count()
}

/// 被覆盖的像素行集合
fn covered_rows(pixels: &[u8]) -> Vec<u32> {
    let mut rows = Vec::new();
    for y in 0..H {
        let row = &pixels[(y * W * 4) as usize..((y + 1) * W * 4) as usize];
        if row.as_chunks::<4>().0.iter().any(|px| px[0] > 8) {
            rows.push(y);
        }
    }
    rows
}

/// 指定 key 的**期望**像素行（与 `subpixel_camera` + `vs_point` 的映射一致）
fn expected_row(key: u8) -> u32 {
    let zoom_y = (H as f32) / 128.0;
    let center_y = (127.0 - f32::from(key)) * zoom_y + zoom_y * 0.5;
    center_y.round() as u32
}

/// 该 key 相邻 ±1 行内是否有覆盖（容忍栅格化像素中心取整的 off-by-one）
fn key_row_covered(rows: &[u32], key: u8) -> bool {
    let center = expected_row(key);
    rows.iter().any(|r| r.abs_diff(center) <= 1)
}

/// **契约 1 + 3**：亚像素档位下每个合法音符至少落 1 像素；`length = 0` 不绘制。
#[test]
fn test_point_mode_paints_every_subpixel_note() {
    let mut notes = Vec::new();
    for i in 0..64u32 {
        let key = 60 + i as u8; // key 60..123，相邻 key 相差 1.875px ⇒ 独立像素行
        let tick = TOTAL_TICKS * (i as f32) / 64.0;
        notes.push(NoteInstance::new(
            tick,
            key,
            120.0,
            NOTE_COLOR,
            encode(1, 2),
        ));
    }
    // 非法音符：length = 0（cull.wgsl 与 vs_point 都必须剔除）
    notes.push(NoteInstance::new(
        TOTAL_TICKS * 0.5,
        10,
        0.0,
        NOTE_COLOR,
        encode(1, 2),
    ));

    let pixels = render_points(&notes, 1, &[]);
    let rows = covered_rows(&pixels);
    let covered = covered_pixels(&pixels);

    assert!(
        covered >= 64,
        "亚像素档位下每个音符仍必须至少落 1 像素（用户硬约束），实际覆盖 {covered} 像素"
    );
    assert!(
        covered <= 64 * 2,
        "点图元应每实例约 1 像素，实际 {covered} 像素（疑似重复绘制）"
    );
    // 64 个合法音符各占一行
    let painted = (60..124u8)
        .filter(|&key| key_row_covered(&rows, key))
        .count();
    assert_eq!(
        painted, 64,
        "64 个亚像素音符必须全部落点（实际 {painted} 个音符有像素）"
    );
    assert!(
        !key_row_covered(&rows, 10),
        "length = 0 的音符不得绘制（key 10 → 期望行 {}）",
        expected_row(10)
    );
}

/// **契约 2**：静音轨不得出现。
#[test]
fn test_point_mode_respects_muted_tracks() {
    let notes = vec![
        // 主音轨（track_enc 1 = current_track）
        NoteInstance::new(0.0, 90, 120.0, NOTE_COLOR, encode(1, 2)),
        // 静音轨（track_enc 2；track_idx 1 被置为静音）
        NoteInstance::new(0.0, 70, 120.0, NOTE_COLOR, encode(2, 2)),
    ];

    let pixels = render_points(&notes, 1, &[1]);
    let rows = covered_rows(&pixels);

    assert!(
        key_row_covered(&rows, 90),
        "主音轨音符必须绘制（key 90 → 期望行 {}）",
        expected_row(90)
    );
    assert!(
        !key_row_covered(&rows, 70),
        "静音轨音符不得绘制（key 70 → 期望行 {}）",
        expected_row(70)
    );
}

/// **契约 4**：点管线缺失时回退 quad 路径，不得静默不画。
///
/// 预览渲染器（`note.wgsl`）与导出无 depth 变体都不建点管线。
#[test]
fn test_draw_points_falls_back_without_point_pipeline() {
    let (device, queue) = crate::test_gpu::shared_device();
    let mut renderer = NoteRenderer::new(&device, &queue, FORMAT);
    // key 110：在 `normal_camera`（zoom_y = 10，高 240px ⇒ 可见 24 键）视口内。
    renderer.upload_instances(
        &[NoteInstance::new(0.0, 110, 40.0, NOTE_COLOR, encode(1, 2))],
        &device,
        &queue,
    );

    let camera = normal_camera();
    let (color, _depth, color_view, depth_view) = make_textures(&device, "point_fallback_color");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("point_fallback_encoder"),
    });
    // 预览渲染器没有 cull uniform 之外的可见列表来源：先跑标准 cull 填入
    // 可见索引 + indirect 参数，再走 draw_points（内部应回退 quad 路径）。
    renderer.prepare_pass(&mut encoder, camera, &queue);
    // cull 的产出需要一次提交才对后续 draw 可见（同一 encoder 内也可，但
    // 这里分成两次提交以复刻生产：prepare 与 draw 同 encoder，见 render_pass.rs）
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("point_fallback_pass"),
            color_attachments: &[Some(color_attachment(&color_view))],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw_points(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));

    // draw_points 回退路径读 indirect 参数（由同一 encoder 内的 cull 填充），
    // GPU 按序执行 ⇒ 回退分支必须真的画出音符。
    let pixels = read_pixels(&device, &queue, &color);
    assert!(
        covered_pixels(&pixels) > 0,
        "无点管线时 draw_points 必须回退 quad 路径绘制，而不是静默不画"
    );
}
