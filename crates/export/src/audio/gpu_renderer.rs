//! GPU 音频合成器 v2 — CPU 只切时间片，region 查找 + voice 生命周期全进 GPU
//!
//! 对比 v1 删掉了整层 CPU voice 管理（Voice, EnvPhase, send_note_on/off,
//! HashMap preset cache, key_idx, real_voice_counts）。
//!
//! 双 pass：event_proc（处理 raw events → voice params）
//!       → render（voice params → audio samples）

use bytemuck::{Pod, Zeroable};
use std::path::Path;
use wgpu::util::DeviceExt;
use xsynth_soundfonts::sf2::load_soundfont_with_samples;

// ── 常量 ─────────────────────────────────────────────
const MAX_VOICES: u32 = 512;
const WGS: u32 = 256;
/// GPU 渲染每 chunk 的样本数。越小越实时（~21ms at 48kHz），
/// 但 dispatch 开销比例越高。1024 是 CPU 抽事件和 GPU 渲染的平衡点。
pub(crate) const GPU_BLOCK_SAMPLES: u32 = 1024;
const MAX_EVENTS: usize = 16_000_000;
const MIN_VEL: u8 = 1;

// ── GPU 数据结构 ────────────────────────────────────
/// 紧凑 RawEvent：tick_offset(4B) + data(4B 打包 kind|channel|key|vel)
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct RawEvent {
    pub tick_offset: u32,
    /// 打包: kind[7:0] | channel[15:8] | key[23:16] | vel[31:24]
    pub data: u32,
}

impl RawEvent {
    pub(crate) fn new(tick_offset: u32, kind: u32, channel: u32, key: u32, vel: u32) -> Self {
        Self {
            tick_offset,
            data: kind | (channel << 8) | (key << 16) | (vel << 24),
        }
    }
}
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuRegion {
    key_low: u32,
    key_high: u32,
    vel_low: u32,
    vel_high: u32,
    buf_offset: u32,
    buf_length: u32,
    loop_start: u32,
    loop_end: u32,
    loop_mode: u32,
    root_key: u32,
    tune: i32,
    volume: f32,
    pan: i32,
}
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuVoiceParams {
    position: f32,
    pitch_ratio: f32,
    volume: f32,
    pan: f32,
    sample_start: u32,
    sample_end: u32,
    loop_start: u32,
    loop_end: u32,
    enabled: u32,
    is_looping: u32,
    channel: u32,
    key: u32,
    released: u32,
    release_frame: u32,
    _pad: u32,
}
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uni {
    ne: u32,
    nr: u32,
    ns: u32,
    sr: u32,
}

// ── GpuRenderer ────────────────────────────────────
struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipe_event: wgpu::ComputePipeline,
    pipe_render: wgpu::ComputePipeline,
    bg_event: wgpu::BindGroup,
    bg_render: wgpu::BindGroup,
    buf_events: wgpu::Buffer,
    buf_regions: wgpu::Buffer,
    buf_samples: wgpu::Buffer,
    buf_params: wgpu::Buffer,
    buf_output: wgpu::Buffer,
    buf_staging: wgpu::Buffer,
    buf_staging2: wgpu::Buffer,
    buf_uni: wgpu::Buffer,
    params_cpu: Vec<GpuVoiceParams>,
    bs: u32,
    ch: u16,
    num_regions: u32,
    /// 0 → 本次用 buf_staging, 1 → 用 buf_staging2，轮换实现双缓冲流水线
    staging_toggle: u8,
}

/// GPU 异步提交的句柄。调用 `submit` 获得此句柄，之后调用 `readback` 等待 GPU 完成并取回音频。
pub(crate) struct PendingRender {
    rx: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    ns: u32,
    ch: u32,
    uses_buf2: bool,
}

impl GpuRenderer {
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        samples: &[f32],
        regions: &[GpuRegion],
        ch: u16,
    ) -> Result<Self, String> {
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
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(EVENT_PROC_SRC)),
        });
        let module_render = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(RENDER_SRC)),
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
    fn submit(&self, events: &[RawEvent], ns: u32) -> PendingRender {
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
                sr: 48000,
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
    fn readback(&self, pending: &PendingRender) -> Vec<f32> {
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

// ── GpuSynth ────────────────────────────────────────
pub(crate) struct GpuSynth {
    gpu: GpuRenderer,
    sample_rate: u32,
    /// GPU 端管理的 voice params 的 CPU 副本（初始为零，每块读回覆盖）
    _params: Vec<GpuVoiceParams>,
    /// 预分配的 params staging buffer（避免 readback_params 每块创建临时 buffer）
    buf_params_staging: wgpu::Buffer,
}

impl GpuSynth {
    pub(crate) fn new(sf2_path: &Path, sample_rate: u32, channels: u16) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .ok_or("无法获取 GPU adapter")?;
        let limits = wgpu::Limits {
            max_buffer_size: 1 << 30,
            max_storage_buffer_binding_size: 1 << 30,
            ..wgpu::Limits::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("GpuSynth"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                ..Default::default()
            },
            None,
        ))
        .map_err(|e| format!("设备创建失败: {}", e))?;

        let (presets, _samples) = load_soundfont_with_samples(sf2_path, sample_rate)
            .map_err(|e| format!("SF2: {}", e))?;

        // flat sample buffer + flat region table
        use std::collections::HashMap;
        let mut flat: Vec<f32> = Vec::new();
        let mut regions: Vec<GpuRegion> = Vec::new();
        let mut offs: HashMap<*const f32, u32> = HashMap::new();
        for preset in &presets {
            for reg in &preset.regions {
                if reg.sample.is_empty() || reg.sample[0].is_empty() {
                    continue;
                }
                let ptr = reg.sample[0].as_ptr();
                let off = *offs.entry(ptr).or_insert_with(|| {
                    let n = flat.len() as u32;
                    flat.extend_from_slice(&reg.sample[0]);
                    n
                });
                let tune = reg.coarse_tune as i32 * 100 + reg.fine_tune as i32;
                let lm = match reg.loop_mode {
                    xsynth_soundfonts::LoopMode::LoopContinuous => 1u32,
                    xsynth_soundfonts::LoopMode::LoopSustain => 2u32,
                    _ => 0u32,
                };
                regions.push(GpuRegion {
                    key_low: *reg.keyrange.start() as u32,
                    key_high: *reg.keyrange.end() as u32,
                    vel_low: *reg.velrange.start() as u32,
                    vel_high: *reg.velrange.end() as u32,
                    buf_offset: off,
                    buf_length: reg.sample[0].len() as u32,
                    loop_start: reg.loop_start,
                    loop_end: reg.loop_end,
                    loop_mode: lm,
                    root_key: reg.root_key as u32,
                    tune,
                    volume: reg.volume,
                    pan: reg.pan as i32,
                });
            }
        }
        tracing::info!("[GPU] {} regions, {} samples", regions.len(), flat.len());

        let gpu = GpuRenderer::new(device, queue, &flat, &regions, channels)?;
        let psize = (std::mem::size_of::<GpuVoiceParams>() * MAX_VOICES as usize) as u64;
        let buf_params_staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("param_staging"),
            size: psize,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            gpu,
            sample_rate,
            _params: vec![GpuVoiceParams::zeroed(); MAX_VOICES as usize],
            buf_params_staging,
        })
    }

    /// 同步渲染（向后兼容）。
    #[expect(dead_code)]
    pub(crate) fn render_block(&mut self, events: &[RawEvent]) -> Vec<f32> {
        let p = self.submit(events);
        self.readback_audio(&p)
    }

    /// 非阻塞提交：上传 params + 事件 + dispatch compute。
    /// GPU 立刻开始工作，CPU 可继续抽下一 block 的事件。
    pub(crate) fn submit(&mut self, events: &[RawEvent]) -> PendingRender {
        self.gpu.queue.write_buffer(
            &self.gpu.buf_params,
            0,
            bytemuck::cast_slice(&self.gpu.params_cpu),
        );
        self.gpu.submit(events, GPU_BLOCK_SAMPLES)
    }

    /// 阻塞读回：等待 GPU 完成 → 读音频 → 读回 params 供下 block 使用。
    pub(crate) fn readback_audio(&mut self, pending: &PendingRender) -> Vec<f32> {
        // 先读音频（这会等 GPU 完成）
        self.gpu.device.poll(wgpu::Maintain::Wait);
        let _ = pending.rx.recv().ok();

        let staging = if pending.uses_buf2 {
            &self.gpu.buf_staging2
        } else {
            &self.gpu.buf_staging
        };
        let ss = staging.slice(..);
        let data = ss.get_mapped_range();
        let smp: &[f32] = bytemuck::cast_slice(&data);
        let n = (pending.ns * pending.ch) as usize;
        let result = smp[..n.min(smp.len())].to_vec();
        drop(data);
        staging.unmap();

        // 在同一个 poll 周期内顺带读回 params（GPU 已完成）
        self.readback_params();

        result
    }

    /// 读回 GPU voice params（用预分配的 staging buffer）。
    fn readback_params(&mut self) {
        let psize = (std::mem::size_of::<GpuVoiceParams>() * MAX_VOICES as usize) as u64;
        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("param_read"),
            });
        enc.copy_buffer_to_buffer(&self.gpu.buf_params, 0, &self.buf_params_staging, 0, psize);
        self.gpu.queue.submit(Some(enc.finish()));
        let ss = self.buf_params_staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        ss.map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).ok();
        });
        self.gpu.device.poll(wgpu::Maintain::Wait);
        let _ = rx.recv().unwrap().ok();
        let data = ss.get_mapped_range();
        let smp: &[GpuVoiceParams] = bytemuck::cast_slice(&data);
        self.gpu
            .params_cpu
            .copy_from_slice(&smp[..MAX_VOICES as usize]);
        drop(data);
        self.buf_params_staging.unmap();
    }

    #[expect(dead_code)]
    pub(crate) fn take_samples(&mut self) -> Vec<f32> {
        Vec::new() // GpuSynth 不再持有 output，render_block 直接返回
    }

    pub(crate) fn is_active(&self) -> bool {
        // 有 GPU voice 活跃（保守返回 true，让 tail 判断）
        self.gpu.params_cpu.iter().any(|v| v.enabled != 0)
    }
}

// ── WGSL: event_proc ─────────────────────────────────
const EVENT_PROC_SRC: &str = r#"
const MV: u32 = 512u;
const WGS: u32 = 256u;

struct RE { to: u32, data: u32, }
struct RG { kl: u32, kh: u32, vl_l: u32, vl_h: u32, bo: u32, bl: u32, ls: u32, le: u32, lm: u32, rk: u32, tn: i32, vol: f32, pan: i32, }
struct VP { pos: f32, pitch: f32, vol: f32, pan: f32, ss: u32, se: u32, ls: u32, le: u32, ena: u32, lp: u32, ch: u32, ky: u32, rel: u32, rf: u32, _pad: u32, }
struct U { ne: u32, nr: u32, ns: u32, sr: u32, }

@group(0) @binding(0) var<storage, read_write> params: array<VP>;
@group(0) @binding(1) var<storage, read> events: array<RE>;
@group(0) @binding(2) var<storage, read> rgns: array<RG>;
@group(0) @binding(3) var<uniform> u: U;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let ei = id.x;
    if ei >= u.ne { return; }
    let ev = events[ei];

    let ev_kind = ev.data & 0xFFu;
    let ev_ch = (ev.data >> 8u) & 0xFFu;
    let ev_key = (ev.data >> 16u) & 0xFFu;
    let ev_vel = (ev.data >> 24u) & 0xFFu;

    if ev_kind == 0u {
        if ev_vel <= 1u { return; }
        var ri = 0u;
        var found = false;
        for (var i = 0u; i < u.nr; i++) {
            let r = rgns[i];
            if ev_key >= r.kl && ev_key <= r.kh && ev_vel >= r.vl_l && ev_vel <= r.vl_h {
                ri = i; found = true; break;
            }
        }
        if !found { return; }
        let r = rgns[ri];

        var slot = 0u;
        var found_slot = false;
        for (var i = 0u; i < MV; i++) {
            let p = params[i];
            if p.ena == 0u || (p.rel != 0u && p.rf + 480u < ev.to) {
                slot = i; found_slot = true; break;
            }
        }
        if !found_slot { return; }

        let semis = f32(ev_key) - f32(r.rk) + f32(r.tn) / 100.0;
        params[slot] = VP(
            f32(ev.to) * pow(2.0, semis / 12.0),
            pow(2.0, semis / 12.0),
            (f32(ev_vel) / 127.0) * r.vol,
            (f32(r.pan) - 64.0) / 63.0,
            r.bo, r.bo + r.bl,
            r.bo + r.ls, r.bo + r.le,
            1u,
            select(0u, 1u, r.lm == 1u || r.lm == 2u),
            ev_ch, ev_key, 0u, 0u, 0u,
        );
    } else if ev_kind == 1u {
        for (var i = 0u; i < MV; i++) {
            let p = params[i];
            if p.ena != 0u && p.ch == ev_ch && p.ky == ev_key && p.rel == 0u {
                params[i].rel = 1u;
                params[i].rf = ev.to;
                break;
            }
        }
    }
}
"#;

// ── WGSL: render ────────────────────────────────────
const RENDER_SRC: &str = r#"
const MV: u32 = 512u;
const WGS: u32 = 256u;

struct VP { pos: f32, pitch: f32, vol: f32, pan: f32, ss: u32, se: u32, ls: u32, le: u32, ena: u32, lp: u32, ch: u32, ky: u32, rel: u32, rf: u32, _pad: u32, }
struct U { ne: u32, nr: u32, ns: u32, sr: u32, }

@group(0) @binding(0) var<storage, read> params: array<VP>;
@group(0) @binding(1) var<storage, read> smpls: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;
@group(0) @binding(3) var<uniform> u: U;

var<workgroup> sl: array<f32, 256>;
var<workgroup> sr_: array<f32, 256>;

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    let sidx = id.x;
    if sidx >= u.ns { return; }

    var L = 0.0f; var R = 0.0f;
    var vi = li;
    loop {
        if vi >= MV { break; }
        let p = params[vi];
        if p.ena != 0u {
            var env = 1.0f;
            if p.rel != 0u && sidx >= p.rf {
                let rf = f32(sidx - p.rf);
                env = pow(0.995, rf);
                if env < 0.001 { env = 0.0; }
            }
            let pos = p.pos + f32(sidx) * p.pitch;
            var pi = u32(pos);
            let fr = pos - f32(pi);
            if p.lp != 0u && p.le > p.ls {
                let llen = p.le - p.ls;
                if llen > 0u && pi >= p.le {
                    pi = p.ls + (pi - p.ls) % llen;
                }
            }
            if pi < p.se - 1u {
                let i0 = p.ss + pi; let i1 = i0 + 1u;
                let sv = smpls[i0] + (smpls[i1] - smpls[i0]) * fr;
                let lg = p.vol * env * sqrt(max(1.0 - p.pan, 0.0));
                let rg = p.vol * env * sqrt(max(1.0 + p.pan, 0.0));
                L += sv * lg; R += sv * rg;
            }
        }
        vi += WGS;
    }

    sl[li] = L; sr_[li] = R;
    workgroupBarrier();
    if li == 0u {
        var tL = 0.0f; var tR = 0.0f;
        for (var i = 0u; i < WGS; i++) { tL += sl[i]; tR += sr_[i]; }
        let oi = sidx * 2u;
        out[oi] = tL;
        out[oi + 1u] = tR;
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::{EVENT_PROC_SRC, RENDER_SRC};
    #[test]
    fn validate_wgsl_shaders() {
        naga::front::wgsl::parse_str(EVENT_PROC_SRC).expect("event_proc WGSL");
        naga::front::wgsl::parse_str(RENDER_SRC).expect("render WGSL");
    }
}
