//! GPU/CPU 像素对比装置：离屏 `Rgba8UnormSrgb` 目标 + 回读 + 逐像素差异统计。
//!
//! 双方约定（与生产一致，不在测试里另立口径）：
//! - 目标格式 `Rgba8UnormSrgb`（与 UI 通道一致），清屏色 **全透明**
//!   → GPU 背景字节 = `[0, 0, 0, 0]`，与 CPU 帧的透明背景同值，可整帧逐像素比较；
//! - GPU 侧走生产主路径：`NoteRenderer::new_onion_skin` +
//!   `prepare_pass`（compute cull）+ `draw`（indirect），不做任何为对齐测试而
//!   改写的裁剪/直绘；
//! - CPU 侧走 `CpuOnionRenderer::render_full`，`current_track` / `muted` 由用例给出。

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use lumino_cpu_onion::{CanvasCamera, CpuFrame, CpuOnionRenderer, TrackIndex};
use lumino_gfx::constants::rendering::DEPTH_FORMAT;
use lumino_gfx::{CameraUniform, GpuEpoch, NoteInstance, NoteRenderer};
use lumino_midi_model::MidiDocument;

/// 测试画布宽：`W * 4 = 1024` 字节/行，天然满足 `copy_texture_to_buffer` 的
/// 256 字节行对齐要求，回读无 padding。
pub const W: u32 = 256;
/// 测试画布高。
pub const H: u32 = 144;
/// 颜色目标格式（与生产 UI 通道一致；sRGB 格式写入时由硬件做 linear→sRGB）。
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// 无主音轨（GPU `ViewState.current_track` 编码：0 = 全部按洋葱皮绘制）。
pub const NO_MAIN_TRACK: u32 = 0;

/// GPU 测试串行锁：单设备多渲染器并行提交在部分驱动上会互相拖死
/// （与 `note_renderer::depth_tests` 同因），本模块统一串行。
static GPU_LOCK: Mutex<()> = Mutex::new(());
/// 进程内共享的实例 + 设备 + 队列（`Instance` 必须与 `Device` 同寿命持有）。
static SHARED: OnceLock<(wgpu::Instance, wgpu::Device, wgpu::Queue)> = OnceLock::new();

/// 获取 GPU 测试串行锁（中毒不连带失败）。
pub fn lock_gpu() -> MutexGuard<'static, ()> {
    GPU_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 共享测试设备（首次调用创建，进程内复用）。
fn shared() -> &'static (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    SHARED.get_or_init(|| {
        use futures::executor::block_on;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("像素对比需要可用的 wgpu 适配器");
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("cpu_gpu_parity_device"),
            required_features: adapter.features() & wgpu::Features::default(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .expect("请求像素对比设备失败");
        (instance, device, queue)
    })
}

/// 相机（画布 = canvas 区域本身，无键盘栏/标尺偏移）。
pub fn camera(zoom: [f32; 2], scroll: [f32; 2], key_count: u16) -> CanvasCamera {
    CanvasCamera {
        width: W,
        height: H,
        scroll_x: scroll[0],
        scroll_y: scroll[1],
        zoom_x: zoom[0],
        zoom_y: zoom[1],
        key_count,
    }
}

/// CPU 相机 → GPU uniform（`keyboard_width` / `ruler_height` / `canvas_offset`
/// 全 0，`max_key_index = key_count - 1`，与 CPU 的键坐标口径逐项对齐）。
pub fn gpu_camera(cam: &CanvasCamera) -> CameraUniform {
    CameraUniform {
        scroll: [cam.scroll_x, cam.scroll_y],
        zoom: [cam.zoom_x, cam.zoom_y],
        viewport_size: [cam.width as f32, cam.height as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [cam.width as f32, cam.height as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: cam.max_key() as f32,
        _padding: [0.0; 3],
    }
}

/// 按生产洋葱皮实例构建规则，把「CPU 会绘制的轨道」转成 GPU 实例。
///
/// 与 `crates/editor/ui/src/host/render/onion_skin/stream/instances.rs` 同源：
/// - 颜色 = `current_track_color_f32(track)`（CPU `NoteStyle` 与 GPU 实例共用的
///   唯一取色入口，测试不复制调色板逻辑）；
/// - `start_length = [start_tick, length]`（tick 单位）；
/// - `border_width = 低 16 位边框宽（1 → 每侧 2px） | 高 16 位 track+1`。
///   高 16 位是洋葱皮区的轨道编码：只要不等于 `ViewState.current_track`，
///   着色器就按洋葱皮轨道出实例色（这里恒传 `NO_MAIN_TRACK` = 0）。
///
/// 该编码与 `ui::onion_skin::onion_border_width` 同式；后者是 `pub(crate)`，
/// 测试无法引用，故此处按同一表达式重写并在文档中登记（Step 6b 已知边界）。
pub fn instances_for(
    document: &MidiDocument,
    current_track: usize,
    muted: &[bool],
) -> Vec<NoteInstance> {
    let mut instances = Vec::new();
    for track in 0..document.notes.len() {
        if track == current_track || muted.get(track).copied().unwrap_or(false) {
            continue;
        }
        let color = lumino_extras::palette::current_track_color_f32(track);
        let border = lumino_cpu_onion::color::NOTE_BORDER_WIDTH | (((track as u32) + 1) << 16);
        for note in document.track_notes(track).iter() {
            instances.push(NoteInstance::new(
                note.start_tick as f32,
                note.key,
                note.length() as f32,
                color,
                border,
            ));
        }
    }
    instances
}

/// CPU 侧渲染一帧（透明背景 + 洋葱皮音符）。
pub fn render_cpu(
    document: &Arc<MidiDocument>,
    index: &Arc<TrackIndex>,
    cam: &CanvasCamera,
    current_track: usize,
    muted: &[bool],
) -> CpuFrame {
    // 4 band：与生产默认 worker 数同量级，band 切分不影响逐像素结果。
    let mut renderer = CpuOnionRenderer::new(4, document.notes.len());
    renderer.resize(W, H);
    renderer
        .render_full(
            Arc::clone(document),
            Arc::clone(index),
            cam,
            current_track,
            muted,
        )
        .clone()
}

/// GPU 侧渲染一帧并回读（生产主路径：compute cull + indirect draw）。
pub fn render_gpu(
    instances: &[NoteInstance],
    cam: &CanvasCamera,
    current_track_enc: u32,
    muted_tracks: &[usize],
) -> Vec<u8> {
    let (_, device, queue) = shared();
    let mut renderer = NoteRenderer::new_onion_skin(device, queue, FORMAT, GpuEpoch::INITIAL);
    renderer.set_view_state(queue, current_track_enc, muted_tracks);
    renderer.upload_instances(instances, device, queue);

    let extent = wgpu::Extent3d {
        width: W,
        height: H,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cpu_gpu_parity_color"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cpu_gpu_parity_depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("cpu_gpu_parity_encoder"),
    });
    renderer.prepare_pass(&mut encoder, gpu_camera(cam), queue);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cpu_gpu_parity_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // 清全透明：与 CPU 帧背景 [0,0,0,0] 同值，可整帧比较。
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    readback(device, queue, &color)
}

/// 回读整帧 RGBA8。
fn readback(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = W * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cpu_gpu_parity_staging"),
        size: (bytes_per_row * H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("cpu_gpu_parity_readback"),
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
    // 非阻塞轮询 + 有界等待（禁止无限阻塞：设备争用/驱动卡死时测试不能挂死进程）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Ok(result) = rx.try_recv() {
            result.expect("像素对比回读 map 失败");
            let data = slice.get_mapped_range();
            let out = data.to_vec();
            drop(data);
            staging.unmap();
            return out;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "像素对比回读超时（30s，GPU 可能被驱动重置或设备争用卡死）"
        );
        let _ = device.poll(wgpu::PollType::Poll);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
