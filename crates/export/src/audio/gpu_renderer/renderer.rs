//! GpuRenderer — 底层 GPU 管线封装
//!
//! 负责 buffer 创建、compute pipeline 构建、submit/readback 循环。
//! GpuSynth 在其上做高阶事件注入和 params 管理。

use super::{EVENT_PROC_SRC, GPU_BLOCK_SAMPLES, MAX_EVENTS, MAX_VOICES, RENDER_SRC, WGS};
use super::{GpuRegion, GpuVoiceParams, RawEvent, Uni};
use bytemuck;
use bytemuck::Zeroable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

// ── GpuRenderer ────────────────────────────────────
pub(crate) struct GpuRenderer {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pipe_event: wgpu::ComputePipeline,
    pipe_render: wgpu::ComputePipeline,
    bg_event: wgpu::BindGroup,
    bg_render: wgpu::BindGroup,
    buf_events: wgpu::Buffer,
    buf_regions: wgpu::Buffer,
    buf_samples: wgpu::Buffer,
    pub(crate) buf_params: wgpu::Buffer,
    buf_output: wgpu::Buffer,
    pub(crate) buf_staging: wgpu::Buffer,
    pub(crate) buf_staging2: wgpu::Buffer,
    buf_uni: wgpu::Buffer,
    pub(crate) params_cpu: Vec<GpuVoiceParams>,
    bs: u32,
    pub(crate) ch: u16,
    num_regions: u32,
    /// 0 → 本次用 buf_staging, 1 → 用 buf_staging2，轮换实现双缓冲流水线
    staging_toggle: u8,
    /// 实际启用的最大 voice 数
    max_voices: u32,
    /// 导出采样率
    sample_rate: u32,
}

/// GPU 异步提交的句柄。调用 `submit` 获得此句柄，之后调用 `readback` 等待 GPU 完成并取回音频。
pub(crate) struct PendingRender {
    pub(crate) rx: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    pub(crate) ns: u32,
    pub(crate) ch: u32,
    pub(crate) uses_buf2: bool,
}

impl GpuRenderer {
    pub(crate) fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        samples: &[f32],
        regions: &[GpuRegion],
        ch: u16,
        sample_rate: u32,
        max_voices: u32,
    ) -> Result<Self, String> {
        let max_voices = max_voices.clamp(1, MAX_VOICES);
        let bs = GPU_BLOCK_SAMPLES;
        let out_sz = (bs * ch as u32 * 4) as u64;
        let params_sz = (std::mem::size_of::<GpuVoiceParams>() * MAX_VOICES as usize) as u64;
        let ev_sz = (std::mem::size_of::<RawEvent>() * MAX_EVENTS) as u64;

        let buf_samples = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("samples"),
            contents: bytemuck::cast_slice(samples),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let buf_regions = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("regions"),
            contents: bytemuck::cast_slice(regions),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let buf_events = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("events"),
            size: ev_sz,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buf_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: params_sz,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let buf_output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: out_sz,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let buf_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: out_sz,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buf_staging2 = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging2"),
            size: out_sz,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buf_uni = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uni"),
            size: std::mem::size_of::<Uni>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 初始化 params 为零
        queue.write_buffer(&buf_params, 0, &vec![0u8; params_sz as usize]);

        // ── 两个 bind group layout：event_proc 和 render 资源需求不同 ──
        let layout_event = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layout_event"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout_render = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layout_render"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bg_event = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_event"),
            layout: &layout_event,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buf_events.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf_regions.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: buf_uni.as_entire_binding(),
                },
            ],
        });
        let bg_render = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_render"),
            layout: &layout_render,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buf_samples.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf_output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: buf_uni.as_entire_binding(),
                },
            ],
        });

        let module_event = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("event_proc"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(EVENT_PROC_SRC)),
        });
        let module_render = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(RENDER_SRC)),
        });

        let pl_event = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl_event"),
            bind_group_layouts: &[&layout_event],
            push_constant_ranges: &[],
        });
        let pl_render = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl_render"),
            bind_group_layouts: &[&layout_render],
            push_constant_ranges: &[],
        });
        let pipe_event = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("pipe_event"),
            layout: Some(&pl_event),
            module: &module_event,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let pipe_render = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("pipe_render"),
            layout: Some(&pl_render),
            module: &module_render,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            pipe_event,
            pipe_render,
            bg_event,
            bg_render,
            buf_events,
            buf_regions,
            buf_samples,
            buf_params,
            buf_output,
            buf_staging,
            buf_staging2,
            buf_uni,
            params_cpu: vec![GpuVoiceParams::zeroed(); MAX_VOICES as usize],
            bs,
            ch,
            num_regions: regions.len() as u32,
            staging_toggle: 0,
            max_voices,
            sample_rate,
        })
    }

    /// 同步渲染（向后兼容，供单次调用场景使用）。
    #[expect(dead_code)]
    fn run(&self, events: &[RawEvent]) -> Vec<f32> {
        let p = self.submit(events, self.bs);
        self.readback(&p)
    }

    /// 非阻塞提交：上传事件 + dispatch compute → 返回句柄。
    /// GPU 开始工作后立即返回，CPU 可继续抽下 block 的事件。
    pub(crate) fn submit(&self, events: &[RawEvent], ns: u32) -> PendingRender {
        let ne = events.len() as u32;
        let ch = self.ch as u32;
        let zs = (ns * ch * 4) as usize;

        self.queue
            .write_buffer(&self.buf_events, 0, bytemuck::cast_slice(events));

        self.queue.write_buffer(
            &self.buf_uni,
            0,
            bytemuck::bytes_of(&Uni {
                ne,
                nr: self.num_regions,
                ns,
                sr: self.sample_rate,
                mv: self.max_voices,
                ch,
            }),
        );

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });

        if ne > 0 {
            let nwg = ne.div_ceil(WGS);
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("event"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&self.pipe_event);
            cp.set_bind_group(0, &self.bg_event, &[]);
            cp.dispatch_workgroups(nwg, 1, 1);
        }

        {
            let nwg = ns.div_ceil(WGS);
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("render"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&self.pipe_render);
            cp.set_bind_group(0, &self.bg_render, &[]);
            cp.dispatch_workgroups(nwg, 1, 1);
        }

        // 双缓冲：轮换使用 buf_staging / buf_staging2
        let uses_buf2 = self.staging_toggle & 1 == 1;
        let staging = if uses_buf2 {
            &self.buf_staging2
        } else {
            &self.buf_staging
        };
        enc.copy_buffer_to_buffer(&self.buf_output, 0, staging, 0, zs as u64);
        self.queue.submit(Some(enc.finish()));

        // 发起异步 map（不阻塞），返回 channel receiver 供 readback 等待
        let ss = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        ss.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });

        PendingRender {
            rx,
            ns,
            ch,
            uses_buf2,
        }
    }

    /// 阻塞读回：等待 GPU 完成 + 读取 staging buffer 中的音频数据。
    pub(crate) fn readback(&self, pending: &PendingRender) -> Vec<f32> {
        self.device.poll(wgpu::Maintain::Wait);
        let _ = pending.rx.recv().ok();

        let staging = if pending.uses_buf2 {
            &self.buf_staging2
        } else {
            &self.buf_staging
        };
        let ss = staging.slice(..);
        let data = ss.get_mapped_range();
        let smp: &[f32] = bytemuck::cast_slice(&data);
        let n = (pending.ns * pending.ch) as usize;
        let result = smp[..n.min(smp.len())].to_vec();
        drop(data);
        staging.unmap();
        result
    }
}
