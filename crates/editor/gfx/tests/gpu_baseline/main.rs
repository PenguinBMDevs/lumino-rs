//! GPU 渲染基线：保护 wgpu30 迁移前后「画面不变」。
//!
//! 覆盖范围（Phase 0.6 v1）：钢琴卷帘的三条主渲染路径——
//! - `note_renderer`：音符实例（洋葱皮主路径，`NoteRenderer` + indirect draw）；
//! - `grid_renderer`：无限网格（背景 / 黑键区 / 小节 / 拍 / 细分线）；
//! - `ruler_renderer`：时间标尺（小节 / 拍刻度）。
//!
//! 其余渲染器（走带、CC 力度、MIDI 控制台、MIDITrail、瀑布流、视频导出）
//! 需要产品级场景夹具，留待 Phase 1.2 迁移前补齐；此文件是它们的接入点。
//!
//! 基线策略：
//! - 首次运行写入 `tests/gpu-baseline/{scene}.png`；
//! - 之后逐像素比较，允许每通道 ±1（硬件后端舍入）；
//! - 超过容差时输出差异统计并失败。

#[path = "../cpu_gpu_parity/harness.rs"]
#[allow(dead_code)]
mod harness;
#[path = "../cpu_gpu_parity/scenes.rs"]
#[allow(dead_code)]
mod scenes;

use std::path::PathBuf;

use lumino_gfx::{
    GridPrepareParams, GridRenderer, RulerPrepareParams, RulerRenderer, generate_ruler_instances,
};

const W: u32 = harness::W;
const H: u32 = harness::H;
const FORMAT: wgpu::TextureFormat = harness::FORMAT;

/// 基线目录（workspace 根 / tests / gpu-baseline）。
fn baseline_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join("tests")
        .join("gpu-baseline")
}

/// 断言当前帧与基线一致；基线不存在时创建。
fn assert_baseline(scene: &str, rgba: &[u8]) {
    let dir = baseline_dir();
    std::fs::create_dir_all(&dir).expect("创建 GPU 基线目录失败");
    let path = dir.join(format!("{scene}.png"));

    if !path.exists() {
        write_png(&path, rgba);
        eprintln!(
            "[gpu-baseline] 已创建基线 {}（请提交该文件）",
            path.display()
        );
        return;
    }

    let existing = read_png(&path);
    assert_eq!(
        existing.len(),
        rgba.len(),
        "{scene}: 基线尺寸不一致（{} vs {} 字节）",
        existing.len(),
        rgba.len()
    );

    let mut over = 0_u64;
    let mut max_delta = 0_u8;
    let mut first: Option<(usize, [u8; 4], [u8; 4])> = None;
    let (want_px, _) = existing.as_chunks::<4>();
    let (got_px, _) = rgba.as_chunks::<4>();
    for (index, (a, b)) in want_px.iter().zip(got_px).enumerate() {
        let delta = (0..3)
            .map(|c| a[c].abs_diff(b[c]))
            .max()
            .unwrap_or_default();
        if delta > 1 {
            over += 1;
            if first.is_none() {
                first = Some((index, [a[0], a[1], a[2], a[3]], [b[0], b[1], b[2], b[3]]));
            }
        }
        max_delta = max_delta.max(delta);
    }

    if over > 0 {
        let (index, want, got) = first.expect("over > 0 时必然存在首个差异像素");
        panic!(
            "{scene}: 与基线不一致，超容差像素 {over}/{}，最大通道差 {max_delta}；\
             首个差异像素 #{index} 基线={want:?} 当前={got:?}（基线：{}）",
            (W * H) as u64,
            path.display()
        );
    }
}

fn write_png(path: &std::path::Path, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("创建基线 PNG 失败");
    let mut encoder = png::Encoder::new(file, W, H);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("写入 PNG 头失败");
    writer.write_image_data(rgba).expect("写入 PNG 数据失败");
}

fn read_png(path: &std::path::Path) -> Vec<u8> {
    let file = std::fs::File::open(path).expect("打开基线 PNG 失败");
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("读取 PNG 头失败");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("读取 PNG 数据失败");
    assert_eq!(info.color_type, png::ColorType::Rgba, "基线必须是 RGBA8");
    buffer.truncate(info.buffer_size());
    buffer
}

/// 深色主题背景（东京夜风暴 `background.base`）。
const BG: [f32; 4] = [0.016, 0.020, 0.047, 1.0];

/// 离屏颜色目标 + 命令编码器（调用方自行录制 draw，然后 `finish_offscreen`）。
struct Offscreen {
    device: &'static wgpu::Device,
    queue: &'static wgpu::Queue,
    view: wgpu::TextureView,
    color: wgpu::Texture,
    encoder: wgpu::CommandEncoder,
}

fn begin_offscreen() -> Offscreen {
    let (_, device, queue) = harness::shared();
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu_baseline_color"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gpu_baseline_encoder"),
    });
    Offscreen {
        device,
        queue,
        view,
        color,
        encoder,
    }
}

impl Offscreen {
    /// 录制一次绘制（pass 由调用方用 [`begin_pass`] 创建，保证生命周期可推断）。
    fn record(&mut self, draw: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView)) {
        draw(&mut self.encoder, &self.view);
        let encoder = std::mem::replace(
            &mut self.encoder,
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu_baseline_finished"),
                }),
        );
        self.queue.submit(Some(encoder.finish()));
    }

    fn readback(self) -> Vec<u8> {
        harness::readback(self.device, self.queue, &self.color)
    }
}

/// 创建清屏后的 render pass（清屏色 = 主题背景）。
fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("gpu_baseline_pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(BG[0]),
                    g: f64::from(BG[1]),
                    b: f64::from(BG[2]),
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    })
}

#[test]
fn note_renderer_baseline() {
    let _guard = harness::lock_gpu();
    let case = scenes::dpi(1.0);
    let instances = harness::instances_for(&case.document, case.current_track, &case.muted);
    let cam = harness::camera(case.zoom, case.scroll, case.key_count);
    let muted: Vec<usize> = case
        .muted
        .iter()
        .enumerate()
        .filter_map(|(index, muted)| muted.then_some(index))
        .collect();
    let rgba = harness::render_gpu(&instances, &cam, harness::NO_MAIN_TRACK, &muted);
    assert_baseline("note_renderer", &rgba);
}

#[test]
fn grid_renderer_baseline() {
    let _guard = harness::lock_gpu();
    let (_, device, queue) = harness::shared();
    let mut renderer = GridRenderer::new_without_depth(device, FORMAT);
    renderer.prepare(
        queue,
        &GridPrepareParams {
            viewport_size: (W as f32, H as f32),
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 0.1,
            zoom_y: 10.0,
            keyboard_width: 0.0,
            ruler_height: 0.0,
            color_bg: BG,
            color_bg_black_key: [0.013, 0.016, 0.038, 1.0],
            color_bar: [0.25, 0.28, 0.40, 1.0],
            color_beat: [0.18, 0.20, 0.30, 1.0],
            color_half_beat: [0.12, 0.14, 0.22, 1.0],
            color_grid: [0.08, 0.09, 0.15, 1.0],
            color_key_line: [0.10, 0.11, 0.18, 1.0],
            ppq: 480.0,
            max_key_index: 40.0,
            canvas_offset_x: 0.0,
            canvas_offset_y: 0.0,
            canvas_size: (W as f32, H as f32),
            time_signatures: vec![(0, 4, 4)],
        },
    );

    let mut frame = begin_offscreen();
    frame.record(|encoder, view| {
        let mut pass = begin_pass(encoder, view);
        renderer.draw(&mut pass, 0);
    });
    let rgba = frame.readback();
    assert_baseline("grid_renderer", &rgba);
}

#[test]
fn ruler_renderer_baseline() {
    let _guard = harness::lock_gpu();
    let (_, device, queue) = harness::shared();
    let mut renderer = RulerRenderer::new_without_depth(device, FORMAT);
    let params = RulerPrepareParams {
        viewport_size: (W as f32, H as f32),
        ruler_height: H as f32,
        keyboard_width: 0.0,
        scroll_x: 0.0,
        zoom_x: 0.1,
        ticks_per_measure: 1920,
        ticks_per_beat: 480,
        ppq: 480,
        time_signatures: vec![(0, 4, 4)],
    };
    renderer.prepare(device, queue, &params);
    let count = generate_ruler_instances(
        params.viewport_size.0,
        params.keyboard_width,
        params.ruler_height,
        params.scroll_x,
        params.zoom_x,
        params.ppq,
        &params.time_signatures,
    )
    .len() as u32;

    let mut frame = begin_offscreen();
    frame.record(|encoder, view| {
        let mut pass = begin_pass(encoder, view);
        renderer.draw(&mut pass, count);
    });
    let rgba = frame.readback();
    assert_baseline("ruler_renderer", &rgba);
}
