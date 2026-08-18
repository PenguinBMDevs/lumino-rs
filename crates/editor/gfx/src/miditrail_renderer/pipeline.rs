//! Miditrail 3D 渲染管线创建

use super::MiditrailInstanceGpu;
use super::types::{MiditrailAuraInstanceGpu, MiditrailCameraGpu};
use crate::pipeline::RenderPipelineBuilder;
use wgpu::util::DeviceExt;

pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("miditrail_bind_group_layout"),
        entries: &[
            // binding 0: 相机 uniform
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // binding 1: aura 纹理采样器
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // binding 2: aura 纹理
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

pub fn create_render_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    create_instanced_pipeline(
        device,
        bind_group_layout,
        shader,
        "miditrail_render_pipeline",
        true,
    )
}

/// 创建音符渲染管线（不写入深度缓冲，配合 Painter's algorithm 与琴键最后绘制）。
///
/// 参考 Comet MIDITrail：音符先绘制且不写深度，琴键后绘制使用深度测试，
/// 从而保证琴键始终覆盖在音符之上。
pub fn create_note_render_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    create_instanced_pipeline(
        device,
        bind_group_layout,
        shader,
        "miditrail_note_render_pipeline",
        false,
    )
}

fn create_instanced_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    depth_write: bool,
) -> wgpu::RenderPipeline {
    // 顶点位置与法线
    let pos_normal_layout = wgpu::VertexBufferLayout {
        array_stride: 24,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
        ],
    };
    // 实例数据
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<MiditrailInstanceGpu>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 16,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 32,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 36,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 40,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 44,
                shader_location: 7,
            },
        ],
    };

    RenderPipelineBuilder::new(device, label, shader)
        .bind_group(bind_group_layout)
        .vertex_buffer(pos_normal_layout)
        .vertex_buffer(instance_layout)
        .opaque_target(wgpu::TextureFormat::Rgba8Unorm)
        .depth_stencil(Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: depth_write,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }))
        .build()
}

pub fn create_aura_render_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    // 顶点位置与 UV
    let pos_uv_layout = wgpu::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
        ],
    };
    // Aura 实例数据
    let aura_instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<MiditrailAuraInstanceGpu>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 0,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 4,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 8,
                shader_location: 4,
            },
        ],
    };

    RenderPipelineBuilder::new(device, "miditrail_aura_render_pipeline", shader)
        .bind_group(bind_group_layout)
        .vertex_buffer(pos_uv_layout)
        .vertex_buffer(aura_instance_layout)
        // 叠加混合（SrcAlpha, One）
        .color_target(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            write_mask: wgpu::ColorWrites::ALL,
        })
        .depth_stencil(Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }))
        .build()
}

pub fn create_buffers(
    device: &wgpu::Device,
    vertices: &[f32],
    indices: &[u16],
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer) {
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("miditrail_camera_uniform_buffer"),
        size: std::mem::size_of::<MiditrailCameraGpu>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    crate::gpu_resource_tracker::add_buffer(&uniform_buffer);

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_cube_vertex_buffer"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    crate::gpu_resource_tracker::add_buffer(&vertex_buffer);

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_cube_index_buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    crate::gpu_resource_tracker::add_buffer(&index_buffer);

    (uniform_buffer, vertex_buffer, index_buffer)
}

pub fn create_aura_buffers(device: &wgpu::Device) -> (wgpu::Buffer, wgpu::Buffer) {
    const AURA_VERTICES: [f32; 16] = [
        -1.0, -1.0, 0.0, 0.0, 1.0, -1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 0.0, 1.0,
    ];
    const AURA_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_aura_vertex_buffer"),
        contents: bytemuck::cast_slice(&AURA_VERTICES),
        usage: wgpu::BufferUsages::VERTEX,
    });
    crate::gpu_resource_tracker::add_buffer(&vertex_buffer);

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_aura_index_buffer"),
        contents: bytemuck::cast_slice(&AURA_INDICES),
        usage: wgpu::BufferUsages::INDEX,
    });
    crate::gpu_resource_tracker::add_buffer(&index_buffer);

    (vertex_buffer, index_buffer)
}

pub fn create_aura_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("miditrail_aura_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

pub fn create_aura_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
    data: &[u8],
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("miditrail_aura_texture"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    crate::gpu_resource_tracker::add_texture(&texture);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * 4),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
    texture
}

/// 生成一个软环形 Aura 纹理数据（RGBA8，size x size）。
pub fn generate_aura_ring_data(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let center = (size - 1) as f32 * 0.5;
    let radius = size as f32 * 0.5;
    let inner = radius * 0.35;
    let outer = radius * 0.85;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha = if dist < inner || dist > outer {
                0.0
            } else {
                let mid = (inner + outer) * 0.5;
                let half = (outer - inner) * 0.5;
                let t = 1.0 - ((dist - mid) / half).abs();
                t * t * (3.0 - 2.0 * t)
            };
            let idx = ((y * size + x) * 4) as usize;
            data[idx] = 255;
            data[idx + 1] = 255;
            data[idx + 2] = 255;
            data[idx + 3] = (alpha * 255.0) as u8;
        }
    }
    data
}
