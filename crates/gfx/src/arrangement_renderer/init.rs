use wgpu::util::DeviceExt;

use super::{ArrangementRenderer, ArrangementUniform, INITIAL_CAPACITY, VERTEX_SHADER};
use crate::gpu_resource_tracker;

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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("arrangement_shader"),
            source: wgpu::ShaderSource::Wgsl(VERTEX_SHADER.into()),
        });

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

        // 创建 pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("arrangement_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线 - 使用实例化渲染
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("arrangement_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // 实例数据作为 vertex buffer，使用 Instance step mode
                    wgpu::VertexBufferLayout {
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
                    },
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 6 顶点组成 2 个三角形
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: crate::constants::rendering::depth_stencil_state_for(needs_depth),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 创建 uniform buffer
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("arrangement_uniform"),
            contents: bytemuck::cast_slice(&[ArrangementUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&uniform_buffer);

        // 创建 instance buffer（作为 vertex buffer 使用）
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("arrangement_instance_buffer"),
            size: (INITIAL_CAPACITY * std::mem::size_of::<super::ArrangementNoteInstance>())
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu_resource_tracker::add_buffer(&instance_buffer);

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
        let stride = std::mem::size_of::<super::ArrangementNoteInstance>();
        let needed = instance_count.next_power_of_two().max(INITIAL_CAPACITY);
        if needed > *capacity {
            gpu_resource_tracker::sub_buffer(instance_buffer);
            *capacity = needed;
            *instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("arrangement_instance_buffer"),
                size: (needed * stride) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu_resource_tracker::add_buffer(instance_buffer);
        }
    }
}
