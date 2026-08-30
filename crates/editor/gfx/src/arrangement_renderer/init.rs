use super::{
    ArrangementNoteUniform, ArrangementRenderer, ArrangementUniform, INITIAL_CAPACITY,
    INITIAL_LANE_CAPACITY, VERTEX_SHADER,
};
use crate::gpu_resource_tracker::{self, TrackedBuffer};
use crate::pipeline::RenderPipelineBuilder;
use crate::shader::create_shader_module;

/// 走带音符着色器（直接复用钢琴卷帘常驻 GPU 音符缓冲）
const NOTE_VERTEX_SHADER: &str = include_str!("../shaders/arrangement_note.wgsl");

impl ArrangementRenderer {
    /// 创建新的走带渲染器（默认带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的走带渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        // ── 覆盖层管线（屏幕空间实例，每帧重建）──
        let shader = create_shader_module(device, "arrangement_shader", VERTEX_SHADER);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("arrangement_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline = RenderPipelineBuilder::new(device, "arrangement_pipeline", &shader)
            .bind_group(&bind_group_layout)
            .vertex_buffer(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<super::ArrangementNoteInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Uint32x4,
                        offset: 16,
                        shader_location: 1,
                    },
                ],
            })
            .alpha_blended_target(format)
            .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                needs_depth,
            ))
            .build();

        let uniform_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("arrangement_uniform"),
                contents: bytemuck::cast_slice(&[ArrangementUniform::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        let overlay_buffer = gpu_resource_tracker::create_instance_buffer::<
            super::ArrangementNoteInstance,
        >(device, "arrangement_overlay_buffer", INITIAL_CAPACITY);

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("arrangement_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.inner().as_entire_binding(),
            }],
        });

        // ── 音符管线：复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存）──
        let note_shader =
            create_shader_module(device, "arrangement_note_shader", NOTE_VERTEX_SHADER);

        let note_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("arrangement_note_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // lane_index[track]：文档音轨 → 走带泳道序号（只读存储，顶点阶段读取）
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let note_pipeline =
            RenderPipelineBuilder::new(device, "arrangement_note_pipeline", &note_shader)
                .bind_group(&note_bind_group_layout)
                // NoteInstance 布局：start_length(Float32x2) | key_color(Uint32) | border_width(Uint32)
                // 每音符 1 个实例，4 个顶点按 corner 生成（vertex_index）
                .vertex_buffer(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<crate::NoteInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 12,
                            shader_location: 2,
                        },
                    ],
                })
                .alpha_blended_target(format)
                .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                    needs_depth,
                ))
                .build();

        let note_uniform_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("arrangement_note_uniform"),
                contents: bytemuck::cast_slice(&[ArrangementNoteUniform::default()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        let lane_index_buffer = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("arrangement_lane_index_buffer"),
                size: (INITIAL_LANE_CAPACITY * std::mem::size_of::<f32>()) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );

        let note_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("arrangement_note_bind_group"),
            layout: &note_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: note_uniform_buffer.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lane_index_buffer.inner().as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            uniform_buffer,
            overlay_buffer,
            overlay_capacity: INITIAL_CAPACITY,
            overlay_count: 0,
            overlay_back_len: 0,
            bind_group,
            note_pipeline,
            note_uniform_buffer,
            lane_index_buffer,
            lane_index_capacity: INITIAL_LANE_CAPACITY,
            note_bind_group,
            note_source: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("arrangement_note_source_placeholder"),
                size: 16,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            }),
            note_segments: Vec::new(),
        }
    }

    /// 确保 instance buffer 容量足够（旧缓冲由 [`TrackedBuffer`] Drop 自动注销）
    pub(super) fn ensure_capacity(
        instance_buffer: &mut TrackedBuffer,
        capacity: &mut usize,
        device: &wgpu::Device,
        instance_count: usize,
    ) {
        let needed = instance_count.next_power_of_two().max(INITIAL_CAPACITY);
        if needed > *capacity {
            *capacity = needed;
            *instance_buffer = gpu_resource_tracker::create_instance_buffer::<
                super::ArrangementNoteInstance,
            >(device, "arrangement_instance_buffer", needed);
        }
    }

    /// 确保 lane_index 存储缓冲容量足够（按 f32 元素数计）
    pub(super) fn ensure_lane_capacity(
        lane_index_buffer: &mut TrackedBuffer,
        lane_index_capacity: &mut usize,
        device: &wgpu::Device,
        track_count: usize,
    ) {
        let needed = track_count.next_power_of_two().max(INITIAL_LANE_CAPACITY);
        if needed > *lane_index_capacity {
            *lane_index_capacity = needed;
            *lane_index_buffer = TrackedBuffer::new(
                device,
                &wgpu::BufferDescriptor {
                    label: Some("arrangement_lane_index_buffer"),
                    size: (needed * std::mem::size_of::<f32>()) as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            );
        }
    }
}
