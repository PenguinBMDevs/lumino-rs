//! 弯音编辑模式弯曲音符专用 WGPU 渲染器
//!
//! 使用 `bend_note.wgsl` 着色器，在钢琴卷帘区域渲染弯音模式下被曲线
//! 弯曲的音符段矩形。CPU 端将每个音符按 tick 细分为多个梯形段，
//! 段间采样弯音曲线，实现音符随弯音曲线柔性弯曲的显示效果。

use wgpu::util::DeviceExt;

use crate::gpu_resource_tracker;

/// 弯曲音符段实例 (32 bytes)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BendNoteInstance {
    /// 段起点：[tick_start, y0_top]（逻辑坐标，y 为 key 单位，已含弯音偏移）
    pub position: [f32; 2],
    /// 段终点 tick
    pub end_tick: f32,
    /// 段终点上边（key 单位，含弯音偏移）
    pub y1_top: f32,
    /// RGBA 打包色 0xRRGGBBAA
    pub color_packed: u32,
}

impl BendNoteInstance {
    /// 创建弯曲音符段实例
    ///
    /// `tick_start` / `tick_end`：段范围（tick）
    /// `y0_top` / `y1_top`：段起止上边（key 单位，含弯音偏移）
    #[must_use]
    pub fn new(
        tick_start: f32,
        tick_end: f32,
        y0_top: f32,
        y1_top: f32,
        color_packed: u32,
    ) -> Self {
        Self {
            position: [tick_start, y0_top],
            end_tick: tick_end,
            y1_top,
            color_packed,
        }
    }
}

/// Camera Uniform（与 note.wgsl / pitch_bend.wgsl 的 CameraUniform 一致）
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BendNoteCameraUniform {
    pub scroll: [f32; 2],
    pub zoom: [f32; 2],
    pub viewport_size: [f32; 2],
    pub canvas_offset: [f32; 2],
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub max_key_index: f32,
    pub _padding: f32,
}

/// 弯曲音符渲染器
pub struct BendNoteRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    capacity: usize,
}

impl BendNoteRenderer {
    const SHADER_SRC: &'static str = include_str!("shaders/bend_note.wgsl");
    const INITIAL_CAPACITY: usize = 256;
    const GROWTH_FACTOR: usize = 2;

    /// 创建弯曲音符渲染器（带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的弯曲音符渲染器
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bend_note_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::SHADER_SRC.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bend_note_bind_group_layout"),
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
            label: Some("bend_note_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bend_note_pipeline"),
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
            label: Some("bend_note_camera_uniform"),
            contents: bytemuck::cast_slice(&[BendNoteCameraUniform {
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
            label: Some("bend_note_bind_group"),
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
            label: Some("bend_note_instance_buffer"),
            size: (capacity * std::mem::size_of::<BendNoteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu_resource_tracker::add_buffer(&buffer);
        buffer
    }

    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BendNoteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // end_tick
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32,
                },
                // y1_top
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                // color_packed
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }

    /// 准备渲染数据：上传实例和 camera uniform
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[BendNoteInstance],
        camera: BendNoteCameraUniform,
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

    /// 绘制弯曲音符段
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, instance_count: u32) {
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

impl Drop for BendNoteRenderer {
    fn drop(&mut self) {
        gpu_resource_tracker::sub_buffer(&self.instance_buffer);
        gpu_resource_tracker::sub_buffer(&self.camera_buffer);
    }
}
