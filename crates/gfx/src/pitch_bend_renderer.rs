//! 弯音编辑模式专用 WGPU 渲染器
//!
//! 使用 `pitch_bend.wgsl` 着色器，在钢琴卷帘区域渲染：
//! - 半透明遮罩矩形
//! - 锚点圆（SDF 抗锯齿）
//! - 曲线连线段（阶梯式）
//! - 基准线
//! - 贝塞尔控制柄（scratch-paint 风格实心圆点，仅选中锚点显示）

use wgpu::util::DeviceExt;

use crate::gpu_resource_tracker;

/// 弯音图元类型
pub const TYPE_MASK: u32 = 0;
pub const TYPE_ANCHOR: u32 = 1;
pub const TYPE_LINE: u32 = 2;
pub const TYPE_BASELINE: u32 = 3;
pub const TYPE_HANDLE: u32 = 4;

/// 弯音渲染实例 (48 bytes，8 个 f32/u32)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PitchBendInstance {
    /// 屏幕空间位置（左上角或起点）
    pub screen_pos: [f32; 2],
    /// 屏幕空间尺寸（宽高）
    pub screen_size: [f32; 2],
    /// RGBA 颜色
    pub color: [f32; 4],
    /// 图元类型（TYPE_MASK / TYPE_ANCHOR / TYPE_LINE / TYPE_BASELINE / TYPE_HANDLE）
    pub prim_type: u32,
    /// 锚点半径（仅 TYPE_ANCHOR 使用）
    pub radius: f32,
    /// 对齐填充
    pub _pad: [f32; 3],
}

impl PitchBendInstance {
    /// 创建遮罩实例
    #[must_use]
    pub fn mask(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self {
            screen_pos: [x, y],
            screen_size: [w, h],
            color,
            prim_type: TYPE_MASK,
            radius: 0.0,
            _pad: [0.0; 3],
        }
    }

    /// 创建锚点实例
    #[must_use]
    pub fn anchor(cx: f32, cy: f32, radius: f32, color: [f32; 4]) -> Self {
        Self {
            screen_pos: [cx - radius, cy - radius],
            screen_size: [radius * 2.0, radius * 2.0],
            color,
            prim_type: TYPE_ANCHOR,
            radius,
            _pad: [0.0; 3],
        }
    }

    /// 创建线段实例
    #[must_use]
    pub fn line(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self {
            screen_pos: [x, y],
            screen_size: [w, h],
            color,
            prim_type: TYPE_LINE,
            radius: 0.0,
            _pad: [0.0; 3],
        }
    }

    /// 创建基准线实例
    #[must_use]
    pub fn baseline(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self {
            screen_pos: [x, y],
            screen_size: [w, h],
            color,
            prim_type: TYPE_BASELINE,
            radius: 0.0,
            _pad: [0.0; 3],
        }
    }

    /// 创建贝塞尔控制柄实例（scratch-paint 风格实心小圆点）
    ///
    /// 与锚点同用 SDF 圆渲染，但半径更小、颜色更亮，绘制在锚点之上。
    #[must_use]
    pub fn handle(cx: f32, cy: f32, radius: f32, color: [f32; 4]) -> Self {
        Self {
            screen_pos: [cx - radius, cy - radius],
            screen_size: [radius * 2.0, radius * 2.0],
            color,
            prim_type: TYPE_HANDLE,
            radius,
            _pad: [0.0; 3],
        }
    }
}

/// Camera Uniform（与 note.wgsl 的 CameraUniform 一致）
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PitchBendCameraUniform {
    pub scroll: [f32; 2],
    pub zoom: [f32; 2],
    pub viewport_size: [f32; 2],
    pub canvas_offset: [f32; 2],
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub max_key_index: f32,
    pub _padding: f32,
}

/// 弯音渲染器
pub struct PitchBendRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    capacity: usize,
}

impl PitchBendRenderer {
    const SHADER_SRC: &'static str = include_str!("shaders/pitch_bend.wgsl");
    const INITIAL_CAPACITY: usize = 256;
    const GROWTH_FACTOR: usize = 2;

    /// 创建弯音渲染器（带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的弯音渲染器
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pitch_bend_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pitch_bend_bind_group_layout"),
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
            label: Some("pitch_bend_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pitch_bend_pipeline"),
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
            depth_stencil: crate::constants::rendering::depth_stencil_state_for(needs_depth),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pitch_bend_camera_uniform"),
            contents: bytemuck::cast_slice(&[PitchBendCameraUniform {
                scroll: [0.0; 2],
                zoom: [1.0; 2],
                viewport_size: [800.0, 600.0],
                canvas_offset: [0.0; 2],
                keyboard_width: 120.0,
                ruler_height: 24.0,
                max_key_index: 127.0,
                _padding: 0.0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        gpu_resource_tracker::add_buffer(&camera_buffer);

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pitch_bend_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            instance_buffer,
            camera_buffer,
            bind_group,
            capacity: Self::INITIAL_CAPACITY,
        }
    }

    fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pitch_bend_instance_buffer"),
            size: (capacity * std::mem::size_of::<PitchBendInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu_resource_tracker::add_buffer(&buffer);
        buffer
    }

    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PitchBendInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // screen_pos
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // screen_size
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
                // prim_type
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint32,
                },
                // radius
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
                // _pad[3]
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }

    /// 准备渲染数据：上传实例和 camera uniform
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[PitchBendInstance],
        camera: PitchBendCameraUniform,
    ) {
        puffin::profile_function!();

        // 扩容
        if instances.len() > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instances.len());
            gpu_resource_tracker::sub_buffer(&self.instance_buffer);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        // 上传实例数据
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        // 上传 camera uniform
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[camera]));
    }

    /// 绘制弯音图元
    ///
    /// `instance_offset`：起始实例索引（用于拆分遮罩与锚点/连线的绘制顺序）
    /// `instance_count`：本次绘制的实例数量
    pub fn draw<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        instance_offset: u32,
        instance_count: u32,
    ) {
        puffin::profile_function!();
        if instance_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        let stride = std::mem::size_of::<PitchBendInstance>() as u64;
        render_pass.set_vertex_buffer(
            0,
            self.instance_buffer
                .slice(instance_offset as u64 * stride..),
        );
        render_pass.draw(0..4, 0..instance_count);
    }
}

impl Drop for PitchBendRenderer {
    fn drop(&mut self) {
        gpu_resource_tracker::sub_buffer(&self.instance_buffer);
        gpu_resource_tracker::sub_buffer(&self.camera_buffer);
    }
}
