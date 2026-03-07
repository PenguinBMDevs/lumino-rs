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

/// 视口 uniform 数据
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
    // 位置
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x2,
    },
    // 尺寸
    wgpu::VertexAttribute {
        offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x2,
    },
    // 颜色
    wgpu::VertexAttribute {
        offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
];

/// 裁剪 uniform 数据
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CullUniform {
    instance_count: u32,
    _padding: [u32; 3],
}

/// 间接绘制参数
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawIndirectArgs {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
    // 填充以满足 32 字节对齐要求
    _padding: [u32; 4],
}

/// 音符渲染器 - 使用 wgpu 实例化渲染高效绘制大量音符
pub struct NoteRenderer {
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 计算管线 (用于裁剪)
    cull_pipeline: wgpu::ComputePipeline,
    /// 实例缓冲区 (所有实例)
    instance_buffer: wgpu::Buffer,
    /// 可见实例缓冲区 (裁剪后)
    visible_instance_buffer: wgpu::Buffer,
    /// 间接绘制参数缓冲区
    indirect_buffer: wgpu::Buffer,
    /// 当前缓冲区容量（实例数量）
    capacity: usize,
    /// 视口 uniform 缓冲区
    viewport_buffer: wgpu::Buffer,
    /// 裁剪 uniform 缓冲区
    cull_uniform_buffer: wgpu::Buffer,
    /// 渲染 Bind group
    render_bind_group: wgpu::BindGroup,
    /// 计算 Bind group
    cull_bind_group: wgpu::BindGroup,
    /// 计算 Bind group layout
    cull_bind_group_layout: wgpu::BindGroupLayout,
}

impl NoteRenderer {
    /// 初始缓冲区容量
    const INITIAL_CAPACITY: usize = crate::constants::rendering::INITIAL_INSTANCE_CAPACITY;
    /// 顶点着色器代码 (WGSL)
    const VERTEX_SHADER: &'static str = include_str!("shaders/note.wgsl");
    /// 计算着色器代码 (WGSL)
    const CULL_SHADER: &'static str = include_str!("shaders/cull.wgsl");

    /// 创建新的音符渲染器
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("note_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::VERTEX_SHADER.into()),
        });

        let cull_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cull_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::CULL_SHADER.into()),
        });

        // 创建渲染 bind group layout
        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("note_render_bind_group_layout"),
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

        // 创建计算 bind group layout
        let cull_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("note_cull_bind_group_layout"),
                entries: &[
                    // 视口
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // 裁剪信息
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // 全部实例
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // 可见实例
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // 间接参数
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // 创建渲染 pipeline layout
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("note_render_pipeline_layout"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        // 创建计算 pipeline layout
        let cull_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("note_cull_pipeline_layout"),
            bind_group_layouts: &[&cull_bind_group_layout],
            push_constant_ranges: &[],
        });

        // 创建渲染管线
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("note_pipeline"),
            layout: Some(&render_pipeline_layout),
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

        // 创建计算管线
        let cull_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("note_cull_pipeline"),
            layout: Some(&cull_pipeline_layout),
            module: &cull_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // 创建缓冲区
        let instance_buffer = Self::create_instance_buffer(device, Self::INITIAL_CAPACITY, false);
        let visible_instance_buffer =
            Self::create_instance_buffer(device, Self::INITIAL_CAPACITY, true);

        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("note_indirect_buffer"),
            size: std::mem::size_of::<DrawIndirectArgs>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport_uniform"),
            contents: bytemuck::cast_slice(&[ViewportUniform::new(1.0, 1.0)]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let cull_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cull_uniform"),
            contents: bytemuck::cast_slice(&[CullUniform {
                instance_count: 0,
                _padding: [0; 3],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建渲染 bind group
        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("note_render_bind_group"),
            layout: &render_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        // 创建计算 bind group
        let cull_bind_group = Self::create_cull_bind_group(
            device,
            &cull_bind_group_layout,
            &viewport_buffer,
            &cull_uniform_buffer,
            &instance_buffer,
            &visible_instance_buffer,
            &indirect_buffer,
        );

        // render_bind_group_layout 被保留用于未来扩展，但当前未直接使用
        let _ = render_bind_group_layout;

        Self {
            pipeline,
            cull_pipeline,
            instance_buffer,
            visible_instance_buffer,
            indirect_buffer,
            capacity: Self::INITIAL_CAPACITY,
            viewport_buffer,
            cull_uniform_buffer,
            render_bind_group,
            cull_bind_group,
            cull_bind_group_layout,
        }
    }

    fn create_cull_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        cull_uniform_buffer: &wgpu::Buffer,
        instance_buffer: &wgpu::Buffer,
        visible_instance_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("note_cull_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cull_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: visible_instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: indirect_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// 准备绘制（执行 Compute Culling）
    pub fn prepare(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        instances: &[NoteInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport_size: (f32, f32),
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

        // 上传 cull uniform
        let cull_info = CullUniform {
            instance_count: instances.len() as u32,
            _padding: [0; 3],
        };
        queue.write_buffer(
            &self.cull_uniform_buffer,
            0,
            bytemuck::cast_slice(&[cull_info]),
        );

        // 上传实例数据
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));

        // 重置间接绘制参数 (instance_count = 0)
        let indirect_args = DrawIndirectArgs {
            vertex_count: 4,
            instance_count: 0,
            first_vertex: 0,
            first_instance: 0,
            _padding: [0; 4],
        };
        queue.write_buffer(
            &self.indirect_buffer,
            0,
            bytemuck::cast_slice(&[indirect_args]),
        );

        // 执行 Compute Culling
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("note_cull_pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.cull_pipeline);
        compute_pass.set_bind_group(0, &self.cull_bind_group, &[]);

        // 计算工作组数量 (每组 64 个线程)
        let workgroup_count = (instances.len() as u32).div_ceil(64);
        compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
    }

    /// 绘制音符列表（带裁剪）
    pub fn draw<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        if !has_instances {
            return;
        }

        // 设置裁剪矩形（限制绘制区域）
        if let Some((x, y, width, height)) = scissor_rect {
            render_pass.set_scissor_rect(x, y, width, height);
        }

        // 绑定管线并绘制
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.visible_instance_buffer.slice(..));

        // 使用间接绘制
        render_pass.draw_indirect(&self.indirect_buffer, 0);
    }

    /// 创建实例缓冲区
    fn create_instance_buffer(
        device: &wgpu::Device,
        capacity: usize,
        is_storage: bool,
    ) -> wgpu::Buffer {
        let mut usage = wgpu::BufferUsages::COPY_DST;
        if is_storage {
            usage |= wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX;
        } else {
            usage |= wgpu::BufferUsages::STORAGE;
        }

        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(if is_storage {
                "note_visible_instance_buffer"
            } else {
                "note_instance_buffer"
            }),
            size: (capacity * std::mem::size_of::<NoteInstance>()) as wgpu::BufferAddress,
            usage,
            mapped_at_creation: false,
        })
    }

    /// 扩容缓冲区
    fn grow_buffer(&mut self, device: &wgpu::Device, required_capacity: usize) {
        let new_capacity = (self.capacity * 2).max(required_capacity);
        self.instance_buffer = Self::create_instance_buffer(device, new_capacity, false);
        self.visible_instance_buffer = Self::create_instance_buffer(device, new_capacity, true);
        self.capacity = new_capacity;

        // 重新创建 cull bind group
        self.cull_bind_group = Self::create_cull_bind_group(
            device,
            &self.cull_bind_group_layout,
            &self.viewport_buffer,
            &self.cull_uniform_buffer,
            &self.instance_buffer,
            &self.visible_instance_buffer,
            &self.indirect_buffer,
        );
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
