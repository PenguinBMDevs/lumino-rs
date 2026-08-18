//! 瀑布流模式 GPU 渲染器
//!
//! 使用 compute shader 在 GPU 上直接渲染瀑布流帧，
//! 支持音符绘制、钢琴键盘（含活跃键高亮）、速度控制。
//!
//! # 生命周期
//!
//! 1. `new()` — 创建渲染器，编译 compute shader
//! 2. `render()` — 每帧调用：上传音符数据、dispatch compute shader、写入 storage texture
//! 3. `storage_texture()` — 获取输出纹理，供 export pipeline 读回

use crate::gpu_resource_tracker::{TrackedBuffer, TrackedTexture};

/// 单个瀑布流音符数据（与 waterfall.wgsl 中 WaterfallNote 匹配）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterfallNoteGpu {
    pub key: u32,
    pub start_tick: u32,
    pub end_tick: u32,
    pub color_packed: u32,
}

/// Uniform 参数（与 waterfall.wgsl 中 WaterfallUniform 匹配）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterfallUniformGpu {
    pub tick: u32,
    pub ppq: u32,
    pub key_count: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub kb_height: u32,
    pub speed: f32,
    pub _padding: u32,
}

/// 瀑布流 GPU 渲染器
pub struct WaterfallRenderer {
    compute_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,

    uniform_buffer: TrackedBuffer,
    note_buffer: Option<TrackedBuffer>,
    active_key_colors_buffer: Option<TrackedBuffer>,
    key_offsets_buffer: Option<TrackedBuffer>,

    output_texture: Option<TrackedTexture>,
    output_texture_view: Option<wgpu::TextureView>,

    note_capacity: usize,
    key_offsets_capacity: usize,
    current_width: u32,
    current_height: u32,
}

impl WaterfallRenderer {
    const SHADER: &'static str = include_str!("shaders/waterfall.wgsl");
    const INITIAL_NOTE_CAPACITY: usize = 4096;

    /// 创建瀑布流渲染器。
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = crate::shader::create_shader_module(device, "waterfall_shader", Self::SHADER);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("waterfall_bind_group_layout"),
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
                // binding 1: notes storage buffer (read-only)
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
                // binding 2: active_key_colors storage buffer (read-only)
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
                // binding 4: key_offsets storage buffer (read-only)
                // 音符分桶偏移表：`[offsets[key], offsets[key+1])` 为 key 桶区间，
                // 长度 = key_count + 1（动态分桶，支持任意 key 数量）
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let compute_pipeline = crate::pipeline::ComputePipelineBuilder::new(
            device,
            "waterfall_compute_pipeline",
            &shader,
        )
        .bind_group(&bind_group_layout)
        .build();

        let uniform_buffer = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("waterfall_uniform_buffer"),
                size: std::mem::size_of::<WaterfallUniformGpu>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );

        Self {
            compute_pipeline,
            bind_group_layout,
            bind_group: None,
            uniform_buffer,
            note_buffer: None,
            active_key_colors_buffer: None,
            key_offsets_buffer: None,
            output_texture: None,
            output_texture_view: None,
            note_capacity: 0,
            key_offsets_capacity: 0,
            current_width: 0,
            current_height: 0,
        }
    }

    /// 确保输出纹理已创建（尺寸变化时重建）。
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
        // 释放旧纹理（Option::take 触发 Drop 自动注销）
        self.output_texture.take();
        self.output_texture_view.take();

        let texture = TrackedTexture::new(
            device,
            &wgpu::TextureDescriptor {
                label: Some("waterfall_output_texture"),
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
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.output_texture = Some(texture);
        self.output_texture_view = Some(view);
        self.current_width = width;
        self.current_height = height;
        // 尺寸变化 → bind group 需要重建
        self.bind_group = None;
    }

    /// 确保 note buffer 有足够容量。
    fn ensure_note_buffer(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.note_capacity {
            return;
        }
        let new_cap = count.next_power_of_two().max(Self::INITIAL_NOTE_CAPACITY);
        let size = (new_cap * std::mem::size_of::<WaterfallNoteGpu>()) as u64;
        // 旧缓冲由 Option::take 触发 Drop 自动注销
        let buffer = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("waterfall_note_buffer"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.note_buffer = Some(buffer);
        self.note_capacity = new_cap;
        self.bind_group = None;
    }

    /// 确保 active_key_colors buffer 存在（128 个 u32）。
    fn ensure_active_key_colors_buffer(&mut self, device: &wgpu::Device) {
        if self.active_key_colors_buffer.is_some() {
            return;
        }
        let buffer = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("waterfall_active_key_colors_buffer"),
                size: (128 * 4) as u64, // 128 u32
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.active_key_colors_buffer = Some(buffer);
        self.bind_group = None;
    }

    /// 确保 key_offsets buffer 有足够容量（动态分桶：key_count + 1 个 u32）
    fn ensure_key_offsets_buffer(&mut self, device: &wgpu::Device, key_count: usize) {
        let needed = key_count + 1;
        if needed <= self.key_offsets_capacity {
            return;
        }
        let new_cap = needed.next_power_of_two().max(65); // 至少 64 键 + 1 哨兵
        let size = (new_cap * std::mem::size_of::<u32>()) as u64;
        // 旧缓冲由 Option::take 触发 Drop 自动注销
        let buffer = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("waterfall_key_offsets_buffer"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.key_offsets_buffer = Some(buffer);
        self.key_offsets_capacity = new_cap;
        self.bind_group = None;
    }

    /// 重建 bind group（当 buffers 或 texture 变化时）。
    fn rebuild_bind_group(&mut self, device: &wgpu::Device) {
        let note_buf = self.note_buffer.as_ref().expect("note_buffer 未初始化");
        let key_colors_buf = self
            .active_key_colors_buffer
            .as_ref()
            .expect("active_key_colors_buffer 未初始化");
        let out_view = self
            .output_texture_view
            .as_ref()
            .expect("output_texture_view 未初始化");

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waterfall_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: note_buf.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: key_colors_buf.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(out_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self
                        .key_offsets_buffer
                        .as_ref()
                        .expect("key_offsets_buffer 未初始化")
                        .inner()
                        .as_entire_binding(),
                },
            ],
        });
        self.bind_group = Some(bind_group);
    }

    /// 渲染瀑布流帧。
    ///
    /// # 参数
    /// - `device` — wgpu 设备
    /// - `queue` — wgpu 队列
    /// - `encoder` — 命令编码器（compute pass 将追加到此 encoder）
    /// - `params` — 瀑布流 uniform 参数
    /// - `notes` — 音符数据切片（按 (key, start_tick) 升序排列）
    /// - `key_offsets` — 分桶偏移表（len = key_count + 1），桶 k 区间为
    ///   `[key_offsets[k], key_offsets[k+1])`。动态分桶：支持任意 key 数量。
    ///   为空时回退为单桶（全部音符），shader 仍可工作。
    /// - `active_key_colors` — 活跃键颜色数组（128 个 u32，packed RGBA `0xRRGGBBAA`，0 表示无高亮）
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        params: &WaterfallUniformGpu,
        notes: &[WaterfallNoteGpu],
        key_offsets: &[u32],
        active_key_colors: &[u32; 128],
    ) {
        let width = params.frame_width;
        let height = params.frame_height;
        if width == 0 || height == 0 {
            return;
        }

        // 确保资源已创建
        self.ensure_output_texture(device, width, height);
        self.ensure_note_buffer(device, notes.len());
        self.ensure_active_key_colors_buffer(device);
        self.ensure_key_offsets_buffer(device, key_offsets.len().saturating_sub(1));

        // 重建 bind group（如果资源发生了变化）
        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }

        // 上传 uniform
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[*params]),
        );

        // 上传音符数据
        if let Some(ref buf) = self.note_buffer {
            let note_bytes = bytemuck::cast_slice(notes);
            queue.write_buffer(buf.inner(), 0, note_bytes);
        }

        // 上传分桶偏移表（空时回退单桶：全部音符归入 key 0 桶。
        // 注意必须上传完整 key_count+1 长度，shader 会访问 key_offsets[key_count]）
        if let Some(ref buf) = self.key_offsets_buffer {
            if key_offsets.is_empty() {
                // 单桶回退：key 0 桶 = [0, len]，其余 key 桶为空
                let mut offsets = vec![notes.len() as u32; params.key_count as usize + 1];
                offsets[0] = 0;
                queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&offsets));
            } else {
                queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(key_offsets));
            }
        }

        // 上传活跃键颜色
        if let Some(ref buf) = self.active_key_colors_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(active_key_colors));
        }

        // 计算 dispatch 参数
        let workgroup_size: u32 = 16;
        let dispatch_x = width.div_ceil(workgroup_size);
        let dispatch_y = height.div_ceil(workgroup_size);

        // dispatch compute shader
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("waterfall_compute_pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.compute_pipeline);
            if let Some(ref bg) = self.bind_group {
                compute_pass.set_bind_group(0, bg, &[]);
            }
            compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }
    }

    /// 获取输出纹理的引用（用于 export pipeline 读回）。
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref().map(|t| t.inner())
    }
}
