//! 离屏试画与回读校验。
//!
//! 用真实卷帘管线在离屏纹理上绘制 3 个音符，随后同步回读像素：
//! 出现非清屏像素即视为"确实画出来了"，全程无窗口、无 surface。

use std::time::{Duration, Instant};

/// 离屏检测画面宽度（256 保证回读行距天然满足 256 字节对齐）
const TEST_WIDTH: u32 = 256;
/// 离屏检测画面高度
const TEST_HEIGHT: u32 = 144;
/// 检测清屏色（不透明黑；回读时与音符像素区分）
const TEST_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
/// 离屏回读等待上限
const READBACK_TIMEOUT: Duration = Duration::from_secs(2);

/// 离屏试画卷帘：创建真实 NoteRenderer 管线，绘制 3 个音符并回读校验像素
pub(super) fn render_test(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = crate::NoteRenderer::new(device, queue, format);

    let size = wgpu::Extent3d {
        width: TEST_WIDTH,
        height: TEST_HEIGHT,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("device_check_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("device_check_depth"),
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

    // 相机参数与生产一致：zoom_x=1、zoom_y=4，使音符落于视口内且高度可回读
    let notes = [
        crate::NoteInstance::new(0.0, 126, 200.0, [1.0, 0.0, 0.0, 1.0], 0),
        crate::NoteInstance::new(0.0, 118, 200.0, [0.0, 1.0, 0.0, 1.0], 0),
        crate::NoteInstance::new(0.0, 110, 200.0, [0.0, 0.0, 1.0, 1.0], 0),
    ];
    let camera = crate::CameraUniform::new(crate::CameraParams {
        scroll: [0.0, 0.0],
        zoom: [1.0, 4.0],
        viewport: [TEST_WIDTH as f32, TEST_HEIGHT as f32],
        offset: [0.0, 0.0],
        canvas_size: [TEST_WIDTH as f32, TEST_HEIGHT as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: 127.0,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("device_check_encoder"),
    });
    renderer.prepare_notes(&mut encoder, &notes, device, queue, camera);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("device_check_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(TEST_CLEAR),
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
        renderer.draw(&mut pass, true, None);
    }

    let bytes_per_row = TEST_WIDTH * 4;
    let buffer_size = (bytes_per_row * TEST_HEIGHT) as u64;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("device_check_readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(TEST_HEIGHT),
            },
        },
        size,
    );
    queue.submit(Some(encoder.finish()));

    readback_has_content(device, &staging, buffer_size)
}

/// 同步回读离屏画面，校验存在非清屏像素（证明"确实画出了卷帘"）
fn readback_has_content(
    device: &wgpu::Device,
    staging: &wgpu::Buffer,
    buffer_size: u64,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

    let deadline = Instant::now() + READBACK_TIMEOUT;
    loop {
        if let Ok(result) = rx.try_recv() {
            result.map_err(|e| format!("回读映射失败: {e}"))?;
            let data = staging.slice(..).get_mapped_range();
            let drawn = data
                .as_chunks::<4>()
                .0
                .iter()
                .any(|px| px[0] != 0 || px[1] != 0 || px[2] != 0 || px[3] != 255);
            drop(data);
            staging.unmap();
            return if drawn {
                Ok(())
            } else {
                Err(format!(
                    "离屏画面为空白（{buffer_size} 字节均为清屏色，未画出卷帘）"
                ))
            };
        }
        if Instant::now() >= deadline {
            return Err("离屏回读超时（2 秒）".to_string());
        }
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_millis(10)),
        });
    }
}
