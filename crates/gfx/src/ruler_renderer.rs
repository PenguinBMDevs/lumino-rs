//! 时间轴标尺渲染器 - 使用 wgpu 实例化渲染高效绘制标尺
//!
//! 替代 iced Canvas 绘制，解决黑乐谱编辑时的性能瓶颈

use wgpu::util::DeviceExt;

/// 标尺刻度实例数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RulerTickInstance {
    /// 位置 (x, y)
    pub position: [f32; 2],
    /// 大小 (width, height)
    pub size: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
    /// 刻度类型 (0.0 = 小节, 1.0 = 拍, 2.0 = 细分)
    pub tick_type: f32,
    /// 时间值 (tick)
    pub tick_value: f32,
    /// 填充
    pub _padding: [f32; 2],
}

impl RulerTickInstance {
    pub fn new(
        position: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        tick_type: u8,
        tick_value: f32,
    ) -> Self {
        Self {
            position,
            size,
            color,
            tick_type: tick_type as f32,
            tick_value,
            _padding: [0.0; 2],
        }
    }
}

/// 标尺视口 Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RulerViewportUniform {
    /// 视口大小
    pub viewport_size: [f32; 2],
    /// 标尺高度
    pub ruler_height: f32,
    /// 键盘宽度
    pub keyboard_width: f32,
    /// 滚动位置 X
    pub scroll_x: f32,
    /// 缩放 X
    pub zoom_x: f32,
    /// 每小节 tick 数
    pub ticks_per_measure: f32,
    /// 每拍 tick 数
    pub ticks_per_beat: f32,
    /// 填充
    pub _padding: [f32; 2],
}

/// 标尺准备参数
#[derive(Debug, Clone)]
pub struct RulerPrepareParams {
    pub viewport_size: (f32, f32),
    pub ruler_height: f32,
    pub keyboard_width: f32,
    pub scroll_x: f32,
    pub zoom_x: f32,
    pub ticks_per_measure: u32,
    pub ticks_per_beat: u32,
}

impl RulerViewportUniform {
    pub fn from_params(params: &RulerPrepareParams) -> Self {
        Self {
            viewport_size: [params.viewport_size.0, params.viewport_size.1],
            ruler_height: params.ruler_height,
            keyboard_width: params.keyboard_width,
            scroll_x: params.scroll_x,
            zoom_x: params.zoom_x,
            ticks_per_measure: params.ticks_per_measure as f32,
            ticks_per_beat: params.ticks_per_beat as f32,
            _padding: [0.0; 2],
        }
    }
}

/// 标尺渲染器
pub struct RulerRenderer {
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
    /// 小节线颜色
    measure_color: [f32; 4],
    /// 拍线颜色
    beat_color: [f32; 4],
    /// 细分线颜色
    subdivision_color: [f32; 4],
    /// 背景颜色
    background_color: [f32; 4],
    /// 缓存的刻度实例数据（避免每帧重新生成）
    cached_instances: Vec<RulerTickInstance>,
    /// 缓存是否有效
    cache_valid: bool,
    /// 缓存参数：用于判断是否需要重新生成
    cache_scroll_x: f32,
    cache_zoom_x: f32,
    cache_viewport_width: f32,
    cache_keyboard_width: f32,
    cache_ruler_height: f32,
    cache_ticks_per_measure: u32,
    cache_ticks_per_beat: u32,
}

impl RulerRenderer {
    /// 初始缓冲区容量
    const INITIAL_CAPACITY: usize = 4096;
    /// 缓冲区扩容因子
    const GROWTH_FACTOR: usize = 2;
    /// 顶点着色器代码
    const VERTEX_SHADER: &'static str = include_str!("shaders/ruler.wgsl");

    /// 创建新的标尺渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ruler_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::VERTEX_SHADER.into()),
        });

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ruler_bind_group_layout"),
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
            label: Some("ruler_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ruler_pipeline"),
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
            label: Some("ruler_viewport_uniform"),
            contents: bytemuck::cast_slice(&[RulerViewportUniform::from_params(
                &RulerPrepareParams {
                    viewport_size: (800.0, 600.0),
                    ruler_height: 30.0,
                    keyboard_width: 60.0,
                    scroll_x: 0.0,
                    zoom_x: 0.1,
                    ticks_per_measure: 1920,
                    ticks_per_beat: 480,
                },
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ruler_bind_group"),
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
            measure_color: [0.3, 0.3, 0.3, 1.0],
            beat_color: [0.5, 0.5, 0.5, 1.0],
            subdivision_color: [0.7, 0.7, 0.7, 1.0],
            background_color: [0.9, 0.9, 0.9, 1.0],
            cached_instances: Vec::new(),
            cache_valid: false,
            cache_scroll_x: 0.0,
            cache_zoom_x: 0.0,
            cache_viewport_width: 0.0,
            cache_keyboard_width: 0.0,
            cache_ruler_height: 0.0,
            cache_ticks_per_measure: 0,
            cache_ticks_per_beat: 0,
        }
    }

    /// 创建实例缓冲区
    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ruler_instance_buffer"),
            size: (capacity * std::mem::size_of::<RulerTickInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// 实例缓冲区布局
    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RulerTickInstance>() as wgpu::BufferAddress,
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
                // tick_type
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                // tick_value
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }

    /// 设置颜色主题
    pub fn set_colors(
        &mut self,
        measure: [f32; 4],
        beat: [f32; 4],
        subdivision: [f32; 4],
        background: [f32; 4],
    ) {
        self.measure_color = measure;
        self.beat_color = beat;
        self.subdivision_color = subdivision;
        self.background_color = background;
    }

    /// 生成标尺刻度实例
    fn generate_tick_instances(&self, params: &RulerPrepareParams) -> Vec<RulerTickInstance> {
        let mut instances = Vec::new();

        // 计算可见时间范围
        let visible_tick_start = params.scroll_x / params.zoom_x;
        let visible_tick_end = (params.scroll_x + params.viewport_size.0) / params.zoom_x;

        // 小节线
        let measure_start = (visible_tick_start / params.ticks_per_measure as f32).floor() as u32;
        let measure_end = (visible_tick_end / params.ticks_per_measure as f32).ceil() as u32;

        for measure in measure_start..=measure_end {
            let tick = measure as f32 * params.ticks_per_measure as f32;
            let x = params.keyboard_width + tick * params.zoom_x - params.scroll_x;

            if x >= params.keyboard_width && x <= params.viewport_size.0 {
                instances.push(RulerTickInstance::new(
                    [x, 0.0],
                    [2.0, params.ruler_height],
                    self.measure_color,
                    0, // 小节线
                    tick,
                ));
            }
        }

        // 拍线
        let beat_start = (visible_tick_start / params.ticks_per_beat as f32).floor() as u32;
        let beat_end = (visible_tick_end / params.ticks_per_beat as f32).ceil() as u32;

        for beat in beat_start..=beat_end {
            let tick = beat as f32 * params.ticks_per_beat as f32;
            let x = params.keyboard_width + tick * params.zoom_x - params.scroll_x;

            // 跳过小节线位置
            if tick % params.ticks_per_measure as f32 == 0.0 {
                continue;
            }

            if x >= params.keyboard_width && x <= params.viewport_size.0 {
                instances.push(RulerTickInstance::new(
                    [x, params.ruler_height * 0.3],
                    [1.0, params.ruler_height * 0.7],
                    self.beat_color,
                    1, // 拍线
                    tick,
                ));
            }
        }

        instances
    }

    /// 准备渲染数据（带缓存优化）
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &RulerPrepareParams,
    ) {
        puffin::profile_function!();

        let p = params;
        let params_changed = !self.cache_valid
            || self.cache_scroll_x != p.scroll_x
            || self.cache_zoom_x != p.zoom_x
            || self.cache_viewport_width != p.viewport_size.0
            || self.cache_keyboard_width != p.keyboard_width
            || self.cache_ruler_height != p.ruler_height
            || self.cache_ticks_per_measure != p.ticks_per_measure
            || self.cache_ticks_per_beat != p.ticks_per_beat;

        if params_changed {
            self.cached_instances = self.generate_tick_instances(p);
            self.cache_scroll_x = p.scroll_x;
            self.cache_zoom_x = p.zoom_x;
            self.cache_viewport_width = p.viewport_size.0;
            self.cache_keyboard_width = p.keyboard_width;
            self.cache_ruler_height = p.ruler_height;
            self.cache_ticks_per_measure = p.ticks_per_measure;
            self.cache_ticks_per_beat = p.ticks_per_beat;
            self.cache_valid = true;
        }

        let instances = &self.cached_instances;
        let instance_count = instances.len();

        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        let viewport_uniform = RulerViewportUniform::from_params(p);
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
    fn test_ruler_tick_instance_creation() {
        let instance = RulerTickInstance::new(
            [100.0, 0.0],
            [2.0, 30.0],
            [0.3, 0.3, 0.3, 1.0],
            0, // 小节线
            1920.0,
        );

        assert_eq!(instance.position, [100.0, 0.0]);
        assert_eq!(instance.size, [2.0, 30.0]);
        assert_eq!(instance.tick_type, 0.0);
        assert_eq!(instance.tick_value, 1920.0);
    }

    #[test]
    fn test_viewport_uniform_creation() {
        let p = RulerPrepareParams {
            viewport_size: (1920.0, 1080.0),
            ruler_height: 30.0,
            keyboard_width: 60.0,
            scroll_x: 100.0,
            zoom_x: 0.1,
            ticks_per_measure: 1920,
            ticks_per_beat: 480,
        };
        let uniform = RulerViewportUniform::from_params(&p);

        assert_eq!(uniform.viewport_size, [1920.0, 1080.0]);
        assert_eq!(uniform.ruler_height, 30.0);
        assert_eq!(uniform.keyboard_width, 60.0);
        assert_eq!(uniform.scroll_x, 100.0);
        assert_eq!(uniform.zoom_x, 0.1);
        assert_eq!(uniform.ticks_per_measure, 1920.0);
        assert_eq!(uniform.ticks_per_beat, 480.0);
    }
}
