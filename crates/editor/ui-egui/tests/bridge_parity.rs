//! 桥接对齐测试：`lumino-egui-bridge`（wgpu 27）应重现
//! `egui_kittest`（egui-wgpu 0.37 / wgpu 30）生成的主窗口基线。
//!
//! 同一份 `SceneState` + 同一份 tessellation，差异只可能来自绘制后端；
//! 因此这里用主窗口场景做整帧对比，作为 Task 1.1 的验收证据。

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use lumino_egui_bridge::{Painter, Screen};
use lumino_ui_egui::scene::{self, SceneId, SceneState};

/// 通道容差（与 `compare_egui` 一致）。
const TOLERANCE: u8 = 2;
/// 允许超容差像素占比：桥接与 egui-wgpu 的着色/采样逐行对齐，实测为 0。
const MAX_OVER_RATIO: f64 = 0.0;

fn gpu() -> &'static (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    static GPU: OnceLock<(wgpu::Instance, wgpu::Device, wgpu::Queue)> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("桥接对齐测试需要可用的 wgpu 适配器");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("egui_bridge_parity_device"),
            required_features: adapter.features() & wgpu::Features::default(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .expect("请求桥接对齐测试设备失败");
        (instance, device, queue)
    })
}

fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let unpadded = width * 4;
    let bytes_per_row = unpadded.div_ceil(256) * 256;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("egui_bridge_parity_staging"),
        size: u64::from(bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui_bridge_parity_readback"),
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
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(result) = rx.try_recv() {
            result.expect("桥接对齐测试回读 map 失败");
            let data = slice.get_mapped_range();
            let mut packed = Vec::with_capacity((unpadded * height) as usize);
            for row in 0..height {
                let start = (row * bytes_per_row) as usize;
                packed.extend_from_slice(&data[start..start + unpadded as usize]);
            }
            drop(data);
            staging.unmap();
            return packed;
        }
        assert!(
            Instant::now() < deadline,
            "桥接对齐测试回读超时（30s，GPU 可能被驱动重置）"
        );
        let _ = device.poll(wgpu::PollType::Poll);
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// 用桥接渲染一个场景，返回 RGBA8。
fn render_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene_id: SceneId,
    pixels_per_point: f32,
) -> (Vec<u8>, u32, u32) {
    let (logical_width, logical_height) = scene_id.logical_size();
    let width = (logical_width as f32 * pixels_per_point).round() as u32;
    let height = (logical_height as f32 * pixels_per_point).round() as u32;

    let mut state = SceneState::new(scene_id);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(pixels_per_point);
    lumino_ui_egui::theme::apply(&ctx);
    lumino_ui_egui::fonts::install(&ctx);

    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(logical_width as f32, logical_height as f32),
        )),
        ..Default::default()
    };

    // 与 `egui_kittest` 无表面路径一致：目标是 gamma 格式（`Rgba8Unorm`），
    // 着色器走 `fs_main_gamma`，快照字节可直接比较。
    let mut painter = Painter::new(device, wgpu::TextureFormat::Rgba8Unorm);
    let mut primitives = Vec::new();
    for _ in 0..8 {
        let output = ctx.run_ui(input.clone(), |ui| state.show(ui));
        let repaint_delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::ZERO, |viewport| viewport.repaint_delay);
        let mut textures = output.textures_delta;
        painter.update_textures(device, queue, &mut textures);
        primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        if repaint_delay != Duration::ZERO {
            break;
        }
    }

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("egui_bridge_parity_target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui_bridge_parity_encoder"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui_bridge_parity_clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    let _ = painter.render(
        device,
        queue,
        &mut encoder,
        &view,
        Screen {
            size_px: [width, height],
            pixels_per_point,
            clear: None,
        },
        &primitives,
    );
    queue.submit(Some(encoder.finish()));

    (
        readback(device, queue, &target, width, height),
        width,
        height,
    )
}

fn baseline_path(scene_id: SceneId, scale: f32) -> PathBuf {
    scene::baseline_dir().join(scene::baseline_file_name(scene_id, scale))
}

#[test]
fn bridge_matches_egui_baselines() {
    let (_, device, queue) = gpu();
    let mut failures = Vec::new();

    for scene_id in SceneId::ALL {
        let (rgba, width, height) = render_scene(device, queue, scene_id, 1.0);
        let path = baseline_path(scene_id, 1.0);
        let baseline = image::open(&path)
            .expect("读取 egui 基线失败（先运行 capture_egui）")
            .to_rgba8();
        assert_eq!(
            (baseline.width(), baseline.height()),
            (width, height),
            "{} 基线尺寸与桥接目标不一致",
            scene_id.slug()
        );

        let baseline = baseline.as_raw();
        let debug_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("测试目录必须有工作区根")
            .join("target")
            .join(format!("bridge-{}-dpi100.png", scene_id.slug()));
        image::save_buffer(&debug_path, &rgba, width, height, image::ColorType::Rgba8)
            .expect("写入桥接调试图失败");

        let mut over = 0_u64;
        let mut max_delta = 0_u8;
        let mut sum = 0_u64;
        let (want_pixels, _) = baseline.as_chunks::<4>();
        let (got_pixels, _) = rgba.as_chunks::<4>();
        for (want, got) in want_pixels.iter().zip(got_pixels) {
            let delta = (0..3)
                .map(|channel| want[channel].abs_diff(got[channel]))
                .max()
                .unwrap_or_default();
            max_delta = max_delta.max(delta);
            sum += u64::from(delta);
            if delta > TOLERANCE {
                over += 1;
            }
        }

        let total = u64::from(width) * u64::from(height);
        let ratio = over as f64 / total as f64;
        let mean = sum as f64 / total as f64;
        println!(
            "{:<20} {width}x{height} over_tol={over} ({:.4}%) max={max_delta} mean={mean:.3}",
            scene_id.slug(),
            ratio * 100.0
        );
        if ratio > MAX_OVER_RATIO {
            failures.push(format!(
                "{}: 超容差 {over}/{total}（{:.4}%），最大通道差 {max_delta}，平均 {mean:.3}",
                scene_id.slug(),
                ratio * 100.0
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "桥接输出与 egui 基线差异过大：\n{}",
        failures.join("\n")
    );
}
