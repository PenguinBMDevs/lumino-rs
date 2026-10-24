//! 像素聚合 splat 的正确性验证：与点图元路径逐像素一致。
//!
//! 锁定契约：
//! 1. 每个亚像素音符仍至少落 1 像素（与 point 路径同覆盖）；
//! 2. 同像素重叠时「源索引小/主音轨优先」的胜者颜色正确；
//! 3. 静音轨不出现；主音轨即使静音也显示；
//! 4. 空格保留背景（discard，不写颜色/深度）。

mod precision;

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

const RED: [f32; 4] = [1.0, 0.2, 0.2, 1.0];
const GREEN: [f32; 4] = [0.2, 1.0, 0.2, 1.0];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.2, 1.0];

fn camera() -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [1.0, 1.0],
        viewport_size: [W as f32, H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [W as f32, H as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: 127.0,
        _padding: [0.0; 3],
    }
}

fn encode(track_enc: u32) -> u32 {
    track_enc << 16
}

fn scene() -> Vec<NoteInstance> {
    vec![
        // 洋葱皮红：像素 (100, 63)
        NoteInstance::new(100.0, 64, 1.0, RED, encode(1)),
        // 同像素后来的洋葱皮绿：必须输给更小源索引的红
        NoteInstance::new(100.0, 64, 1.0, GREEN, encode(1)),
        // 静音轨（track_idx=2）：不得出现
        NoteInstance::new(150.0, 64, 1.0, YELLOW, encode(3)),
        // 主音轨（current_track=2）：主轨蓝，即使被静音也显示
        NoteInstance::new(200.0, 64, 1.0, RED, encode(2)),
        // 独立像素 (250, row(70)=57)
        NoteInstance::new(250.0, 70, 1.0, RED, encode(1)),
    ]
}

fn make_textures_at(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("splat_test_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("splat_test_depth"),
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
    (color, color_view, depth_view)
}

fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    color: &'a wgpu::TextureView,
    depth: &'a wgpu::TextureView,
    label: &str,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(CLEAR),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
    })
}

fn read_pixels_at(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let bytes_per_row = width * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("splat_test_staging"),
        size: (bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("splat_test_readback"),
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
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(e)) => panic!("splat 回读失败: {e:?}"),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "splat 回读 30s 未就绪"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!("splat 回读回调丢失"),
        }
    }
    let data = slice.get_mapped_range().to_vec();
    staging.unmap();
    data
}

fn build_renderer_with(notes: &[NoteInstance], camera: CameraUniform) -> NoteRenderer {
    let (device, queue) = crate::test_gpu::shared_device();
    let mut renderer =
        NoteRenderer::new_onion_skin(&device, &queue, FORMAT, crate::GpuEpoch::INITIAL);
    renderer.set_view_state(&queue, 2, &[2]);
    renderer.upload_instances(notes, &device, &queue);
    renderer.prepare_direct(camera, &queue);
    renderer
}

fn render_points_with(
    notes: &[NoteInstance],
    camera: CameraUniform,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let (device, queue) = crate::test_gpu::shared_device();
    let renderer = build_renderer_with(notes, camera);
    let (color, color_view, depth_view) = make_textures_at(&device, width, height);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("splat_test_point"),
    });
    {
        let mut pass = begin_pass(&mut encoder, &color_view, &depth_view, "point_pass");
        renderer.draw_points(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    read_pixels_at(&device, &queue, &color, width, height)
}

fn render_points(notes: &[NoteInstance]) -> Vec<u8> {
    render_points_with(notes, camera(), W, H)
}

fn render_splat_with(
    notes: &[NoteInstance],
    camera: CameraUniform,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let (device, queue) = crate::test_gpu::shared_device();
    let mut renderer = build_renderer_with(notes, camera);
    let (color, color_view, depth_view) = make_textures_at(&device, width, height);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("splat_test_splat"),
    });
    assert!(
        renderer.prepare_splat_for_test(&mut encoder, &device, &queue, camera, (width, height)),
        "splat 准备必须成功（小场景强制绕过实例数阈值）"
    );
    {
        let mut pass = begin_pass(&mut encoder, &color_view, &depth_view, "splat_pass");
        assert!(
            renderer.draw_splat(&mut pass, None),
            "splat 绑定缺失时不得静默跳过"
        );
    }
    queue.submit(Some(encoder.finish()));
    read_pixels_at(&device, &queue, &color, width, height)
}

fn render_splat(notes: &[NoteInstance]) -> Vec<u8> {
    render_splat_with(notes, camera(), W, H)
}

fn pixel_at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * width + x) * 4) as usize;
    [
        pixels[idx],
        pixels[idx + 1],
        pixels[idx + 2],
        pixels[idx + 3],
    ]
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    pixel_at(pixels, W, x, y)
}

fn expected(color: [f32; 4]) -> [u8; 4] {
    [
        (color[0] * 255.0).round() as u8,
        (color[1] * 255.0).round() as u8,
        (color[2] * 255.0).round() as u8,
        255,
    ]
}

const MAIN_BLUE: [f32; 4] = [0.2, 0.55, 1.0, 1.0];

#[test]
fn test_splat_matches_point_path_pixels() {
    let notes = scene();
    let point = render_points(&notes);
    let splat = render_splat(&notes);
    assert_eq!(
        point, splat,
        "splat 必须与点图元路径逐像素一致（同覆盖、同胜者颜色、同背景）"
    );
}

#[test]
fn test_splat_dense_scene_matches_point_path() {
    // 覆盖不同的中心取整/重叠/跨轨道：仍必须逐像素一致
    let mut notes = Vec::with_capacity(4096);
    for i in 0..4096usize {
        let tick = ((i * 37) % 320) as f32;
        let key = ((i * 53) % 128) as u8;
        let length = 1.0 + ((i * 7) % 30) as f32;
        let track = if i % 5 == 0 {
            2
        } else if i % 7 == 0 {
            3
        } else {
            1
        };
        let color = if i % 3 == 0 { GREEN } else { RED };
        notes.push(NoteInstance::new(tick, key, length, color, encode(track)));
    }
    let point = render_points(&notes);
    let splat = render_splat(&notes);
    let mut diffs = 0usize;
    for y in 0..H {
        for x in 0..W {
            let p = pixel(&point, x, y);
            let s = pixel(&splat, x, y);
            if p != s {
                diffs += 1;
                if diffs <= 12 {
                    eprintln!("diff ({x},{y}) point={p:?} splat={s:?}");
                }
            }
        }
    }
    assert_eq!(
        diffs, 0,
        "4096 音符密集场景：splat 与点图元路径必须逐像素一致（差异 {diffs} 像素）"
    );
}

#[test]
fn test_splat_winner_and_mute_contract() {
    let notes = scene();
    let pixels = render_splat(&notes);
    let clear = expected([0.02, 0.02, 0.02, 1.0]);

    assert_eq!(
        pixel(&pixels, 100, 63),
        expected(crate::FALLBACK_TRACK_COLORS[0]),
        "同像素重叠必须由更小源索引的红色胜出"
    );
    assert_eq!(
        pixel(&pixels, 150, 63),
        clear,
        "静音轨音符不得出现（保留背景）"
    );
    assert_eq!(
        pixel(&pixels, 200, 63),
        expected(MAIN_BLUE),
        "主音轨固定蓝、且主轨即使静音也显示"
    );
    assert_eq!(
        pixel(&pixels, 250, 57),
        expected(crate::FALLBACK_TRACK_COLORS[0]),
        "独立音符必须落点"
    );
    assert_eq!(
        pixel(&pixels, 0, 0),
        clear,
        "空格必须保留背景（discard 不写颜色）"
    );
}
