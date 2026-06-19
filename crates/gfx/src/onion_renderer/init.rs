use super::{OnionRenderer, OnionViewportUniform};
use wgpu::util::DeviceExt;

impl OnionRenderer {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let vertex_shader_src = Self::VERTEX_SHADER_SRC;
        let compute_shader_src = Self::COMPUTE_SHADER_SRC;

        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_vertex"),
            source: wgpu::ShaderSource::Wgsl(vertex_shader_src.into()),
        });
        let compute_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_compute"),
            source: wgpu::ShaderSource::Wgsl(compute_shader_src.into()),
        });

        let max_storage_binding = device.limits().max_storage_buffer_binding_size as u64;

        // ─── Compute bind group layout (4 bindings) ───
        // binding 0: viewport uniform
        // binding 1: note_pool storage (read)
        // binding 2: instance_indices storage (read_write)
        // binding 3: indirect_args storage (read_write)
        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("onion_compute_bgl"),
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
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
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
                ],
            });

        // ─── Render bind group layout (3 bindings) ───
        // binding 0: viewport uniform
        // binding 1: instance_indices storage (read)
        // binding 2: note_pool storage (read)
        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("onion_render_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // ─── Pipeline layouts ────────────────────────────
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_compute_pl"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_render_pl"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        // ─── Compute pipeline ───────────────────────────
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("onion_compute"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // ─── Render pipeline ────────────────────────────
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("onion_render"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &vertex_module,
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ─── Buffers ────────────────────────────────────
        let note_pool_buffer = Self::create_note_pool_buffer(device, Self::INITIAL_NOTE_CAPACITY);
        let instance_indices_buffer =
            Self::create_instance_indices_buffer(device, Self::INITIAL_INDICES_CAPACITY);
        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_indirect"),
            size: std::mem::size_of::<super::types::DrawIndirectArgs>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_viewport"),
            contents: bytemuck::cast_slice(&[OnionViewportUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ─── Bind groups ────────────────────────────────
        let compute_bind_group = Self::create_compute_bind_group(
            device,
            &compute_bind_group_layout,
            &viewport_buffer,
            &note_pool_buffer,
            &instance_indices_buffer,
            &indirect_buffer,
        );
        let render_bind_group = Self::create_render_bind_group(
            device,
            &render_bind_group_layout,
            &viewport_buffer,
            &instance_indices_buffer,
            &note_pool_buffer,
        );

        Self {
            note_pool_buffer,
            instance_indices_buffer,
            indirect_buffer,
            viewport_buffer,
            render_pipeline,
            compute_pipeline,
            compute_bind_group,
            render_bind_group,
            compute_bind_group_layout,
            render_bind_group_layout,
            note_pool_capacity: Self::INITIAL_NOTE_CAPACITY,
            indices_capacity: Self::INITIAL_INDICES_CAPACITY,
            total_note_count: 0,
            max_storage_binding,
            cpu_note_pool: Vec::new(),
            last_list_version: u64::MAX,
            last_color_hash: u64::MAX,
            last_viewport: None,
        }
    }
}
