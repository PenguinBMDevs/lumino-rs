use wgpu::util::DeviceExt;

use super::{ArrangementRenderer, ArrangementUniform, INITIAL_CAPACITY, VERTEX_SHADER};
use crate::gpu_resource_tracker;
use crate::pipeline::RenderPipelineBuilder;
use crate::shader::create_shader_module;

impl ArrangementRenderer {
    /// 创建新的走带渲染器（默认带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的走带渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader = create_shader_module(device, "arrangement_shader", VERTEX_SHADER);

        // 创建 bind group layout - 只绑定 uniform buffer
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("arrangement_bind_group_layout"),
            entries: &[
                // binding 0: uniform buffer
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
            ],
        });

        // 创建渲染管线 - 使用实例化渲染
        let pipeline = RenderPipelineBuilder::new(device, "arrangement_pipeline", &shader)
            .bind_group(&bind_group_layout)
            // 实例数据作为 vertex buffer，使用 Instance step mode
            .vertex_buffer(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<super::ArrangementNoteInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    // location 0: xywh (Float32x4)
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 0,
                        shader_location: 0,
                    },
                    // location 1: packed (Uint32x4)
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Uint32x4,
                        offset: 16,
                        shader_location: 1,
                    },
                ],
            })
            .alpha_blended_target(format)
            .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                needs_depth,
            ))
            .build();

        // 创建 uniform buffer
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("arrangement_uniform"),
            contents: bytemuck::cast_slice(&[ArrangementUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&uniform_buffer);

        // 创建 instance buffer（作为 vertex buffer 使用）
        let instance_buffer = gpu_resource_tracker::create_instance_buffer::<
            super::ArrangementNoteInstance,
        >(device, "arrangement_instance_buffer", INITIAL_CAPACITY);

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("arrangement_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            uniform_buffer,
            instance_buffer,
            bind_group,
            capacity: INITIAL_CAPACITY,
            last_instance_count: 0,
        }
    }

    /// 确保 instance buffer 容量足够
    pub(super) fn ensure_capacity(
        instance_buffer: &mut wgpu::Buffer,
        capacity: &mut usize,
        device: &wgpu::Device,
        instance_count: usize,
    ) {
        let needed = instance_count.next_power_of_two().max(INITIAL_CAPACITY);
        if needed > *capacity {
            gpu_resource_tracker::sub_buffer(instance_buffer);
            *capacity = needed;
            *instance_buffer = gpu_resource_tracker::create_instance_buffer::<
                super::ArrangementNoteInstance,
            >(device, "arrangement_instance_buffer", needed);
        }
    }
}
