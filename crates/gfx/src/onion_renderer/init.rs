use super::{OnionRenderer, OnionViewportUniform};
use wgpu::util::DeviceExt;

impl OnionRenderer {
    /// 创建新的洋葱皮渲染器
    ///
    /// 相比旧版：
    /// - 不再创建 compute pipeline
    /// - 不再创建 instance_indices_buffer / indirect_buffer / key_offsets_buffer / key_ranges_buffer
    /// - 只创建 note_pool_buffer + viewport_buffer + render_pipeline
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader_src = Self::SHADER_SRC;
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let max_storage_binding = device.limits().max_storage_buffer_binding_size as u64;

        // ─── Render bind group layout ───────────────────
        // binding 0: viewport uniform
        // binding 1: note_pool storage (read)
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
                ],
            });

        // ─── Render pipeline layout ─────────────────────
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_render_pipeline_layout"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        // ─── Render pipeline ────────────────────────────
        // 使用 TriangleStrip + 4 顶点/实例（与 note_renderer 一致）
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("onion_render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[], // 使用 storage buffer 取实例数据，无需 vertex buffer
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
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
        let initial_note_pool_capacity = Self::INITIAL_NOTE_CAPACITY;
        let note_pool_buffer = Self::create_note_pool_buffer(device, initial_note_pool_capacity);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_viewport_uniform"),
            contents: bytemuck::cast_slice(&[OnionViewportUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ─── Bind group ─────────────────────────────────
        let render_bind_group = Self::create_render_bind_group(
            device,
            &render_bind_group_layout,
            &viewport_buffer,
            &note_pool_buffer,
        );

        Self {
            note_pool_buffer,
            viewport_buffer,
            render_pipeline,
            render_bind_group,
            render_bind_group_layout,
            note_pool_capacity: initial_note_pool_capacity,
            note_count: 0,
            max_storage_binding,
            cpu_note_pool: Vec::new(),
            last_list_version: u64::MAX,
            last_color_hash: u64::MAX,
        }
    }
}
