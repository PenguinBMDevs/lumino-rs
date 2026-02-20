use wgpu::util::DeviceExt;

/// 音符实例数据 - 每个音符对应一个实例
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NoteInstance {
    /// 左上角位置 (x, y)
    pub position: [f32; 2],
    /// 尺寸 (width, height)
    pub size: [f32; 2],
    /// 颜色 (r, g, b, a)
    pub color: [f32; 4],
}

impl NoteInstance {
    /// 创建新的音符实例
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            size: [width, height],
            color,
        }
    }
}

/// Viewport  uniform 数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportUniform {
    size: [f32; 2],
    _padding: [f32; 2],
}

impl ViewportUniform {
    fn new(width: f32, height: f32) -> Self {
        Self {
            size: [width, height],
            _padding: [0.0, 0.0],
        }
    }
}

/// 顶点属性布局（静态常量）
const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    // position
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x2,
    },
    // size
    wgpu::VertexAttribute {
        offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x2,
    },
    // color
    wgpu::VertexAttribute {
        offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
];

/// 音符渲染器 - 使用 wgpu 实例化渲染高效绘制大量音符
pub struct NoteRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 实例缓冲区 (动态更新)
    instance_buffer: wgpu::Buffer,
    /// 当前缓冲区容量（实例数量）
    capacity: usize,
    /// Viewport uniform 缓冲区
    viewport_buffer: wgpu::Buffer,
    /// Bind group
    bind_group: wgpu::BindGroup,
    /// Bind group layout
    bind_group_layout: wgpu::BindGroupLayout,
}

impl NoteRenderer {
    /// 初始缓冲区容量
    const INITIAL_CAPACITY: usize = 1024;
    /// 顶点着色器代码 (WGSL)
    const VERTEX_SHADER: &'static str = include_str!("shaders/note.wgsl");

    /// 创建新的音符渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("note_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::VERTEX_SHADER.into()),
        });

        // 创建 bind group layout（用于 viewport uniform）
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("note_bind_group_layout"),
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
            label: Some("note_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("note_pipeline"),
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

        // 创建实例缓冲区
        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        // 创建 viewport uniform 缓冲区（初始值为 0，会在第一次 draw 时更新）
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport_uniform"),
            contents: bytemuck::cast_slice(&[ViewportUniform::new(1.0, 1.0)]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建 bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("note_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            instance_buffer,
            capacity: Self::INITIAL_CAPACITY,
            viewport_buffer,
            bind_group,
            bind_group_layout,
        }
    }

    /// 绘制音符列表（带裁剪）
    /// 
    /// # 参数
    /// - `render_pass`: 活跃的渲染通道
    /// - `instances`: 要绘制的音符实例列表
    /// - `device`: wgpu 设备（用于缓冲区扩容）
    /// - `queue`: wgpu 队列（用于上传数据）
    /// - `viewport_size`: 视口尺寸 (width, height)
    /// - `scissor_rect`: 裁剪矩形 (x, y, width, height)，像素坐标
    pub fn draw<'r>(
        &'r mut self,
        render_pass: &mut wgpu::RenderPass<'r>,
        instances: &[NoteInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport_size: (f32, f32),
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        if instances.is_empty() {
            return;
        }

        // 检查是否需要扩容
        if instances.len() > self.capacity {
            self.grow_buffer(device, instances.len());
        }

        // 上传 viewport uniform
        let viewport = ViewportUniform::new(viewport_size.0, viewport_size.1);
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[viewport]));

        // 上传实例数据
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(instances),
        );

        // 设置裁剪矩形（限制绘制区域）
        if let Some((x, y, width, height)) = scissor_rect {
            render_pass.set_scissor_rect(x, y, width, height);
        }

        // 绑定管线并绘制
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        
        // 每个实例渲染一个三角形带（4个顶点组成矩形）
        render_pass.draw(0..4, 0..instances.len() as u32);
    }

    /// 创建实例缓冲区
    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("note_instance_buffer"),
            size: (capacity * std::mem::size_of::<NoteInstance>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// 扩容缓冲区
    fn grow_buffer(&mut self, device: &wgpu::Device, required_capacity: usize) {
        let new_capacity = (self.capacity * 2).max(required_capacity);
        self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
        self.capacity = new_capacity;
    }

    /// 实例缓冲区布局描述
    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<NoteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &VERTEX_ATTRIBUTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_instance_size() {
        // 确保结构体大小符合预期: 2*4 + 2*4 + 4*4 = 32 bytes
        assert_eq!(std::mem::size_of::<NoteInstance>(), 32);
    }
}
