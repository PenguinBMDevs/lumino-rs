use wgpu::util::DeviceExt;

pub mod types;

pub use types::{CameraUniform, CullUniform, DrawIndirectArgs, NoteInstance, RenderUniform, VERTEX_ATTRIBUTES};

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
    /// 最大缓冲区容量（受 GPU max_storage_buffer_binding_size 限制）
    max_capacity: usize,
    /// 上次实际上传的实例数量（用于 prepare_pass 调度 compute）
    last_upload_count: u32,
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
                    // viewport
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
                    // cull_info
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
                    // all_instances
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
                    // visible_instances
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
                    // indirect_args
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
        let max_capacity = (device.limits().max_storage_buffer_binding_size as usize)
            / std::mem::size_of::<NoteInstance>();

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
            contents: bytemuck::cast_slice(&[CameraUniform::default()]),
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

        // 创建计算 bind group（初始时无实例数据，使用0作为计数）
        let cull_bind_group = Self::create_cull_bind_group(
            device,
            &cull_bind_group_layout,
            &viewport_buffer,
            &cull_uniform_buffer,
            &instance_buffer,
            &visible_instance_buffer,
            &indirect_buffer,
            0,
        );

        Self {
            pipeline,
            cull_pipeline,
            instance_buffer,
            visible_instance_buffer,
            indirect_buffer,
            capacity: Self::INITIAL_CAPACITY,
            max_capacity,
            last_upload_count: 0,
            viewport_buffer,
            cull_uniform_buffer,
            render_bind_group,
            cull_bind_group,
            cull_bind_group_layout,
        }
    }

    /// 兼容方法：数据+camera一步准备好（内部仍拆分成两步）
    pub fn prepare(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        instances: &[NoteInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: CameraUniform,
    ) {
        self.prepare_instances(encoder, instances, device, queue);
        self.prepare_pass(encoder, camera, queue);
    }

    /// 仅在音符数据真正变化时调用：负责 buffer 扩容 + instance upload
    pub fn prepare_instances(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        instances: &[NoteInstance],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if instances.is_empty() {
            self.last_upload_count = 0;
            return;
        }

        // 检查是否需要扩容
        if instances.len() > self.capacity {
            self.grow_buffer(device, instances.len());
        }

        let upload_count = instances.len().min(self.capacity);
        if upload_count < instances.len() {
            tracing::warn!(
                "NoteRenderer: {} instances exceed max_capacity ({}), truncating",
                instances.len(),
                self.max_capacity
            );
        }

        self.last_upload_count = upload_count as u32;

        // 上传 cull uniform
        let cull_info = CullUniform {
            instance_count: self.last_upload_count,
            _padding: [0; 3],
        };
        queue.write_buffer(
            &self.cull_uniform_buffer,
            0,
            bytemuck::cast_slice(&[cull_info]),
        );

        // 上传实例数据
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..upload_count]),
        );

        // 更新 bind group 以反映新的数据范围（如果缓冲区没有扩容，需要更新绑定范围）
        // 注意：如果 grow_buffer 被调用，它已经在内部更新了 bind group
        // 这里只在未扩容时更新
        if upload_count <= self.capacity && self.capacity == self.instance_buffer.size() as usize / std::mem::size_of::<NoteInstance>() {
            self.cull_bind_group = Self::create_cull_bind_group(
                device,
                &self.cull_bind_group_layout,
                &self.viewport_buffer,
                &self.cull_uniform_buffer,
                &self.instance_buffer,
                &self.visible_instance_buffer,
                &self.indirect_buffer,
                upload_count,
            );
        }
    }

    /// 滚动/缩放等视口变化时调用：只更新 camera 并重跑 compute cull
    pub fn prepare_pass(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        camera: CameraUniform,
        queue: &wgpu::Queue,
    ) {
        if self.last_upload_count == 0 {
            return;
        }

        // 上传 viewport uniform
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[camera]));

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
        // Vulkan 限制单维度 dispatch 最大 65535，因此拆成 2D
        const MAX_DISPATCH_X: u32 = 65535;
        let workgroup_count = self.last_upload_count.div_ceil(64);
        let dispatch_x = workgroup_count.min(MAX_DISPATCH_X);
        let dispatch_y = workgroup_count.div_ceil(MAX_DISPATCH_X);
        compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
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

    /// 扩容缓冲区（受 max_capacity 限制）
    fn grow_buffer(&mut self, device: &wgpu::Device, required_capacity: usize) {
        let growth_factor = crate::constants::rendering::BUFFER_GROWTH_FACTOR;
        let new_capacity = ((self.capacity.saturating_mul(growth_factor))
            .max(required_capacity))
        .min(self.max_capacity);
        if new_capacity <= self.capacity {
            return;
        }

        tracing::debug!(
            "Growing note buffer: {} -> {} (required: {})",
            self.capacity,
            new_capacity,
            required_capacity
        );

        self.instance_buffer = Self::create_instance_buffer(device, new_capacity, false);
        self.visible_instance_buffer = Self::create_instance_buffer(device, new_capacity, true);
        self.capacity = new_capacity;

        // 重新创建 cull bind group（扩容后使用当前上传的实例数）
        self.cull_bind_group = Self::create_cull_bind_group(
            device,
            &self.cull_bind_group_layout,
            &self.viewport_buffer,
            &self.cull_uniform_buffer,
            &self.instance_buffer,
            &self.visible_instance_buffer,
            &self.indirect_buffer,
            self.last_upload_count as usize,
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

    /// 创建计算 bind group（带实际数据大小限制）
    fn create_cull_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        cull_uniform_buffer: &wgpu::Buffer,
        instance_buffer: &wgpu::Buffer,
        visible_instance_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
        instance_count: usize,
    ) -> wgpu::BindGroup {
        let instance_size = std::mem::size_of::<NoteInstance>() as u64;
        let actual_data_size = (instance_count as u64) * instance_size;
        let buffer_size = instance_buffer.size();

        // 限制绑定范围到实际数据大小，避免GPU预取超出范围
        // SAFETY: actual_data_size > 0 已经检查过，所以 NonZeroU64::new 不会返回 None
        let instance_binding = if let Some(size) = std::num::NonZeroU64::new(actual_data_size) {
            if actual_data_size < buffer_size {
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: instance_buffer,
                    offset: 0,
                    size: Some(size),
                })
            } else {
                instance_buffer.as_entire_binding()
            }
        } else {
            instance_buffer.as_entire_binding()
        };

        let visible_binding = if let Some(size) = std::num::NonZeroU64::new(actual_data_size) {
            if actual_data_size < buffer_size {
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: visible_instance_buffer,
                    offset: 0,
                    size: Some(size),
                })
            } else {
                visible_instance_buffer.as_entire_binding()
            }
        } else {
            visible_instance_buffer.as_entire_binding()
        };

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
                    resource: instance_binding,
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: visible_binding,
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: indirect_buffer.as_entire_binding(),
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 NoteInstance 创建和属性访问
    #[test]
    fn test_note_instance_creation() {
        let instance = NoteInstance {
            position: [100.0, 60.0],
            size: [200.0, 20.0],
            color: [1.0, 0.5, 0.0, 0.8],
        };

        assert_eq!(instance.position, [100.0, 60.0]);
        assert_eq!(instance.size, [200.0, 20.0]);
        assert_eq!(instance.color, [1.0, 0.5, 0.0, 0.8]);
    }

    /// 测试 CameraUniform 默认值
    #[test]
    fn test_camera_uniform_default() {
        let camera = CameraUniform {
            scroll: [0.0, 0.0],
            zoom: [1.0, 20.0],
            viewport_size: [800.0, 600.0],
            canvas_offset: [0.0, 0.0],
            keyboard_width: 60.0,
            ruler_height: 30.0,
            max_key_index: 127.0,
            _padding: 0.0,
        };

        assert_eq!(camera.scroll, [0.0, 0.0]);
        assert_eq!(camera.zoom, [1.0, 20.0]);
        assert_eq!(camera.viewport_size, [800.0, 600.0]);
    }

    /// 测试 CullUniform 创建
    #[test]
    fn test_cull_uniform_creation() {
        let cull = CullUniform {
            instance_count: 1000,
            _padding: [0; 3],
        };

        assert_eq!(cull.instance_count, 1000);
    }

    /// 测试常量配置
    #[test]
    fn test_constants() {
        use crate::constants::rendering;

        // 验证初始容量已增大
        assert!(rendering::INITIAL_INSTANCE_CAPACITY >= 65536);
        // 验证扩容因子
        assert_eq!(rendering::BUFFER_GROWTH_FACTOR, 2);
    }

    /// 测试 DrawIndirectArgs 默认值
    #[test]
    fn test_draw_indirect_args_default() {
        let args = DrawIndirectArgs::default();

        assert_eq!(args.vertex_count, 4);
        assert_eq!(args.instance_count, 0);
        assert_eq!(args.first_vertex, 0);
        assert_eq!(args.first_instance, 0);
    }
}
