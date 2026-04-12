//! 钢琴卷帘网格渲染器
//!
//! 使用 GPU Fragment Shader 高效渲染无限网格，实现 O(1) 渲染时间。

use wgpu::util::DeviceExt;

/// Camera Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridCameraUniform {
    pub viewport_size: [f32; 2],
    pub camera_pos: [f32; 2], // (scroll_x, scroll_y)
    pub zoom: [f32; 2],       // (zoom_x, zoom_y)
    pub margins: [f32; 2],    // (keyboard_width, ruler_height)
    pub color_bg: [f32; 4],
    pub color_bg_black_key: [f32; 4],
    pub color_bar: [f32; 4],
    pub color_beat: [f32; 4],
    pub color_grid: [f32; 4],
    pub color_key_line: [f32; 4],
}

impl GridCameraUniform {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        viewport_width: f32,
        viewport_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
        keyboard_width: f32,
        ruler_height: f32,
        color_bg: [f32; 4],
        color_bg_black_key: [f32; 4],
        color_bar: [f32; 4],
        color_beat: [f32; 4],
        color_grid: [f32; 4],
        color_key_line: [f32; 4],
    ) -> Self {
        Self {
            viewport_size: [viewport_width, viewport_height],
            camera_pos: [scroll_x, scroll_y],
            zoom: [zoom_x, zoom_y],
            margins: [keyboard_width, ruler_height],
            color_bg,
            color_bg_black_key,
            color_bar,
            color_beat,
            color_grid,
            color_key_line,
        }
    }
}

// 兼容旧代码的占位符（已废弃 CPU 实例生成逻辑，保留类型以减少大面积联级修改）
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridLineInstance {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub color: [f32; 4],
    pub width: f32,
    pub _padding: [f32; 3],
}

impl GridLineInstance {
    pub fn new(start: [f32; 2], end: [f32; 2], color: [f32; 4], width: f32) -> Self {
        Self {
            start,
            end,
            color,
            width,
            _padding: [0.0; 3],
        }
    }
}

/// 网格渲染器
pub struct GridRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 视口 uniform 缓冲区
    camera_buffer: wgpu::Buffer,
    /// Bind group
    bind_group: wgpu::BindGroup,
}

impl GridRenderer {
    /// 顶点着色器代码
    const SHADER_SRC: &'static str = include_str!("shaders/infinite_grid.wgsl");

    /// 创建新的网格渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("infinite_grid_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });

        // 创建 bind group layout
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

        // 创建 pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("infinite_grid_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("infinite_grid_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[], // 放弃 CPU 传递顶点
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
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建 bind group
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

    /// 准备渲染数据
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        _instances: &[GridLineInstance],
        _device: &wgpu::Device, // added underscore since it's unused
        queue: &wgpu::Queue,
        viewport_size: (f32, f32),
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
        keyboard_width: f32,
        ruler_height: f32,
        color_bg: [f32; 4],
        color_bg_black_key: [f32; 4],
        color_bar: [f32; 4],
        color_beat: [f32; 4],
        color_grid: [f32; 4],
        color_key_line: [f32; 4],
    ) {
        puffin::profile_function!();
        // 更新视口 uniform
        let viewport = GridCameraUniform::new(
            viewport_size.0,
            viewport_size.1,
            scroll_x,
            scroll_y,
            zoom_x,
            zoom_y,
            keyboard_width,
            ruler_height,
            color_bg,
            color_bg_black_key,
            color_bar,
            color_beat,
            color_grid,
            color_key_line,
        );
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[viewport]));
    }

    /// 绘制网格线
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, _instance_count: u32) {
        puffin::profile_function!();
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // 画一个全屏的四边形（4个顶点，使用 TriangleStrip）
        render_pass.draw(0..4, 0..1);
    }
}
