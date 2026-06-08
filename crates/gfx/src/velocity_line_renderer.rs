//! 速度/CC/Bend 折线段 + 控制点渲染器
//!
//! 替代旧的 CcBarRenderer 柱状条，改为折线+控制点风格。
//! 两个独立的实例化渲染管线：
//! - `VelocityLineRenderer` — 折线段（2px 宽连接线）
//! - `VelocityCircleRenderer` — 控制点圆形标记
//!
//! 背景/网格线/中心线等矩形元素继续使用 CcBarRenderer 绘制。

use wgpu::util::DeviceExt;

// ── 折线段实例 ──

/// 折线段实例数据 — 32 bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VelocityLineInstance {
    /// 线段起点屏幕坐标
    pub start: [f32; 2],
    /// 线段终点屏幕坐标
    pub end: [f32; 2],
    /// 线段颜色 RGBA
    pub color: [f32; 4],
}

/// 折线段渲染器
pub struct VelocityLineRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    viewport_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    capacity: usize,
    cached_instances: Vec<VelocityLineInstance>,
    cache_valid: bool,
    cache_viewport_size: (f32, f32),
    cache_instance_count: usize,
}

impl VelocityLineRenderer {
    const INITIAL_CAPACITY: usize = 4096;
    const GROWTH_FACTOR: usize = 2;
    const SHADER_SRC: &'static str = include_str!("shaders/velocity_line.wgsl");

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("velocity_line_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("velocity_line_bind_group_layout"),
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
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("velocity_line_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("velocity_line_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Self::instance_buffer_layout()],
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

        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("velocity_line_viewport_uniform"),
            contents: bytemuck::cast_slice(&[super::cc_bar_renderer::CcBarViewportUniform::new(
                800.0, 600.0,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("velocity_line_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            instance_buffer,
            viewport_buffer,
            bind_group,
            capacity: Self::INITIAL_CAPACITY,
            cached_instances: Vec::new(),
            cache_valid: false,
            cache_viewport_size: (0.0, 0.0),
            cache_instance_count: 0,
        }
    }

    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("velocity_line_instance_buffer"),
            size: (capacity * std::mem::size_of::<VelocityLineInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VelocityLineInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[VelocityLineInstance],
        viewport_size: (f32, f32),
    ) {
        puffin::profile_function!();

        let instance_count = instances.len();
        let params_changed = !self.cache_valid
            || self.cache_viewport_size != viewport_size
            || self.cache_instance_count != instance_count;

        if params_changed {
            self.cached_instances.clear();
            self.cached_instances.extend_from_slice(instances);
            self.cache_viewport_size = viewport_size;
            self.cache_instance_count = instance_count;
            self.cache_valid = true;
        }

        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        let viewport_uniform =
            super::cc_bar_renderer::CcBarViewportUniform::new(viewport_size.0, viewport_size.1);
        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }

    pub fn draw(&self, render_pass: &mut wgpu::RenderPass, instance_count: u32) {
        puffin::profile_function!();
        if instance_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, 0..instance_count);
    }
}

// ── 控制点圆形实例 ──

/// 控制点圆形实例数据 — 32 bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VelocityCircleInstance {
    /// 圆心屏幕坐标
    pub center: [f32; 2],
    /// 半径（像素）
    pub radius: f32,
    /// Padding 到 16 字节对齐
    pub _pad: f32,
    /// 颜色 RGBA
    pub color: [f32; 4],
}

/// 控制点圆形渲染器
pub struct VelocityCircleRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    viewport_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    capacity: usize,
    cached_instances: Vec<VelocityCircleInstance>,
    cache_valid: bool,
    cache_viewport_size: (f32, f32),
    cache_instance_count: usize,
}

impl VelocityCircleRenderer {
    const INITIAL_CAPACITY: usize = 4096;
    const GROWTH_FACTOR: usize = 2;
    const SHADER_SRC: &'static str = include_str!("shaders/velocity_circle.wgsl");

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("velocity_circle_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("velocity_circle_bind_group_layout"),
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
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("velocity_circle_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("velocity_circle_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Self::instance_buffer_layout()],
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

        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("velocity_circle_viewport_uniform"),
            contents: bytemuck::cast_slice(&[super::cc_bar_renderer::CcBarViewportUniform::new(
                800.0, 600.0,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("velocity_circle_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            instance_buffer,
            viewport_buffer,
            bind_group,
            capacity: Self::INITIAL_CAPACITY,
            cached_instances: Vec::new(),
            cache_valid: false,
            cache_viewport_size: (0.0, 0.0),
            cache_instance_count: 0,
        }
    }

    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("velocity_circle_instance_buffer"),
            size: (capacity * std::mem::size_of::<VelocityCircleInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VelocityCircleInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[VelocityCircleInstance],
        viewport_size: (f32, f32),
    ) {
        puffin::profile_function!();

        let instance_count = instances.len();
        let params_changed = !self.cache_valid
            || self.cache_viewport_size != viewport_size
            || self.cache_instance_count != instance_count;

        if params_changed {
            self.cached_instances.clear();
            self.cached_instances.extend_from_slice(instances);
            self.cache_viewport_size = viewport_size;
            self.cache_instance_count = instance_count;
            self.cache_valid = true;
        }

        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        let viewport_uniform =
            super::cc_bar_renderer::CcBarViewportUniform::new(viewport_size.0, viewport_size.1);
        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }

    pub fn draw(&self, render_pass: &mut wgpu::RenderPass, instance_count: u32) {
        puffin::profile_function!();
        if instance_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, 0..instance_count);
    }
}

#[cfg(test)]
#[path = "velocity_line_renderer/tests.rs"]
mod tests;
