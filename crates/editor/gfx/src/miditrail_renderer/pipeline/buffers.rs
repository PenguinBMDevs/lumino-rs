use super::super::types::MiditrailCameraGpu;
use crate::gpu_resource_tracker::TrackedBuffer;

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

pub fn create_buffers(
    device: &wgpu::Device,
    vertices: &[f32],
    indices: &[u16],
) -> (TrackedBuffer, TrackedBuffer, TrackedBuffer) {
    let uniform_buffer = TrackedBuffer::new(
        device,
        &wgpu::BufferDescriptor {
            label: Some("miditrail_camera_uniform_buffer"),
            size: std::mem::size_of::<MiditrailCameraGpu>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
    );

    let vertex_buffer = TrackedBuffer::new_init(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("miditrail_cube_vertex_buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        },
    );

    let index_buffer = TrackedBuffer::new_init(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("miditrail_cube_index_buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        },
    );

    (uniform_buffer, vertex_buffer, index_buffer)
}

/// 音符平面索引缓冲（`QUAD_INDICES`，6×u16 常驻；仅音符 draw 绑定）。
pub fn create_quad_index_buffer(device: &wgpu::Device, indices: &[u16]) -> TrackedBuffer {
    TrackedBuffer::new_init(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("miditrail_quad_index_buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        },
    )
}
