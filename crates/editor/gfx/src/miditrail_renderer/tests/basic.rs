//! Miditrail 渲染器基础烟雾测试

use super::super::*;
use futures::executor::block_on;
use wgpu::util::DeviceExt;

#[test]
fn test_cube_constants() {
    assert_eq!(MiditrailRenderer::CUBE_VERTICES.len(), 144);
    assert_eq!(MiditrailRenderer::CUBE_INDICES.len(), 36);
}

/// 验证 wgpu 设备/提交/读回链路可用（与 Miditrail 无关的烟雾测试）。
#[test]
fn test_wgpu_basic_red_triangle() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("需要适配器");
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("basic_test_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求设备失败");

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("basic_test_output"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let shader = crate::shader::create_shader_module(
        &device,
        "basic_test_shader",
        r#"
            @vertex
            fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
                var pos = array<vec2<f32>, 3>(
                    vec2(-1.0, -1.0),
                    vec2(3.0, -1.0),
                    vec2(-1.0, 3.0),
                );
                return vec4(pos[idx], 0.0, 1.0);
            }
            @fragment
            fn fs_main() -> @location(0) vec4<f32> {
                return vec4(1.0, 0.0, 0.0, 1.0);
            }
        "#,
    );
    let pipeline =
        crate::pipeline::RenderPipelineBuilder::new(&device, "basic_test_pipeline", &shader)
            .opaque_target(wgpu::TextureFormat::Rgba8Unorm)
            .build();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("basic_test_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("basic_test_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    }

    let bytes_per_row = (32u32 * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("basic_test_staging"),
        contents: &vec![0u8; (bytes_per_row * 32) as usize],
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(32),
            },
        },
        wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).expect("map_async 回调发送失败");
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv()
        .expect("map_async 回调未收到")
        .expect("map_async 失败");
    let data = slice.get_mapped_range();
    let mut red_count = 0usize;
    for row in 0..32 {
        let row_start = (row * bytes_per_row) as usize;
        for col in 0..32 {
            let idx = row_start + col as usize * 4;
            if data[idx] == 255 && data[idx + 1] == 0 && data[idx + 2] == 0 {
                red_count += 1;
            }
        }
    }
    drop(data);
    staging.unmap();
    assert!(
        red_count > 100,
        "基础全屏红三角测试失败：红像素数 {red_count}"
    );
}

/// 渲染一帧并回读统计非黑像素（Normal/Top 视图切换测试共用）。
pub(super) fn render_and_count_non_black(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut MiditrailRenderer,
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
) -> usize {
    let (width, height) = (uniform.frame_width, uniform.frame_height);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_switch_encoder"),
    });
    renderer.render(device, queue, &mut encoder, uniform, notes);
    let texture = renderer.output_texture().expect("渲染后应存在输出纹理");

    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = (width * 4).next_multiple_of(align);
    let buffer_size = (padded_bytes_per_row * height) as u64;
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_switch_staging"),
        contents: &vec![0u8; buffer_size as usize],
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).expect("map_async 回调发送失败");
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv()
        .expect("map_async 回调未收到")
        .expect("map_async 失败");

    let data = slice.get_mapped_range();
    let mut non_black = 0usize;
    for row in 0..height {
        let row_start = (row * padded_bytes_per_row) as usize;
        for col in 0..width {
            let idx = row_start + (col * 4) as usize;
            if data[idx] != 0 || data[idx + 1] != 0 || data[idx + 2] != 0 {
                non_black += 1;
            }
        }
    }
    drop(data);
    staging.unmap();
    non_black
}
