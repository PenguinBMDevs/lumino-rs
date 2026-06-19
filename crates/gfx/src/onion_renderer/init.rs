use super::{CameraUniform, DrawIndirectArgs, OnionRenderer, OnionViewportUniform};
use wgpu::util::DeviceExt;

impl OnionRenderer {
    /// 创建新的洋葱皮渲染器
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let vertex_shader_src = Self::VERTEX_SHADER_SRC;
        let compute_shader_src = Self::COMPUTE_SHADER_SRC;

        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_vertex_shader"),
            source: wgpu::ShaderSource::Wgsl(vertex_shader_src.into()),
        });
        let compute_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_compute_shader"),
            source: wgpu::ShaderSource::Wgsl(compute_shader_src.into()),
        });

        let max_storage_binding = device.limits().max_storage_buffer_binding_size as u64;
        let _max_buffer_size = device.limits().max_buffer_size;
        // 初始容量不要设太大，避免空项目占用过多 GPU 显存；
        // bucket 模式上传时会按需扩容。
        let initial_note_pool_capacity = Self::INITIAL_NOTE_CAPACITY;

        // ─── Compute bind group layout ──────────────────
        // binding 0: viewport uniform
        // binding 1: note_pool storage (read)
        // binding 2: instance_indices storage (read_write)
        // binding 3: indirect_args storage (read_write)
        // binding 4: key_offsets storage (read) — bucket 模式
        // binding 5: key_ranges storage (read) — bucket 模式
        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("onion_compute_bind_group_layout"),
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // ─── Render bind group layout ───────────────────
        // binding 0: camera uniform
        // binding 1: instance_indices storage (read)
        // binding 2: note_pool storage (read)
        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("onion_render_bind_group_layout"),
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

        // ─── Pipeline layouts ───────────────────────────
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_compute_pipeline_layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_render_pipeline_layout"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        // ─── Compute pipeline ───────────────────────────
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("onion_compute_pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // ─── Render pipeline ────────────────────────────
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("onion_render_pipeline"),
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
        let note_pool_buffer = Self::create_note_pool_buffer(device, initial_note_pool_capacity);
        let indices_capacity = Self::INITIAL_INDICES_CAPACITY;
        let instance_indices_buffer =
            Self::create_instance_indices_buffer(device, indices_capacity);
        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_indirect_buffer"),
            size: std::mem::size_of::<DrawIndirectArgs>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_viewport_uniform"),
            contents: bytemuck::cast_slice(&[OnionViewportUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_camera_uniform"),
            contents: bytemuck::cast_slice(&[CameraUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let key_offsets_buffer = Self::create_key_offsets_buffer(device);
        let key_ranges_buffer = Self::create_key_ranges_buffer(device);

        // ─── Bind groups ────────────────────────────────
        let compute_bind_group = Self::create_compute_bind_group(
            device,
            &compute_bind_group_layout,
            &viewport_buffer,
            &note_pool_buffer,
            &instance_indices_buffer,
            &indirect_buffer,
            &key_offsets_buffer,
            &key_ranges_buffer,
        );
        let render_bind_group = Self::create_render_bind_group(
            device,
            &render_bind_group_layout,
            &camera_buffer,
            &instance_indices_buffer,
            &note_pool_buffer,
        );

        Self {
            note_pool_buffer,
            instance_indices_buffer,
            indirect_buffer,
            viewport_buffer,
            camera_buffer,
            key_offsets_buffer,
            key_ranges_buffer,
            render_pipeline,
            compute_pipeline,
            compute_bind_group,
            render_bind_group,
            compute_bind_group_layout,
            render_bind_group_layout,
            note_pool_capacity: initial_note_pool_capacity,
            note_count: 0,
            indices_capacity,
            max_storage_binding,
            bucket_mode: false,
            last_bucket_version: 0,
            last_color_version: 0,
            last_key_min: 0,
            last_key_max: 255,
            bind_groups_dirty: false,
            last_viewport: None,
            last_camera: None,
            notes_dirty: false,
        }
    }
}
