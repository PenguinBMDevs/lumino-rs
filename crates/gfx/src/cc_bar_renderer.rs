//! CC 控制器柱状图渲染器
//!
//! 使用 GPU 实例化渲染绘制 MIDI CC 事件的垂直柱状条。
//! 与 yinhe 的自动化渲染方式一致：
//! - 每根柱子 2px 宽
//! - 高度 = value / 127 * panel_height
//! - 底部对齐（value 0 = 面板底部，value 127 = 面板顶部）
//! - CPU 计算屏幕坐标，GPU 直接绘制

use wgpu::util::DeviceExt;

/// CC 柱状条实例数据 — 24 bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CcBarInstance {
    /// 位置 (x, y) — 屏幕空间像素坐标，左上角
    pub position: [f32; 2],
    /// 尺寸 (width, height) — 像素
    pub size: [f32; 2],
    /// 颜色 RGBA
    pub color: [f32; 4],
}

impl CcBarInstance {
    /// 创建新的 CC 柱状条实例
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            size: [width, height],
            color,
        }
    }
}

/// 视口 Uniform — 8 bytes
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CcBarViewportUniform {
    /// 视口尺寸 (width, height)
    pub viewport_size: [f32; 2],
}

impl CcBarViewportUniform {
    #[must_use]
    pub const fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            viewport_size: [viewport_width, viewport_height],
        }
    }
}

/// CC 柱状条渲染器
pub struct CcBarRenderer {
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
    /// 缓存的实例数据
    cached_instances: Vec<CcBarInstance>,
    /// 缓存是否有效
    cache_valid: bool,
    /// 缓存参数
    cache_viewport_size: (f32, f32),
    cache_instance_count: usize,
}

impl CcBarRenderer {
    /// 初始缓冲区容量
    const INITIAL_CAPACITY: usize = 4096;
    /// 缓冲区扩容因子
    const GROWTH_FACTOR: usize = 2;
    /// 顶点着色器代码
    const SHADER_SRC: &'static str = include_str!("shaders/cc_bar.wgsl");

    /// 创建新的 CC 柱状条渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cc_bar_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cc_bar_bind_group_layout"),
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
            label: Some("cc_bar_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cc_bar_pipeline"),
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

        // 创建缓冲区
        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cc_bar_viewport_uniform"),
            contents: bytemuck::cast_slice(&[CcBarViewportUniform::new(800.0, 600.0)]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cc_bar_bind_group"),
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

    /// 创建实例缓冲区
    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cc_bar_instance_buffer"),
            size: (capacity * std::mem::size_of::<CcBarInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// 实例缓冲区布局
    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CcBarInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // size
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }

    /// 准备渲染数据
    ///
    /// `instances` — CC 柱状条实例列表（屏幕空间坐标）
    /// `viewport_size` — 视口尺寸（用于 NDC 转换）
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[CcBarInstance],
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

        // 扩容实例缓冲区
        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        // 上传实例数据
        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        // 更新视口 uniform
        let viewport_uniform = CcBarViewportUniform::new(viewport_size.0, viewport_size.1);
        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }

    /// 执行渲染
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
mod tests {
    use super::*;

    #[test]
    fn test_cc_bar_instance_creation() {
        let instance = CcBarInstance::new(
            100.0, // x
            50.0,  // y
            2.0,   // width
            80.0,  // height
            [0.3, 0.7, 0.9, 0.85], // color
        );

        assert_eq!(instance.position, [100.0, 50.0]);
        assert_eq!(instance.size, [2.0, 80.0]);
        assert_eq!(instance.color, [0.3, 0.7, 0.9, 0.85]);
    }

    #[test]
    fn test_viewport_uniform_creation() {
        let uniform = CcBarViewportUniform::new(1920.0, 1080.0);
        assert_eq!(uniform.viewport_size, [1920.0, 1080.0]);
    }
}
