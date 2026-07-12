//! GPU 音频渲染计算管线 — 封装 wgpu compute 管线
//!
//! 提供以下能力：
//! - 初始化 wgpu 设备/队列
//! - 加载 WGSL 着色器，创建计算管线
//! - 管理 GPU 缓冲区（样本数据、voice 状态、参数、输出）
//! - 调度计算、读取结果

use bytemuck::{Pod, Zeroable};

/// 渲染参数（与 WGSL `RenderParams` 结构体对齐）
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct RenderParams {
    pub(crate) sample_rate: f32,
    pub(crate) num_voices: u32,
    pub(crate) num_samples: u32,
    pub(crate) output_offset: u32,
    pub(crate) max_voices: u32,
    pub(crate) _pad0: u32,
    pub(crate) _pad1: u32,
    pub(crate) _pad2: u32,
}

/// Voice 状态（与 WGSL `VoiceState` 结构体对齐）
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct VoiceState {
    pub(crate) sample_pos: f32,
    pub(crate) pitch_ratio: f32,
    pub(crate) volume: f32,
    pub(crate) pan_left: f32,
    pub(crate) pan_right: f32,
    pub(crate) loop_start: f32,
    pub(crate) loop_end: f32,
    pub(crate) loop_mode: u32,
    pub(crate) sample_index: u32,
    pub(crate) envelope_attack: f32,
    pub(crate) envelope_decay: f32,
    pub(crate) envelope_sustain: f32,
    pub(crate) envelope_release: f32,
    pub(crate) envelope_value: f32,
    pub(crate) env_stage: u32,
    pub(crate) env_time: f32,
    pub(crate) is_active: u32,
    pub(crate) start_sample_offset: u32,
    pub(crate) release_sample_offset: u32,
    pub(crate) key: u32,
    pub(crate) channel: u32,
    pub(crate) _pad0: u32,
    pub(crate) _pad1: u32,
    pub(crate) _pad2: u32,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self::zeroed()
    }
}

/// GPU 计算渲染器
pub(crate) struct GpuComputeRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    compute_pipeline: wgpu::ComputePipeline,
    clear_pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,

    // 缓冲区
    params_buffer: wgpu::Buffer,
    voice_states_buffer: wgpu::Buffer,
    voice_staging_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    sample_data_buffer: wgpu::Buffer,
    output_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,

    // 元数据
    max_voices: u32,
    output_capacity: usize,
    #[allow(dead_code)]
    sample_data_size: u64,
}

impl GpuComputeRenderer {
    /// 创建 GPU 计算渲染器
    ///
    /// # 参数
    /// - `max_voices`: 最大并发 voice 数
    /// - `batch_samples`: 每批次最大样点数（立体声）
    /// - `sample_data`: 所有样本数据的扁平数组（立体声交错 f32）
    pub(crate) fn new(
        max_voices: u32,
        batch_samples: u32,
        sample_data: &[f32],
    ) -> crate::error::ExportResult<Self> {
        let shader_source = include_str!("shaders/voice_render.wgsl");

        // 创建 wgpu 实例
        let instance = wgpu::Instance::default();

        // 适配器（无窗口后端）
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|_| crate::error::ExportError::AudioWrite("无法创建 wgpu 适配器".into()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Lumino GPU Audio Render"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            trace: wgpu::Trace::default(),
        }))
        .map_err(|e| crate::error::ExportError::AudioWrite(format!("无法创建 wgpu 设备: {e}")))?;

        // 创建计算管线
        let compute_pipeline = Self::create_pipeline(&device, shader_source, "main")?;
        let clear_pipeline = Self::create_pipeline(&device, shader_source, "clear_output")?;

        // 创建缓冲区
        let sample_data_size = (sample_data.len() * 4) as u64; // f32 = 4 bytes
        let output_capacity = (batch_samples * 2) as usize; // 立体声交错

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_params"),
            size: std::mem::size_of::<RenderParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let voice_states_size = (max_voices as u64) * std::mem::size_of::<VoiceState>() as u64;
        let voice_states_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_voice_states"),
            size: voice_states_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let voice_staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_voice_states_staging"),
            size: voice_states_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sample_data_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_samples"),
            size: sample_data_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_output"),
            size: (output_capacity * 4) as u64, // f32 = 4 bytes
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_staging_buffer"),
            size: (output_capacity * 4) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 上传样本数据
        queue.write_buffer(&sample_data_buffer, 0, bytemuck::cast_slice(sample_data));

        // 创建绑定组
        let bind_group_layout = compute_pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_audio_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: voice_states_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: sample_data_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            device,
            queue,
            compute_pipeline,
            clear_pipeline,
            bind_group,
            params_buffer,
            voice_states_buffer,
            voice_staging_buffer,
            sample_data_buffer,
            output_buffer,
            staging_buffer,
            max_voices,
            output_capacity,
            sample_data_size,
        })
    }

    fn create_pipeline(
        device: &wgpu::Device,
        shader_source: &str,
        entry_point: &str,
    ) -> crate::error::ExportResult<wgpu::ComputePipeline> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu_audio_voice_render"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_audio_bind_group_layout"),
            entries: &[
                // binding 0: RenderParams (uniform)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(
                            std::mem::size_of::<RenderParams>() as u64,
                        ),
                    },
                    count: None,
                },
                // binding 1: VoiceState[] (storage read_write)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 2: sample_data[] (storage read)
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
                // binding 3: output_buffer[] (storage read_write)
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
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu_audio_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gpu_audio_compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(compute_pipeline)
    }

    /// 上传活跃 voice 状态到 GPU
    ///
    /// 只上传实际活跃的 voice，避免每次传输完整的 `max_voices` 状态。
    pub(crate) fn upload_voice_states(&self, states: &[VoiceState]) {
        if states.is_empty() {
            return;
        }
        self.queue
            .write_buffer(&self.voice_states_buffer, 0, bytemuck::cast_slice(states));
    }

    /// 调度计算着色器
    ///
    /// 流程：先 GPU 端清零输出缓冲区，再调度 voice 渲染核函数，最后拷贝到 staging。
    pub(crate) fn dispatch(&mut self, params: &RenderParams) {
        let num_voices = params.num_voices;
        if num_voices == 0 {
            return;
        }

        // 更新参数 uniform
        self.queue
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(params));

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Audio Render Encoder"),
            });

        // 1. GPU 端清零输出缓冲区
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Clear Output Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.clear_pipeline);
            cpass.set_bind_group(0, &self.bind_group, &[]);
            let clear_count = (params.num_samples * 2).div_ceil(256);
            cpass.dispatch_workgroups(clear_count, 1, 1);
        }

        // 2. 调度 voice 渲染
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Compute Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.bind_group, &[]);
            // 每个 workgroup 256 个线程，每个线程处理一个 voice
            let workgroup_count = num_voices.div_ceil(256);
            cpass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // 3. 拷贝输出结果到 staging buffer
        let output_size = ((params.num_samples * 2) * 4) as u64;
        encoder.copy_buffer_to_buffer(&self.output_buffer, 0, &self.staging_buffer, 0, output_size);

        self.queue.submit(Some(encoder.finish()));
    }

    /// 读取渲染后的音频输出
    pub(crate) fn read_output(
        &self,
        expected_samples: u32,
    ) -> crate::error::ExportResult<Vec<f32>> {
        // 等待 GPU 完成
        let buffer_slice = self.staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        rx.recv()
            .map_err(|_| crate::error::ExportError::AudioWrite("GPU 读取超时".into()))?
            .map_err(|e| crate::error::ExportError::AudioWrite(format!("GPU 映射失败: {e}")))?;

        // Copy the data
        let mut result = Vec::with_capacity((expected_samples * 2) as usize);
        {
            let data = buffer_slice.get_mapped_range();
            let slice: &[f32] = bytemuck::cast_slice(&data);
            result.extend_from_slice(&slice[..((expected_samples * 2) as usize)]);
        }

        self.staging_buffer.unmap();
        Ok(result)
    }

    /// 确保输出缓冲区足以容纳指定样点数
    ///
    /// 当一次性渲染较长片段时，输出可能超过初始 `batch_samples` 容量，需要重新分配。
    pub(crate) fn ensure_output_capacity(
        &mut self,
        samples: u32,
    ) -> crate::error::ExportResult<()> {
        let needed = (samples * 2) as usize;
        if needed <= self.output_capacity {
            return Ok(());
        }

        let size = (needed * 4) as u64;
        self.output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_output"),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        self.staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_audio_staging_buffer"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.output_capacity = needed;

        // 重新创建绑定组，因为 output buffer 已变
        let bind_group_layout = self.compute_pipeline.get_bind_group_layout(0);
        self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_audio_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.voice_states_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.sample_data_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.output_buffer.as_entire_binding(),
                },
            ],
        });

        Ok(())
    }

    /// 读取 GPU 写回后的 voice 状态
    ///
    /// 仅读取前 `count` 个 voice（即本次 dispatch 实际使用的部分），
    /// 避免每次回读完整的 `max_voices` 状态。
    pub(crate) fn read_voice_states(
        &self,
        count: u32,
    ) -> crate::error::ExportResult<Vec<VoiceState>> {
        let count = count.min(self.max_voices) as usize;
        if count == 0 {
            return Ok(Vec::new());
        }

        let size = (count as u64) * std::mem::size_of::<VoiceState>() as u64;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Read Voice States Encoder"),
            });
        encoder.copy_buffer_to_buffer(
            &self.voice_states_buffer,
            0,
            &self.voice_staging_buffer,
            0,
            size,
        );
        self.queue.submit(Some(encoder.finish()));

        let buffer_slice = self.voice_staging_buffer.slice(..size);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        rx.recv()
            .map_err(|_| crate::error::ExportError::AudioWrite("GPU voice 读取超时".into()))?
            .map_err(|e| {
                crate::error::ExportError::AudioWrite(format!("GPU voice 映射失败: {e}"))
            })?;

        let mut result = Vec::with_capacity(count);
        {
            let data = buffer_slice.get_mapped_range();
            let slice: &[VoiceState] = bytemuck::cast_slice(&data);
            result.extend_from_slice(&slice[..count]);
        }
        self.voice_staging_buffer.unmap();
        Ok(result)
    }
}
