//! 钢琴键盘渲染器 - 使用 wgpu 实例化渲染高效绘制键盘
//!
//! 替代 iced Canvas 绘制，解决黑乐谱编辑时的性能瓶颈

use wgpu::util::DeviceExt;

/// 琴键实例数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct KeyInstance {
    /// 位置 (x, y)
    pub position: [f32; 2],
    /// 大小 (width, height)
    pub size: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
    /// 是否黑键 (0.0 = 白键, 1.0 = 黑键)
    pub is_black: f32,
    /// 键索引
    pub key_index: f32,
    /// 填充
    pub _padding: [f32; 2],
}

impl KeyInstance {
    pub fn new(
        position: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        is_black: bool,
        key_index: u16,
    ) -> Self {
        Self {
            position,
            size,
            color,
            is_black: if is_black { 1.0 } else { 0.0 },
            key_index: key_index as f32,
            _padding: [0.0; 2],
        }
    }
}

/// 键盘视口 Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct KeyboardViewportUniform {
    /// 视口大小
    pub viewport_size: [f32; 2],
    /// 键盘宽度
    pub keyboard_width: f32,
    /// 时间轴高度
    pub ruler_height: f32,
    /// 滚动位置 Y
    pub scroll_y: f32,
    /// 缩放 Y
    pub zoom_y: f32,
    /// 可见键数量
    pub visible_key_count: f32,
    /// 填充
    pub _padding: [f32; 2],
}

impl KeyboardViewportUniform {
    pub fn new(
        viewport_width: f32,
        viewport_height: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_y: f32,
        zoom_y: f32,
        visible_key_count: u16,
    ) -> Self {
        Self {
            viewport_size: [viewport_width, viewport_height],
            keyboard_width,
            ruler_height,
            scroll_y,
            zoom_y,
            visible_key_count: visible_key_count as f32,
            _padding: [0.0; 2],
        }
    }
}

/// 键盘渲染器
pub struct KeyboardRenderer {
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
    /// 白键颜色
    white_key_color: [f32; 4],
    /// 黑键颜色
    black_key_color: [f32; 4],
    /// 选中键颜色
    selected_key_color: [f32; 4],
}

impl KeyboardRenderer {
    /// 初始缓冲区容量（128键钢琴）
    const INITIAL_CAPACITY: usize = 128;
    /// 缓冲区扩容因子
    const GROWTH_FACTOR: usize = 2;
    /// 顶点着色器代码
    const VERTEX_SHADER: &'static str = include_str!("shaders/keyboard.wgsl");

    /// 创建新的键盘渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("keyboard_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::VERTEX_SHADER.into()),
        });

        // 创建 bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("keyboard_bind_group_layout"),
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
            label: Some("keyboard_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("keyboard_pipeline"),
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
            label: Some("keyboard_viewport_uniform"),
            contents: bytemuck::cast_slice(&[KeyboardViewportUniform::new(
                800.0, 600.0, 60.0, 30.0, 0.0, 20.0, 128,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("keyboard_bind_group"),
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
            white_key_color: [1.0, 1.0, 1.0, 1.0],
            black_key_color: [0.2, 0.2, 0.2, 1.0],
            selected_key_color: [0.4, 0.7, 1.0, 1.0],
        }
    }

    /// 创建实例缓冲区
    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("keyboard_instance_buffer"),
            size: (capacity * std::mem::size_of::<KeyInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// 实例缓冲区布局
    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<KeyInstance>() as wgpu::BufferAddress,
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
                // is_black
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                // key_index
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }

    /// 设置颜色主题
    pub fn set_colors(&mut self, white: [f32; 4], black: [f32; 4], selected: [f32; 4]) {
        self.white_key_color = white;
        self.black_key_color = black;
        self.selected_key_color = selected;
    }

    /// 生成琴键实例
    fn generate_key_instances(
        &self,
        visible_key_count: u16,
        keyboard_width: f32,
        zoom_y: f32,
        scroll_y: f32,
        ruler_height: f32,
    ) -> Vec<KeyInstance> {
        let mut instances = Vec::with_capacity(visible_key_count as usize);
        let max_key_index = (visible_key_count.saturating_sub(1)) as f32;

        for i in 0..visible_key_count {
            let key_index = i as isize;
            let world_y = (max_key_index - key_index as f32) * zoom_y;
            let screen_y = world_y - scroll_y + ruler_height;

            // 跳过不在视口内的键
            if screen_y + zoom_y < ruler_height || screen_y > 10000.0 {
                continue;
            }

            let is_black = Self::is_key_dark(key_index);
            let color = if is_black {
                self.black_key_color
            } else {
                self.white_key_color
            };

            // 黑键宽度为白键的 60%
            let key_width = if is_black {
                keyboard_width * 0.6
            } else {
                keyboard_width
            };

            // 黑键水平偏移
            let x_offset = if is_black { keyboard_width * 0.4 } else { 0.0 };

            instances.push(KeyInstance::new(
                [x_offset, screen_y],
                [key_width, zoom_y],
                color,
                is_black,
                i,
            ));
        }

        instances
    }

    /// 判断琴键是否为黑键（12平均律）
    fn is_key_dark(key: isize) -> bool {
        let note_in_octave = key.rem_euclid(12);
        matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
    }

    /// 准备渲染数据
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport_size: (f32, f32),
        keyboard_width: f32,
        ruler_height: f32,
        scroll_y: f32,
        zoom_y: f32,
        visible_key_count: u16,
    ) {
        puffin::profile_function!();
        // 生成琴键实例
        let instances = self.generate_key_instances(
            visible_key_count,
            keyboard_width,
            zoom_y,
            scroll_y,
            ruler_height,
        );

        let instance_count = instances.len();

        // 扩容检查
        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        // 上传实例数据
        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        // 更新视口 uniform
        let viewport_uniform = KeyboardViewportUniform::new(
            viewport_size.0,
            viewport_size.1,
            keyboard_width,
            ruler_height,
            scroll_y,
            zoom_y,
            visible_key_count,
        );
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
    fn test_key_instance_creation() {
        let instance =
            KeyInstance::new([10.0, 20.0], [60.0, 20.0], [1.0, 1.0, 1.0, 1.0], false, 60);

        assert_eq!(instance.position, [10.0, 20.0]);
        assert_eq!(instance.size, [60.0, 20.0]);
        assert_eq!(instance.is_black, 0.0);
        assert_eq!(instance.key_index, 60.0);
    }

    #[test]
    fn test_is_key_dark() {
        // C (0) = 白键
        assert!(!KeyboardRenderer::is_key_dark(0));
        // C# (1) = 黑键
        assert!(KeyboardRenderer::is_key_dark(1));
        // D (2) = 白键
        assert!(!KeyboardRenderer::is_key_dark(2));
        // D# (3) = 黑键
        assert!(KeyboardRenderer::is_key_dark(3));
        // E (4) = 白键
        assert!(!KeyboardRenderer::is_key_dark(4));
        // F (5) = 白键
        assert!(!KeyboardRenderer::is_key_dark(5));
        // F# (6) = 黑键
        assert!(KeyboardRenderer::is_key_dark(6));
    }

    #[test]
    fn test_viewport_uniform_creation() {
        let uniform = KeyboardViewportUniform::new(1920.0, 1080.0, 60.0, 30.0, 100.0, 20.0, 128);

        assert_eq!(uniform.viewport_size, [1920.0, 1080.0]);
        assert_eq!(uniform.keyboard_width, 60.0);
        assert_eq!(uniform.ruler_height, 30.0);
        assert_eq!(uniform.scroll_y, 100.0);
        assert_eq!(uniform.zoom_y, 20.0);
        assert_eq!(uniform.visible_key_count, 128.0);
    }
}
