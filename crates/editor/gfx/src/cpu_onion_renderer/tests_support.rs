//! CPU 洋葱皮测试共享夹具：离屏目标 + 叠加层绘制 + GPU 读回。
//!
//! 仅 `#[cfg(test)]` 引入：`tests.rs`（整帧/四边形一致性）与 `tests_partial.rs`
//!（脏矩形局部上传）共用同一套读回路径，避免两份驱动细节各自漂移。

use crate::constants::rendering::DEPTH_FORMAT;

use super::{CpuOnionFrame, CpuOnionRenderer};

/// 输出目标格式（与 UI 离屏纹理一致：sRGB）。
pub(super) const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// `copy_texture_to_buffer` 的行距对齐常量（256 字节）。
const READBACK_STRIDE: u32 = 256;

/// 把一帧画到清黑离屏目标上并同步读回（紧凑排布 RGBA8）。
///
/// 调用方必须已 `prepare` 该帧。视口被设置成帧尺寸，四边形因此 1:1 铺满目标：
/// - alpha = 255 的像素 → 读回到纹理里的原字节（sRGB 采样/写回往返无损）；
/// - alpha = 0 的像素 → 保持清屏黑（不改变目标）。
pub(super) fn render_and_readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut CpuOnionRenderer,
    frame: &CpuOnionFrame,
) -> Vec<u8> {
    let (width, height) = frame.size();
    renderer.set_viewport(width, height);
    // 视口写进 uniform 才能让四边形 1:1 铺满目标：同版本再次 `prepare` 只会
    // 触发 uniform 写入（`Reuse` 不上传），因此不会掩盖任何上传行为。
    renderer.prepare(queue, frame);
    let color = create_color_target(device, width, height);
    let depth = create_depth_target(device, width, height);
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
        renderer.draw(&mut pass, frame);
    }
    queue.submit(Some(encoder.finish()));

    readback_rgba8(device, queue, &color, width, height)
}

/// 创建 RGBA8 sRGB 颜色目标（`RENDER_ATTACHMENT | COPY_SRC`）。
pub(super) fn create_color_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cpu_onion_gpu_test_color"),
        size: wgpu::Extent3d {
            width,
            height,
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

/// 创建深度目标（与管线 depth-stencil 状态匹配）。
fn create_depth_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cpu_onion_gpu_test_depth"),
        size: wgpu::Extent3d {
            width,
            height,
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

/// 取紧凑排布（无行距填充）RGBA8 图像的像素（越界返回全零，测试侧不 panic）。
pub(super) fn pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    let mut out = [0u8; 4];
    if let Some(chunk) = bytes.get(offset..offset + 4) {
        out.copy_from_slice(chunk);
    }
    out
}
