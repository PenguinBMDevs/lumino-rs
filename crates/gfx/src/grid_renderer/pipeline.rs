//! GridRenderer 渲染管线创建

use wgpu::util::DeviceExt;

use super::camera::GridCameraUniform;

/// 网格渲染器
pub struct GridRenderer {
    pub pipeline: wgpu::RenderPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl GridRenderer {
    /// 顶点着色器代码
    const SHADER_SRC: &'static str = include_str!("../shaders/infinite_grid.wgsl");

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("infinite_grid_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("infinite_grid_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("infinite_grid_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("infinite_grid_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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
        });

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("infinite_grid_camera_uniform"),
            contents: bytemuck::cast_slice(&[GridCameraUniform::new(
                1.0,
                1.0,
                0.0,
                0.0,
                1.0,
                1.0,
                0.0,
                0.0,
                [0.1, 0.1, 0.1, 1.0],
                [0.07, 0.07, 0.07, 1.0],
                [0.3, 0.3, 0.3, 1.0],
                [0.2, 0.2, 0.2, 1.0],
                [0.15, 0.15, 0.15, 1.0],
                [0.15, 0.15, 0.15, 1.0],
                [0.15, 0.15, 0.15, 1.0],
                1920.0,
                127.0,
                0.0,
                0.0,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("infinite_grid_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            camera_buffer,
            bind_group,
        }
    }
}
