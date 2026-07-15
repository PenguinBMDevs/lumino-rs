use super::buffer::CullBuffers;
use super::types::{CameraUniform, CullUniform};
use crate::gpu_resource_tracker;
use crate::note_renderer::NoteRenderer;
use wgpu::util::DeviceExt;

impl NoteRenderer {
    /// 创建新的音符渲染器
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("note_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::VERTEX_SHADER.into()),
        });

        let cull_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cull_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::CULL_SHADER.into()),
        });

        // 创建渲染 bind group layout
        let render_bind_group_layout = Self::create_render_bind_group_layout(device);

        // 创建计算 bind group layout
        let cull_bind_group_layout = Self::create_cull_bind_group_layout(device);

        // 创建渲染管线
        let pipeline =
            Self::create_render_pipeline(device, &shader, &render_bind_group_layout, format);

        // 创建计算管线
        let cull_pipeline =
            Self::create_cull_pipeline(device, &cull_shader, &cull_bind_group_layout);

        // 创建缓冲区
        let max_capacity = (device.limits().max_storage_buffer_binding_size as usize)
            / std::mem::size_of::<crate::NoteInstance>();
        let max_capacity = max_capacity.min(
            (device.limits().max_buffer_size as usize) / std::mem::size_of::<crate::NoteInstance>(),
        );

        let (
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            viewport_buffer,
            cull_uniform_buffer,
        ) = Self::create_renderer_buffers(device, queue);

        // 创建渲染 bind group
        let render_bind_group =
            Self::create_render_bind_group(device, &render_bind_group_layout, &viewport_buffer);

        // 创建计算 bind group（初始时无实例数据，使用0作为计数）
        let cull_bind_group = Self::build_cull_bind_group(
            device,
            &viewport_buffer,
            &cull_uniform_buffer,
            &gpu_note_buffer,
            &visible_instance_buffer,
            &indirect_buffer,
            &cull_bind_group_layout,
        );

        Self {
            pipeline,
            cull_pipeline,
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            capacity: Self::INITIAL_CAPACITY,
            max_capacity,
            last_upload_count: 0,
            viewport_buffer,
            cull_uniform_buffer,
            render_bind_group,
            cull_bind_group,
            cull_bind_group_layout,
        }
    }

    /// 创建渲染 bind group layout。
    fn create_render_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("note_render_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    /// 创建计算 bind group layout。
    fn create_cull_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("note_cull_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    /// 创建渲染管线（含 pipeline layout）。
    fn create_render_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        render_bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("note_render_pipeline_layout"),
                bind_group_layouts: &[render_bind_group_layout],
                push_constant_ranges: &[],
            });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("note_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Self::instance_buffer_layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: crate::constants::rendering::depth_stencil_state(),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// 创建计算管线（含 pipeline layout）。
    fn create_cull_pipeline(
        device: &wgpu::Device,
        cull_shader: &wgpu::ShaderModule,
        cull_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::ComputePipeline {
        let cull_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("note_cull_pipeline_layout"),
            bind_group_layouts: &[cull_bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("note_cull_pipeline"),
            layout: Some(&cull_pipeline_layout),
            module: cull_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    /// 创建所有 GPU 缓冲区（音符实例缓冲、间接绘制缓冲、视口/剔除 uniform 缓冲）。
    fn create_renderer_buffers(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (
        crate::gpu_note_buffer::GpuNoteBuffer,
        wgpu::Buffer,
        wgpu::Buffer,
        wgpu::Buffer,
        wgpu::Buffer,
    ) {
        let gpu_note_buffer = crate::gpu_note_buffer::GpuNoteBuffer::new(device, queue);
        let visible_instance_buffer =
            Self::create_instance_buffer(device, Self::INITIAL_CAPACITY, true);

        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("note_indirect_buffer"),
            size: std::mem::size_of::<super::types::DrawIndirectArgs>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu_resource_tracker::add_buffer(&indirect_buffer);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport_uniform"),
            contents: bytemuck::cast_slice(&[CameraUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&viewport_buffer);

        let cull_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cull_uniform"),
            contents: bytemuck::cast_slice(&[CullUniform {
                instance_count: 0,
                _padding: [0; 3],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&cull_uniform_buffer);

        (
            gpu_note_buffer,
            visible_instance_buffer,
            indirect_buffer,
            viewport_buffer,
            cull_uniform_buffer,
        )
    }

    /// 创建渲染 bind group。
    fn create_render_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("note_render_bind_group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        })
    }

    /// 组装 CullBuffers 并创建计算 bind group。
    fn build_cull_bind_group(
        device: &wgpu::Device,
        viewport_buffer: &wgpu::Buffer,
        cull_uniform_buffer: &wgpu::Buffer,
        gpu_note_buffer: &crate::gpu_note_buffer::GpuNoteBuffer,
        visible_instance_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
        cull_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        let cull_buffers = CullBuffers {
            layout: cull_bind_group_layout,
            viewport_buffer,
            cull_uniform_buffer,
            instance_buffer: gpu_note_buffer.buffer(),
            visible_instance_buffer,
            indirect_buffer,
            instance_count: 0,
        };
        Self::create_cull_bind_group(device, &cull_buffers)
    }
}
