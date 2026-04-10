//! 钢琴卷帘网格渲染器
//!
//! 使用 wgpu 高效渲染网格线，替代 Canvas 绘制以提升性能

use wgpu::util::DeviceExt;

/// 网格线实例数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridLineInstance {
    /// 起点位置 (x, y)
    pub start: [f32; 2],
    /// 终点位置 (x, y)
    pub end: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
    /// 线宽
    pub width: f32,
    /// 填充
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

/// 视口 Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridViewportUniform {
    pub viewport_size: [f32; 2],
    pub _padding: [f32; 2],
}

impl GridViewportUniform {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            viewport_size: [width, height],
            _padding: [0.0; 2],
        }
    }
}

/// 网格渲染器
pub struct GridRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 实例缓冲区
    instance_buffer: wgpu::Buffer,
    /// 视口 uniform 缓冲区
    viewport_buffer: wgpu::Buffer,
    /// Bind group
    bind_group: wgpu::BindGroup,
    /// 当前缓冲区容量（实例数量）
    capacity: usize,
}

impl GridRenderer {
    /// 初始缓冲区容量（增大以支持更密集的网格）
    const INITIAL_CAPACITY: usize = 8192;
    /// 缓冲区扩容因子
    const GROWTH_FACTOR: usize = 2;
    /// 顶点着色器代码
    const VERTEX_SHADER: &'static str = include_str!("shaders/grid.wgsl");

    /// 创建新的网格渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::VERTEX_SHADER.into()),
        });

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid_bind_group_layout"),
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

        // 创建 pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid_pipeline"),
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 创建缓冲区
        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid_viewport_uniform"),
            contents: bytemuck::cast_slice(&[GridViewportUniform::new(1.0, 1.0)]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid_bind_group"),
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
        }
    }

    /// 准备渲染数据
    pub fn prepare(
        &mut self,
        instances: &[GridLineInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport_size: (f32, f32),
    ) {
        // 更新视口 uniform
        let viewport = GridViewportUniform::new(viewport_size.0, viewport_size.1);
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[viewport]));

        if instances.is_empty() {
            return;
        }

        // 检查是否需要扩容
        if instances.len() > self.capacity {
            self.grow_buffer(device, instances.len());
        }

        // 上传实例数据
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
    }

    /// 绘制网格线
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, instance_count: u32) {
        if instance_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, 0..instance_count);
    }

    /// 创建实例缓冲区
    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid_instance_buffer"),
            size: (capacity * std::mem::size_of::<GridLineInstance>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// 扩容缓冲区（使用 saturating_mul 防止溢出）
    fn grow_buffer(&mut self, device: &wgpu::Device, required_capacity: usize) {
        let new_capacity = self
            .capacity
            .saturating_mul(Self::GROWTH_FACTOR)
            .max(required_capacity);
        if new_capacity > self.capacity {
            tracing::debug!(
                "GridRenderer: growing buffer {} -> {}",
                self.capacity,
                new_capacity
            );
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }
    }

    /// 实例缓冲区布局
    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GridLineInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // start
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                // end
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
                // color
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
                // width
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 32,
                    shader_location: 3,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 GridLineInstance 创建
    #[test]
    fn test_grid_line_instance_creation() {
        let line = GridLineInstance::new(
            [0.0, 0.0],
            [100.0, 100.0],
            [0.5, 0.5, 0.5, 1.0],
            1.0,
        );

        assert_eq!(line.start, [0.0, 0.0]);
        assert_eq!(line.end, [100.0, 100.0]);
        assert_eq!(line.color, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(line.width, 1.0);
    }

    /// 测试 GridViewportUniform 创建
    #[test]
    fn test_grid_viewport_uniform_creation() {
        let viewport = GridViewportUniform::new(1920.0, 1080.0);

        assert_eq!(viewport.viewport_size, [1920.0, 1080.0]);
    }

    /// 测试初始容量配置
    #[test]
    fn test_initial_capacity() {
        // 验证初始容量已增大
        assert!(GridRenderer::INITIAL_CAPACITY >= 8192);
        // 验证扩容因子
        assert_eq!(GridRenderer::GROWTH_FACTOR, 2);
    }
}
