//! CPU 洋葱皮叠加层测试：纯逻辑断言 + GPU 读回像素证据。

use crate::RenderParams;
use crate::constants::rendering::{DEPTH_FORMAT, is_depth_stencil_compatible};
use crate::pipeline::test_device;

use super::{
    CpuOnionFrame, CpuOnionRenderer,
    pipeline::{self, CpuOnionUniform},
    upload::{UploadPlan, UploadState},
};

/// 输出目标格式（与 UI 离屏纹理一致：sRGB）。
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// `copy_texture_to_buffer` 的行距对齐常量（256 字节）。
const READBACK_STRIDE: u32 = 256;

#[test]
fn cpu_onion_frame_defaults_to_none() {
    let params = RenderParams::default();
    assert!(
        params.cpu_onion_frame.is_none(),
        "默认必须为 None：不接入时渲染行为零变化"
    );
}

#[test]
fn cpu_onion_frame_builder_does_not_drift_from_default() {
    let built = RenderParams::builder().build();
    assert!(
        built.cpu_onion_frame.is_none(),
        "builder 默认值必须从主类型拷贝（不得漂移成 Some）"
    );

    let frame = CpuOnionFrame::new(1, 2, 3, 4, vec![0u8; 3 * 4 * 4], 11);
    let params = RenderParams::builder()
        .cpu_onion_frame(Some(frame.clone()))
        .build();
    let got = params
        .cpu_onion_frame
        .as_ref()
        .expect("builder 应把传入的 CPU 洋葱皮帧带进 RenderParams");
    assert_eq!(got.version, 11);
    assert_eq!(got.size(), (3, 4));

    // RenderParams 的 Clone / Debug 必须能处理 CpuOnionFrame（Debug 不打印像素）
    let cloned = params.clone();
    assert!(
        cloned.cpu_onion_frame.is_some(),
        "RenderParams::clone 必须保留 CPU 洋葱皮帧"
    );
    let text = format!("{cloned:?}");
    assert!(
        text.contains("cpu_onion_frame"),
        "RenderParams Debug 必须包含 CPU 洋葱皮字段"
    );
}

#[test]
fn cpu_onion_uniform_carries_physical_rect_and_viewport() {
    let frame = CpuOnionFrame::new(12, 34, 5, 6, vec![0u8; 5 * 6 * 4], 9);
    let uniform = CpuOnionUniform::new((800, 600), &frame);

    assert_eq!(uniform.viewport, [800.0, 600.0, 0.0, 0.0]);
    assert_eq!(
        uniform.rect,
        [12.0, 34.0, 5.0, 6.0],
        "uniform 必须按物理像素原样传递矩形（不做 DPI 缩放）"
    );
}

#[test]
fn cpu_onion_pipeline_depth_state_matches_ui_pass_contract() {
    let state = pipeline::depth_stencil_state();
    assert_eq!(state.format, DEPTH_FORMAT, "depth 格式必须与 UI pass 一致");
    assert!(!state.depth_write_enabled, "叠加层不得写深度");
    assert_eq!(state.depth_compare, wgpu::CompareFunction::Always);
    assert_eq!(state.bias.constant, 0);
    assert_eq!(state.bias.slope_scale, 0.0);
    assert_eq!(state.bias.clamp, 0.0);

    assert!(
        is_depth_stencil_compatible(true, Some(&state)),
        "带 depth attachment 的 UI pass 必须接受该管线"
    );
    assert!(
        !is_depth_stencil_compatible(false, Some(&state)),
        "无 depth 的导出 pass 不兼容——调用方必须先判定 depth 存在（本步不接导出）"
    );
}

#[test]
fn cpu_onion_upload_state_skips_repeated_version() {
    let mut state = UploadState::default();
    let frame = CpuOnionFrame::new(0, 0, 3, 3, vec![0u8; 3 * 3 * 4], 5);

    assert_eq!(state.plan(&frame), UploadPlan::RecreateAndUpload);
    state.record(&frame);
    assert_eq!(
        state.plan(&frame),
        UploadPlan::Reuse,
        "尺寸与版本都相同的帧不得重复上传"
    );
    assert_eq!(state.plan(&frame.clone()), UploadPlan::Reuse);
}

#[test]
fn cpu_onion_invalid_frame_is_not_drawn() {
    let (device, queue) = test_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);
    renderer.set_viewport(2, 2);

    let bad = CpuOnionFrame::new(0, 0, 2, 2, vec![0u8; 15], 1);
    renderer.prepare(&queue, &bad);
    assert_eq!(
        renderer.uploaded_version(),
        None,
        "像素长度不匹配的帧不得进入上传路径"
    );
}

/// GPU 读回：2x2 源帧（左上不透明红、其余透明）画到清黑目标上，验证：
/// - 不透明像素逐字节一致（sRGB 采样 → sRGB 写回的往返无损）；
/// - 透明像素保持清屏色（alpha = 0 不改变目标）。
#[test]
fn cpu_onion_gpu_quad_preserves_source_pixels() {
    let (device, queue) = test_device();

    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);
    renderer.set_viewport(2, 2);

    let mut pixels = vec![0u8; 16];
    pixels[0..4].copy_from_slice(&[255, 0, 0, 255]);
    let frame = CpuOnionFrame::new(0, 0, 2, 2, pixels, 1);
    renderer.prepare(&queue, &frame);
    assert_eq!(renderer.uploaded_version(), Some(1));

    // 同版本再次 prepare：内容未变化，不重复上传（版本保持 1）
    renderer.prepare(&queue, &frame);
    assert_eq!(
        renderer.uploaded_version(),
        Some(1),
        "相同 version 的帧不得触发重新上传"
    );

    let color = create_color_target(&device);
    let depth = create_depth_target(&device);
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("cpu_onion_gpu_test_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cpu_onion_gpu_test_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, &frame);
    }
    queue.submit(Some(encoder.finish()));

    let bytes = readback_rgba8(&device, &queue, &color, 2, 2);
    assert_eq!(
        pixel(&bytes, 2, 0, 0),
        [255, 0, 0, 255],
        "不透明红像素必须逐字节一致（sRGB 采样/写回往返）"
    );
    for (x, y) in [(1, 0), (0, 1), (1, 1)] {
        assert_eq!(
            pixel(&bytes, 2, x, y),
            [0, 0, 0, 255],
            "透明像素必须保持清屏黑（alpha = 0 不改变目标）：({x}, {y})"
        );
    }
}

/// 创建 2x2 RGBA8 sRGB 颜色目标（`RENDER_ATTACHMENT | COPY_SRC`）。
fn create_color_target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cpu_onion_gpu_test_color"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// 创建 2x2 深度目标（与管线 depth-stencil 状态匹配）。
fn create_depth_target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cpu_onion_gpu_test_depth"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
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

/// 同步回读 RGBA8 纹理（去掉 256 字节行距填充），超时以断言失败收尾（不无限阻塞）。
fn readback_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cpu_onion_gpu_test_staging"),
        size: u64::from(READBACK_STRIDE) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("cpu_onion_gpu_test_readback"),
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
                bytes_per_row: Some(READBACK_STRIDE),
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

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Ok(result) = rx.try_recv() {
            result.expect("CPU 洋葱皮 GPU 测试：回读 map 失败");
            let data = slice.get_mapped_range();
            let row_bytes = (width * 4) as usize;
            let mut out = Vec::with_capacity(row_bytes * height as usize);
            for row in 0..height as usize {
                let start = row * READBACK_STRIDE as usize;
                out.extend_from_slice(&data[start..start + row_bytes]);
            }
            drop(data);
            staging.unmap();
            return out;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "CPU 洋葱皮 GPU 测试：回读超时（30s，GPU 可能被驱动重置或设备争用卡死）"
        );
        let _ = device.poll(wgpu::PollType::Poll);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// 取紧凑排布（无行距填充）RGBA8 图像的像素。
fn pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]
}
