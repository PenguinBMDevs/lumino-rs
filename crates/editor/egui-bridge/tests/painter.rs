//! wgpu 27 桥接的 GPU 集成测试：真实设备 + 离屏 sRGB 目标 + 回读。

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use egui::epaint::{ImageDelta, Primitive};
use egui::{ClippedPrimitive, Color32, ColorImage, Mesh, Pos2, Rect, TextureId, TextureOptions};
use lumino_egui_bridge::{Painter, Screen};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const BG: Color32 = Color32::from_rgb(0x24, 0x28, 0x3B);

/// 进程内共享实例 + 设备 + 队列。
fn gpu() -> &'static (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    static GPU: OnceLock<(wgpu::Instance, wgpu::Device, wgpu::Queue)> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("桥接测试需要可用的 wgpu 适配器");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("egui_bridge_test_device"),
            required_features: adapter.features() & wgpu::Features::default(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .expect("请求桥接测试设备失败");
        (instance, device, queue)
    })
}

fn create_target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("egui_bridge_test_target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, color: wgpu::Color) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("egui_bridge_test_clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
}

fn readback(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<Color32> {
    // WIDTH * 4 = 256，天然满足 bytes_per_row 的 256 对齐。
    let bytes_per_row = WIDTH * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("egui_bridge_test_staging"),
        size: (bytes_per_row * HEIGHT) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui_bridge_test_readback"),
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
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(result) = rx.try_recv() {
            result.expect("桥接测试回读 map 失败");
            let data = slice.get_mapped_range();
            let (chunks, _) = data.as_chunks::<4>();
            let pixels = chunks
                .iter()
                .map(|pixel| {
                    Color32::from_rgba_unmultiplied(pixel[0], pixel[1], pixel[2], pixel[3])
                })
                .collect();
            drop(data);
            staging.unmap();
            return pixels;
        }
        assert!(
            Instant::now() < deadline,
            "桥接测试回读超时（30s，GPU 可能被驱动重置）"
        );
        let _ = device.poll(wgpu::PollType::Poll);
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// 纯色四边形（使用 1×1 白色纹理 + 顶点色）。
fn colored_quad(clip_rect: Rect, color: Color32) -> ClippedPrimitive {
    let mut mesh = Mesh {
        texture_id: TextureId::Managed(0),
        ..Default::default()
    };
    let corners = [
        Pos2::new(0.0, 0.0),
        Pos2::new(WIDTH as f32, 0.0),
        Pos2::new(WIDTH as f32, HEIGHT as f32),
        Pos2::new(0.0, HEIGHT as f32),
    ];
    for corner in corners {
        mesh.colored_vertex(corner, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ClippedPrimitive {
        clip_rect,
        primitive: Primitive::Mesh(mesh),
    }
}

/// 1×1 白色纹理增量（供纯色四边形采样）。
fn white_texture_delta() -> egui::TexturesDelta {
    let mut delta = egui::TexturesDelta::default();
    delta.push(
        TextureId::Managed(0),
        ImageDelta::full(
            ColorImage::new([1, 1], vec![Color32::WHITE]),
            TextureOptions::LINEAR,
        ),
    );
    delta
}

/// 渲染给定图元并回读。
fn render(primitives: &[ClippedPrimitive]) -> Vec<Color32> {
    let (_, device, queue) = gpu();
    let mut painter = Painter::new(device, FORMAT);
    let mut textures = white_texture_delta();
    painter.update_textures(device, queue, &mut textures);

    let target = create_target(device);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui_bridge_test_encoder"),
    });
    clear(&mut encoder, &view, wgpu::Color::BLACK);
    let drawn = painter.render(
        device,
        queue,
        &mut encoder,
        &view,
        Screen {
            size_px: [WIDTH, HEIGHT],
            pixels_per_point: 1.0,
        },
        primitives,
    );
    assert_eq!(drawn, primitives.len(), "所有图元都应被提交绘制");
    queue.submit(Some(encoder.finish()));
    readback(device, queue, &target)
}

fn pixel(pixels: &[Color32], x: u32, y: u32) -> Color32 {
    pixels[(y * WIDTH + x) as usize]
}

fn assert_channel_close(actual: Color32, expected: Color32, tolerance: u8, label: &str) {
    let delta = [
        actual.r().abs_diff(expected.r()),
        actual.g().abs_diff(expected.g()),
        actual.b().abs_diff(expected.b()),
    ];
    assert!(
        delta.iter().all(|value| *value <= tolerance),
        "{label}: 颜色 {:?} 与期望 {:?} 的通道差 {delta:?} 超过 {tolerance}",
        actual.to_array(),
        expected.to_array()
    );
}

#[test]
fn solid_quad_roundtrips_srgb_color() {
    let primitives = [colored_quad(
        Rect::from_min_size(Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32)),
        BG,
    )];
    let pixels = render(&primitives);
    assert_channel_close(pixel(&pixels, 32, 32), BG, 1, "中心像素");
    assert_channel_close(pixel(&pixels, 0, 0), BG, 1, "左上角");
    assert_channel_close(pixel(&pixels, WIDTH - 1, HEIGHT - 1), BG, 1, "右下角");
}

#[test]
fn scissor_clips_to_left_half() {
    let primitives = [colored_quad(
        Rect::from_min_max(Pos2::ZERO, Pos2::new(WIDTH as f32 / 2.0, HEIGHT as f32)),
        BG,
    )];
    let pixels = render(&primitives);
    assert_channel_close(pixel(&pixels, 16, 32), BG, 1, "裁剪区内");
    assert_channel_close(
        pixel(&pixels, 48, 32),
        Color32::BLACK,
        1,
        "裁剪区外（保留清屏色）",
    );
}

#[test]
fn egui_text_upload_uses_font_atlas() {
    let (_, device, queue) = gpu();
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(WIDTH as f32, HEIGHT as f32),
        )),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| {
        let rect = ui.ctx().viewport_rect();
        ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, BG);
        ui.painter().text(
            Pos2::new(4.0, 8.0),
            egui::Align2::LEFT_TOP,
            "Lumino",
            egui::FontId::proportional(20.0),
            Color32::WHITE,
        );
    });
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    assert!(
        !output.textures_delta.set.is_empty(),
        "首帧必须包含字体图集上传"
    );

    let mut painter = Painter::new(device, FORMAT);
    let mut textures = output.textures_delta;
    painter.update_textures(device, queue, &mut textures);

    let target = create_target(device);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui_bridge_text_encoder"),
    });
    clear(&mut encoder, &view, wgpu::Color::BLACK);
    painter.render(
        device,
        queue,
        &mut encoder,
        &view,
        Screen {
            size_px: [WIDTH, HEIGHT],
            pixels_per_point: output.pixels_per_point,
        },
        &primitives,
    );
    queue.submit(Some(encoder.finish()));

    let pixels = readback(device, queue, &target);
    assert_channel_close(pixel(&pixels, 1, 1), BG, 1, "面板背景");
    let bright = pixels
        .iter()
        .filter(|pixel| pixel.r() > 200 && pixel.g() > 200 && pixel.b() > 200)
        .count();
    assert!(bright > 10, "文字未渲染：亮色像素只有 {bright} 个");
}
