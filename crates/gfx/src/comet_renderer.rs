//! Comet 风格 GPU 渲染器
//!
//! 为视频导出提供全 GPU 的 Comet 渲染样式：
//! Enhanced、MIDITrail、PFA、Velocities、Channels。
//! 所有样式均使用计算着色器直接写入 storage texture，再由 ExportPipeline 读回 CPU。

mod shader;
mod types;

pub use types::{CometNoteGpu, CometRenderStyle, CometUniformGpu};

/// Comet GPU 渲染器
pub struct CometRenderer {
    /// 每个样式一条 compute pipeline
    pipelines: [wgpu::ComputePipeline; 5],
    /// 共享 bind group layout
    bind_group_layout: wgpu::BindGroupLayout,
    /// 共享 uniform buffer
    uniform_buffer: wgpu::Buffer,
    /// 音符 storage buffer（按需增长）
    note_buffer: Option<wgpu::Buffer>,
    /// 活跃键颜色 storage buffer（128 u32）
    active_keys_buffer: Option<wgpu::Buffer>,
    /// 输出 storage texture
    output_texture: Option<wgpu::Texture>,
    output_texture_view: Option<wgpu::TextureView>,

    note_capacity: usize,
    current_width: u32,
    current_height: u32,
}

impl CometRenderer {
    const INITIAL_NOTE_CAPACITY: usize = 4096;

    /// 创建渲染器，同时编译 5 个样式的 compute pipeline。
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = Self::create_bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("comet_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let mut pipelines = Vec::with_capacity(5);
        for style in [
            CometRenderStyle::Enhanced,
            CometRenderStyle::MIDITrail,
            CometRenderStyle::PFA,
            CometRenderStyle::Velocities,
            CometRenderStyle::Channels,
        ] {
            let source = shader::source_for_style(style);
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("comet_shader_{:?}", style)),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&format!("comet_pipeline_{:?}", style)),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(style.entry_point()),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
            pipelines.push(pipeline);
        }

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("comet_uniform_buffer"),
            size: std::mem::size_of::<CometUniformGpu>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        crate::gpu_resource_tracker::add_buffer(&uniform_buffer);

        Self {
            pipelines: pipelines.try_into().expect("恰好 5 个 pipeline"),
            bind_group_layout,
            uniform_buffer,
            note_buffer: None,
            active_keys_buffer: None,
            output_texture: None,
            output_texture_view: None,
            note_capacity: 0,
            current_width: 0,
            current_height: 0,
        }
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("comet_bind_group_layout"),
            entries: &[
                // binding 0: uniform
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
                // binding 1: notes storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 2: active keys storage buffer
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
                // binding 3: output storage texture
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        })
    }

    /// 确保输出纹理存在。
    fn ensure_output_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.current_width == width
            && self.current_height == height
            && self.output_texture.is_some()
        {
            return;
        }

        if let Some(tex) = self.output_texture.take() {
            crate::gpu_resource_tracker::sub_texture(&tex);
        }
        self.output_texture_view.take();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("comet_output_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        crate::gpu_resource_tracker::add_texture(&texture);

        self.output_texture_view =
            Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.output_texture = Some(texture);
        self.current_width = width;
        self.current_height = height;
    }

    /// 确保 note buffer 有足够容量。
    fn ensure_note_buffer(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.note_capacity {
            return;
        }
        let new_cap = count.next_power_of_two().max(Self::INITIAL_NOTE_CAPACITY);
        let size = (new_cap * std::mem::size_of::<CometNoteGpu>()) as u64;

        if let Some(buf) = self.note_buffer.take() {
            crate::gpu_resource_tracker::sub_buffer(&buf);
        }

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("comet_note_buffer"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        crate::gpu_resource_tracker::add_buffer(&buffer);
        self.note_buffer = Some(buffer);
        self.note_capacity = new_cap;
    }

    /// 确保活跃键颜色 buffer 存在。
    fn ensure_active_keys_buffer(&mut self, device: &wgpu::Device) {
        if self.active_keys_buffer.is_some() {
            return;
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("comet_active_keys_buffer"),
            size: (128 * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        crate::gpu_resource_tracker::add_buffer(&buffer);
        self.active_keys_buffer = Some(buffer);
    }

    /// 重建 bind group。
    fn rebuild_bind_group(&mut self, device: &wgpu::Device) -> wgpu::BindGroup {
        let note_buf = self
            .note_buffer
            .as_ref()
            .expect("comet note_buffer 未初始化");
        let active_keys_buf = self
            .active_keys_buffer
            .as_ref()
            .expect("comet active_keys_buffer 未初始化");
        let out_view = self
            .output_texture_view
            .as_ref()
            .expect("comet output_texture_view 未初始化");

        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("comet_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: note_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: active_keys_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(out_view),
                },
            ],
        })
    }

    /// 渲染一帧。
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        style: CometRenderStyle,
        uniform: &CometUniformGpu,
        notes: &[CometNoteGpu],
        active_keys: &[u32; 128],
    ) {
        let width = uniform.frame_width;
        let height = uniform.frame_height;
        if width == 0 || height == 0 {
            return;
        }

        self.ensure_output_texture(device, width, height);
        self.ensure_note_buffer(device, notes.len());
        self.ensure_active_keys_buffer(device);

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[*uniform]));

        if let Some(ref buf) = self.note_buffer {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(notes));
        }
        if let Some(ref buf) = self.active_keys_buffer {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(active_keys));
        }

        let bind_group = self.rebuild_bind_group(device);
        let pipeline_index = style as usize;
        let pipeline = &self.pipelines[pipeline_index];

        let workgroup_size: u32 = 16;
        let dispatch_x = width.div_ceil(workgroup_size);
        let dispatch_y = height.div_ceil(workgroup_size);

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("comet_compute_pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }
    }

    /// 获取输出纹理引用，供 ExportPipeline 拷贝。
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref()
    }
}

impl Drop for CometRenderer {
    fn drop(&mut self) {
        crate::gpu_resource_tracker::sub_buffer(&self.uniform_buffer);
        if let Some(ref buf) = self.note_buffer {
            crate::gpu_resource_tracker::sub_buffer(buf);
        }
        if let Some(ref buf) = self.active_keys_buffer {
            crate::gpu_resource_tracker::sub_buffer(buf);
        }
        if let Some(ref tex) = self.output_texture {
            crate::gpu_resource_tracker::sub_texture(tex);
        }
    }
}
