//! 洋葱皮 GPU 驱动渲染管线
//!
//! 完全消除 CPU 侧的音符遍历、矩形合并、顶点数据生成。
//! 使用 compute shader 进行可见性剔除，通过间接绘制实现实例化渲染。
//!
//! 数据流:
//!   1. 所有音轨音符平铺为 SoA 布局 → Storage Buffer 常驻 GPU
//!   2. 视口/轨道掩码变化时 → 调度 Compute Shader 剔除（dirty tracking 跳过无变化帧）
//!   3. 剔除结果 → Instance Index Buffer → draw_indexed_indirect
//!
//! 优化要点：
//!   - Bind group 仅在 buffer 重建时重建（非每帧）
//!   - Push constant 传 note_count / indices_capacity（替代 arrayLength）
//!   - GPU 端清零 indirect args（消除 CPU 冗余 write_buffer）
//!   - Dirty tracking 跳过静止帧的 compute dispatch
//!   - Buffer 按需扩容 + 空闲缩容

pub mod types;

pub use types::{
    CameraUniform, DrawIndirectArgs, OnionNote, OnionTrackColors, OnionTrackMask,
    OnionViewportUniform, TrackColor,
};
use wgpu::util::DeviceExt;

/// 洋葱皮 GPU 渲染器
pub struct OnionRenderer {
    // ─── GPU 资源 ──────────────────────────────────────
    /// 音符池 Storage Buffer（所有音轨的音符，SoA 布局）
    note_pool_buffer: wgpu::Buffer,
    /// 实例索引缓冲区（compute shader 输出）
    instance_indices_buffer: wgpu::Buffer,
    /// 间接绘制参数缓冲区
    indirect_buffer: wgpu::Buffer,
    /// 索引缓冲区（单位矩形，6 个顶点）
    index_buffer: wgpu::Buffer,
    /// 视口 uniform buffer
    viewport_buffer: wgpu::Buffer,
    /// 轨道掩码 uniform buffer
    track_mask_buffer: wgpu::Buffer,
    /// 轨道颜色 uniform buffer
    track_color_buffer: wgpu::Buffer,
    /// 相机 uniform buffer（复用 CameraUniform）
    camera_buffer: wgpu::Buffer,

    // ─── Pipeline ──────────────────────────────────────
    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    compute_bind_group: wgpu::BindGroup,
    render_bind_group: wgpu::BindGroup,
    compute_bind_group_layout: wgpu::BindGroupLayout,
    render_bind_group_layout: wgpu::BindGroupLayout,

    // ─── 状态 ──────────────────────────────────────────
    /// 音符池容量（OnionNote 数量）
    note_pool_capacity: usize,
    /// 实际音符数量
    note_count: usize,
    /// 实例索引缓冲区容量
    indices_capacity: usize,
    /// GPU 最大 storage buffer binding size
    max_storage_binding: u64,
    /// Bind group 是否需要重建（buffer 被重建时置 true）
    bind_groups_dirty: bool,
    /// 上一次 cull 的视口数据（用于 dirty tracking）
    last_viewport: Option<OnionViewportUniform>,
    /// 上一次 cull 的相机数据（用于 dirty tracking）
    last_camera: Option<CameraUniform>,
    /// 上一次 cull 的轨道掩码（用于 dirty tracking）
    last_track_mask: Option<OnionTrackMask>,
    /// 音符数据是否在上次 cull 后变化过
    notes_dirty: bool,
}

impl OnionRenderer {
    const INITIAL_NOTE_CAPACITY: usize = 8192;
    const INITIAL_INDICES_CAPACITY: usize = 65536;
    const INDICES_SHRINK_THRESHOLD: f64 = 0.25;
    const MAX_INDICES_CAPACITY: usize = 33_554_432;
    const WORKGROUP_SIZE: u32 = 256;

    const VERTEX_SHADER_SRC: &'static str =
        include_str!("shaders/onion_render.wgsl");
    const COMPUTE_SHADER_SRC: &'static str =
        include_str!("shaders/onion_cull.wgsl");

    /// 创建新的洋葱皮渲染器
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let vertex_shader_src = Self::VERTEX_SHADER_SRC;
        let compute_shader_src = Self::COMPUTE_SHADER_SRC;

        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_vertex_shader"),
            source: wgpu::ShaderSource::Wgsl(vertex_shader_src.into()),
        });
        let compute_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_compute_shader"),
            source: wgpu::ShaderSource::Wgsl(compute_shader_src.into()),
        });

        let max_storage_binding = device.limits().max_storage_buffer_binding_size as u64;
        let max_buffer_size = device.limits().max_buffer_size;
        let max_note_pool_bytes = max_storage_binding
            .min(max_buffer_size)
            .min(1_600_000_000) as usize;
        let note_pool_capacity =
            (max_note_pool_bytes / std::mem::size_of::<OnionNote>()).min(Self::INITIAL_NOTE_CAPACITY);

        // ─── Compute bind group layout ──────────────────
        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("onion_compute_bind_group_layout"),
                entries: &[
                    // binding 0: viewport uniform
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
                    // binding 1: track mask uniform
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
                    // binding 2: note pool storage (read)
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
                    // binding 3: instance indices storage (read_write)
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
                    // binding 4: indirect args storage (read_write)
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

        // ─── Render bind group layout ───────────────────
        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("onion_render_bind_group_layout"),
                entries: &[
                    // binding 0: camera uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 1: track colors uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 2: instance indices storage (read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 3: note pool storage (read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
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

        // ─── Pipeline layouts ───────────────────────────
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_compute_pipeline_layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_render_pipeline_layout"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        // ─── Compute pipeline ───────────────────────────
        let compute_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("onion_compute_pipeline"),
                layout: Some(&compute_pipeline_layout),
                module: &compute_module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        // ─── Render pipeline ────────────────────────────
        let render_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("onion_render_pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vertex_module,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &vertex_module,
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
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // ─── Buffers ────────────────────────────────────
        let note_pool_buffer = Self::create_note_pool_buffer(device, note_pool_capacity);
        let indices_capacity = Self::INITIAL_INDICES_CAPACITY;
        let instance_indices_buffer =
            Self::create_instance_indices_buffer(device, indices_capacity);
        let index_buffer = Self::create_quad_index_buffer(device);
        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_indirect_buffer"),
            size: std::mem::size_of::<DrawIndirectArgs>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_viewport_uniform"),
            contents: bytemuck::cast_slice(&[OnionViewportUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let track_mask_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_track_mask_uniform"),
            contents: bytemuck::cast_slice(&[OnionTrackMask::all()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let track_color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_track_color_uniform"),
            contents: bytemuck::cast_slice(&[OnionTrackColors::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_camera_uniform"),
            contents: bytemuck::cast_slice(&[CameraUniform::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ─── Bind groups ────────────────────────────────
        let compute_bind_group = Self::create_compute_bind_group(
            device,
            &compute_bind_group_layout,
            &viewport_buffer,
            &track_mask_buffer,
            &note_pool_buffer,
            &instance_indices_buffer,
            &indirect_buffer,
        );
        let render_bind_group = Self::create_render_bind_group(
            device,
            &render_bind_group_layout,
            &camera_buffer,
            &track_color_buffer,
            &instance_indices_buffer,
            &note_pool_buffer,
        );

        Self {
            note_pool_buffer,
            instance_indices_buffer,
            indirect_buffer,
            index_buffer,
            viewport_buffer,
            track_mask_buffer,
            track_color_buffer,
            camera_buffer,
            render_pipeline,
            compute_pipeline,
            compute_bind_group,
            render_bind_group,
            compute_bind_group_layout,
            render_bind_group_layout,
            note_pool_capacity,
            note_count: 0,
            indices_capacity,
            max_storage_binding,
            bind_groups_dirty: false,
            last_viewport: None,
            last_camera: None,
            last_track_mask: None,
            notes_dirty: false,
        }
    }

    // ─── Buffer helpers ─────────────────────────────────

    fn create_note_pool_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let size = (capacity * std::mem::size_of::<OnionNote>()) as wgpu::BufferAddress;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_note_pool"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_instance_indices_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let size = (capacity * std::mem::size_of::<u32>()) as wgpu::BufferAddress;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("onion_instance_indices"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        })
    }

    fn create_quad_index_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("onion_quad_index_buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn create_compute_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        track_mask_buffer: &wgpu::Buffer,
        note_pool_buffer: &wgpu::Buffer,
        instance_indices_buffer: &wgpu::Buffer,
        indirect_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onion_compute_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: track_mask_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: note_pool_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: instance_indices_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: indirect_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn create_render_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        camera_buffer: &wgpu::Buffer,
        track_color_buffer: &wgpu::Buffer,
        instance_indices_buffer: &wgpu::Buffer,
        note_pool_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onion_render_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: track_color_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: instance_indices_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: note_pool_buffer.as_entire_binding(),
                },
            ],
        })
    }

    // ─── 公共 API ───────────────────────────────────────

    /// 上传所有洋葱皮音符到 GPU
    ///
    /// 替换整个音符池内容。传入所有需要显示的其它音轨的音符。
    pub fn upload_notes(&mut self, notes: &[OnionNote], device: &wgpu::Device, queue: &wgpu::Queue) {
        let count = notes.len();
        if count == 0 {
            self.note_count = 0;
            self.notes_dirty = true;
            return;
        }

        let mut buffer_rebuilt = false;

        // 按需扩容 note_pool_buffer
        let required = count.next_power_of_two().max(Self::INITIAL_NOTE_CAPACITY);
        if required > self.note_pool_capacity {
            let max_capacity = (self.max_storage_binding as usize
                / std::mem::size_of::<OnionNote>())
                .min(100_000_000);
            let new_capacity = required.min(max_capacity);
            if new_capacity > self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(device, new_capacity);
                self.note_pool_capacity = new_capacity;
                buffer_rebuilt = true;
                tracing::info!(
                    "OnionRenderer: note pool grown to {} ({} MB)",
                    new_capacity,
                    (new_capacity * std::mem::size_of::<OnionNote>()) / (1024 * 1024)
                );
            }
        }

        // 空闲缩容：当实际使用量远低于容量时释放内存
        let shrink_threshold = (self.note_pool_capacity as f64 * Self::INDICES_SHRINK_THRESHOLD) as usize;
        if count < shrink_threshold && self.note_pool_capacity > Self::INITIAL_NOTE_CAPACITY * 2 {
            let new_capacity = count.next_power_of_two().max(Self::INITIAL_NOTE_CAPACITY);
            if new_capacity < self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(device, new_capacity);
                self.note_pool_capacity = new_capacity;
                buffer_rebuilt = true;
                tracing::info!(
                    "OnionRenderer: note pool shrunk to {} ({} MB)",
                    new_capacity,
                    (new_capacity * std::mem::size_of::<OnionNote>()) / (1024 * 1024)
                );
            }
        }

        // 按需扩容 instance_indices_buffer（可见音符可能接近总数）
        let required_indices = count;
        if required_indices > self.indices_capacity {
            let new_indices_cap = required_indices.next_power_of_two().min(Self::MAX_INDICES_CAPACITY);
            if new_indices_cap > self.indices_capacity {
                self.instance_indices_buffer =
                    Self::create_instance_indices_buffer(device, new_indices_cap);
                self.indices_capacity = new_indices_cap;
                buffer_rebuilt = true;
                tracing::info!(
                    "OnionRenderer: indices buffer grown to {} ({} MB)",
                    new_indices_cap,
                    (new_indices_cap * std::mem::size_of::<u32>()) / (1024 * 1024)
                );
            }
        }

        let upload_count = count.min(self.note_pool_capacity);
        self.note_count = upload_count;
        self.notes_dirty = true;

        queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&notes[..upload_count]),
        );

        // 仅在 buffer 真正被重建时才 rebuild bind group
        if buffer_rebuilt {
            self.rebuild_bind_groups(device);
        }
    }

    /// 上传轨道颜色表
    pub fn upload_track_colors(&self, colors: &OnionTrackColors, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.track_color_buffer,
            0,
            bytemuck::cast_slice(&[*colors]),
        );
    }

    /// 设置轨道掩码
    pub fn upload_track_mask(&mut self, mask: &OnionTrackMask, queue: &wgpu::Queue) {
        if Some(mask) != self.last_track_mask.as_ref() {
            self.last_track_mask = Some(*mask);
        }
        queue.write_buffer(
            &self.track_mask_buffer,
            0,
            bytemuck::cast_slice(&[*mask]),
        );
    }

    /// 准备计算剔除（视口或轨道掩码变化时调用）
    ///
    /// 执行 compute shader 剔除，结果写入 instance_indices_buffer 和 indirect_buffer。
    /// 内置 dirty tracking：当视口/相机/轨道掩码/音符均未变化时跳过 compute dispatch。
    pub fn prepare_cull(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &OnionViewportUniform,
        camera: &CameraUniform,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
    ) {
        if self.note_count == 0 {
            return;
        }

        // Dirty check：检测视口/相机/轨道掩码/音符是否有变化
        let viewport_changed = self.last_viewport.as_ref() != Some(viewport);
        let camera_changed = self.last_camera.as_ref() != Some(camera);
        let anything_dirty = viewport_changed || camera_changed || self.notes_dirty;

        if !anything_dirty {
            return;
        }

        // 更新缓存状态
        self.last_viewport = Some(*viewport);
        self.last_camera = Some(*camera);
        self.notes_dirty = false;

        // 构建完整的 viewport uniform（合并 cull 参数：note_count + indices_capacity）
        let full_viewport = OnionViewportUniform {
            tick_start: viewport.tick_start,
            tick_end: viewport.tick_end,
            pitch_min: viewport.pitch_min,
            pitch_max: viewport.pitch_max,
            note_count: self.note_count as u32,
            indices_capacity: self.indices_capacity as u32,
            _padding: [0; 2],
        };

        // 上传视口 uniform（含 cull 参数，仅在变化时上传）
        if viewport_changed || self.notes_dirty {
            queue.write_buffer(
                &self.viewport_buffer,
                0,
                bytemuck::cast_slice(&[full_viewport]),
            );
        }
        // 上传相机 uniform（仅在变化时上传）
        if camera_changed {
            queue.write_buffer(
                &self.camera_buffer,
                0,
                bytemuck::cast_slice(&[*camera]),
            );
        }

        // 如果 bind group 因 buffer 重建而脏了，先修复
        if self.bind_groups_dirty {
            self.rebuild_bind_groups(device);
            self.bind_groups_dirty = false;
        }

        // 执行 Compute Culling
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("onion_cull_pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);

        let workgroup_count = (self.note_count as u32).div_ceil(Self::WORKGROUP_SIZE);
        compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
    }

    /// 执行间接绘制（TriangleStrip + 4 顶点/实例，与 note.wgsl 一致）
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        if self.note_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        render_pass.draw_indirect(&self.indirect_buffer, 0);
    }

    /// 获取当前音符数量
    pub fn note_count(&self) -> usize {
        self.note_count
    }

    /// 获取音符池容量
    pub fn note_pool_capacity(&self) -> usize {
        self.note_pool_capacity
    }

    /// 获取 GPU 内存占用（字节）
    pub fn gpu_memory_usage(&self) -> u64 {
        self.note_pool_buffer.size()
            + self.instance_indices_buffer.size()
            + self.indirect_buffer.size()
            + self.index_buffer.size()
            + self.viewport_buffer.size()
            + self.track_mask_buffer.size()
            + self.track_color_buffer.size()
            + self.camera_buffer.size()
    }

    // ─── 内部方法 ───────────────────────────────────────

    fn rebuild_bind_groups(&mut self, device: &wgpu::Device) {
        self.compute_bind_group = Self::create_compute_bind_group(
            device,
            &self.compute_bind_group_layout,
            &self.viewport_buffer,
            &self.track_mask_buffer,
            &self.note_pool_buffer,
            &self.instance_indices_buffer,
            &self.indirect_buffer,
        );
        self.render_bind_group = Self::create_render_bind_group(
            device,
            &self.render_bind_group_layout,
            &self.camera_buffer,
            &self.track_color_buffer,
            &self.instance_indices_buffer,
            &self.note_pool_buffer,
        );
    }
}

/// 将 OnionSkinColors（来自 ui crate）转换为 OnionTrackColors
/// 由 UI 层调用，传入数组 [r, g, b, a] 颜色值
pub fn convert_onion_colors(
    colors: &[(f32, f32, f32, f32)],
) -> OnionTrackColors {
    let mut track_colors = OnionTrackColors::default();
    for (i, &(r, g, b, a)) in colors.iter().enumerate() {
        if i >= 64 {
            break;
        }
        track_colors.colors[i] = TrackColor::from_rgba(r, g, b, a);
    }
    track_colors
}
