//! GPU 音频合成器 — 完整替代 xsynth 的 GPU 渲染管线
//!
//! CPU 端做 voice 生命周期 + SF2 region 查找 + 包络，GPU 端做采样插值 + 混音。

use std::path::Path;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use xsynth_soundfonts::LoopMode;
use xsynth_soundfonts::sf2::load_soundfont_with_samples;

// ── GPU 着色器常量 ─────────────────────────────────────

const MAX_VOICES: usize = 512;
const WORKGROUP_SIZE: u32 = 256; // 硬件上限，用 stride 循环处理 >256 voices
/// GPU 渲染块大小（样本数）= ~5.94s @ 44100Hz
/// 大块减少 dispatch 次数、提高 GPU occupancy、摊销 overhead
pub(crate) const GPU_BLOCK_SAMPLES: u32 = 262_144;

// ── WGSL 对齐的数据结构 ────────────────────────────────

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
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct RenderUniforms {
    num_voices: u32,
    num_samples: u32,
    sample_rate: u32,
    _pad: u32,
}

// ── CPU 端数据结构 ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum EnvPhase {
    Attack,
    Decay,
    Sustain,
    Release,
    Done,
}

/// 一个活跃的合成 voice
struct Voice {
    channel: u32,
    key: u8,
    sample_offset: u32,
    sample_length: u32,
    loop_start: u32,
    loop_end: u32,
    is_looping: bool,
    position: f32,
    envelope: f32,
    env_phase: EnvPhase,
    target_volume: f32,
    pan: f32,
    pitch_ratio: f32,
}

/// 预计算 region 元数据（flat lookup 用）
struct RegionMeta {
    key_low: u8,
    key_high: u8,
    vel_low: u8,
    vel_high: u8,
    buf_offset: u32,
    buf_length: u32,
    loop_start: u32,
    loop_end: u32,
    loop_mode: LoopMode,
    root_key: u8,
    fine_tune: i16,
    coarse_tune: i16,
    volume: f32,
    pan: i16,
}

/// 按 preset 分组的 region 列表 + 按 MIDI key 的二级索引
///
/// 查找：先取 `key_idx[key as usize]` 获得 ~50-100 个候选 region index，
/// 再在其中按 velocity 范围扫描。避免了扫描 preset 全部 10000+ regions。
struct PresetRegions {
    regions: Vec<RegionMeta>,
    /// key_idx[key as usize] = 覆盖该 MIDI key 的 region 在 `regions` 中的下标列表
    /// key 是 u8 (0-255)，超出标准 0-127 范围的 key 会命中空 Vec，安全返回 None
    key_idx: Vec<Vec<u16>>,
}

// ── GPU 渲染器封装 ─────────────────────────────────────

struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sample_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    output_buf: wgpu::Buffer,
    staging_buf: wgpu::Buffer,
    /// 预分配的 voice params——避免每块堆分配
    params: Vec<GpuVoiceParams>,
    block_samples: u32,
    channels: u16,
}

impl GpuRenderer {
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        sample_data: &[f32],
        channels: u16,
    ) -> Result<Self, String> {
        let block_samples = GPU_BLOCK_SAMPLES;
        let sample_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sample_buf"),
            contents: bytemuck::cast_slice(sample_data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let voice_params_size = (std::mem::size_of::<GpuVoiceParams>() * MAX_VOICES) as u64;
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params_buf"),
            size: voice_params_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_init = RenderUniforms {
            num_voices: 0,
            num_samples: block_samples,
            sample_rate: 48000,
            _pad: 0,
        };
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniform_buf"),
            contents: bytemuck::bytes_of(&uniform_init),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let out_bytes = (block_samples * channels as u32 * 4) as u64;
        let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output_buf"),
            size: out_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_buf"),
            size: out_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voice_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_SRC)),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bg_layout"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: sample_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform_buf.as_entire_binding(),
                },
            ],
        });
        let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl_layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("voice_pipeline"),
            layout: Some(&pl_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            layout,
            bind_group,
            sample_buf,
            params_buf,
            uniform_buf,
            output_buf,
            staging_buf,
            params: vec![GpuVoiceParams::zeroed(); MAX_VOICES],
            block_samples,
            channels,
        })
    }

    fn render(&mut self, voices: &[Voice], sample_rate: u32) -> Vec<f32> {
        let _t0 = std::time::Instant::now();
        let nv = voices.len().min(MAX_VOICES) as u32;
        let ns = self.block_samples;
        let ch = self.channels as u32;
        let zs = (ns * ch * 4) as usize;

        // Uniforms
        self.queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&RenderUniforms {
                num_voices: nv,
                num_samples: ns,
                sample_rate,
                _pad: 0,
            }),
        );
        let _t1 = std::time::Instant::now();

        // Pack params into pre-allocated vec（无堆分配）
        let gp = &mut self.params;
        for v in gp.iter_mut() {
            *v = GpuVoiceParams::zeroed();
        }
        for (i, v) in voices.iter().enumerate().take(MAX_VOICES) {
            gp[i] = GpuVoiceParams {
                position: v.position,
                pitch_ratio: v.pitch_ratio,
                volume: (v.envelope * v.target_volume).clamp(0.0, 1.0),
                pan: v.pan.clamp(-1.0, 1.0),
                sample_start: v.sample_offset,
                sample_end: v.sample_offset + v.sample_length,
                loop_start: v.sample_offset + v.loop_start,
                loop_end: v.sample_offset + v.loop_end,
                enabled: 1,
                is_looping: if v.is_looping { 1 } else { 0 },
            };
        }
        self.queue
            .write_buffer(&self.params_buf, 0, bytemuck::cast_slice(gp));
        let _t2 = std::time::Instant::now();

        // Dispatch
        let nwgs = ns.div_ceil(WORKGROUP_SIZE);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cp"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&self.pipeline);
            cp.set_bind_group(0, &self.bind_group, &[]);
            cp.dispatch_workgroups(nwgs, 1, 1);
        }
        enc.copy_buffer_to_buffer(&self.output_buf, 0, &self.staging_buf, 0, zs as u64);
        self.queue.submit(Some(enc.finish()));
        let _t3 = std::time::Instant::now();

        // Readback
        let ss = self.staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        ss.map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).ok();
        });
        self.device.poll(wgpu::Maintain::Wait);
        let _t4 = std::time::Instant::now();
        let _ = rx.recv().unwrap().ok();
        let data = ss.get_mapped_range();
        let smp: &[f32] = bytemuck::cast_slice(&data);
        let result = smp[..(ns * ch) as usize].to_vec();
        drop(data);
        self.staging_buf.unmap();
        let _t5 = std::time::Instant::now();

        tracing::debug!(
            "[GPU.perf] upload={:?} pack={:?} submit={:?} poll_wait={:?} readback={:?}  nv={} ns={}",
            _t1 - _t0,
            _t2 - _t1,
            _t3 - _t2,
            _t4 - _t3,
            _t5 - _t4,
            nv,
            ns,
        );
        result
    }
}

// ── GpuSynth ──────────────────────────────────────────

/// GPU 合成器 — 完整替代 xsynth 的合成引擎
pub(crate) struct GpuSynth {
    gpu: GpuRenderer,
    /// (bank, program) → PresetRegions，含 key 二级索引，避免 10k+ region 线性扫描
    regions_by_preset: std::collections::HashMap<(u16, u16), PresetRegions>,
    voices: Vec<Voice>,
    programs: [u8; 16],
    banks: [u16; 16],
    pitch_bends: [f32; 16],
    channel_volumes: [f32; 16],
    sample_rate: u32,
    #[allow(dead_code)]
    _channels: u16,
    block_samples: u32,
    /// 输出缓存
    output: Vec<f32>,
}

impl GpuSynth {
    /// 创建 GPU 合成器
    pub(crate) fn new(sf2_path: &Path, sample_rate: u32, channels: u16) -> Result<Self, String> {
        let block_samples = GPU_BLOCK_SAMPLES;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .ok_or("无法获取 GPU adapter")?;
        // SF2 sample buffer 可能超过 wgpu 默认 256MB 限制
        // RTX 2060 Vulkan 后端实际支持 2GB+, 这里设 1GB 安全值
        let limits = wgpu::Limits {
            max_buffer_size: 1 << 30,                 // 1 GB
            max_storage_buffer_binding_size: 1 << 30, // 1 GB
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

        // 加载 SF2
        let (presets, _samples) = load_soundfont_with_samples(sf2_path, sample_rate)
            .map_err(|e| format!("SF2: {}", e))?;

        // 构建 flat sample buffer + 按 preset 分组的 region 查找表
        // 用 Arc 指针去重：同一声 sample 数据只上传一次到 GPU
        use std::collections::HashMap;
        let mut flat: Vec<f32> = Vec::new();
        let mut regions_by_preset: HashMap<(u16, u16), PresetRegions> = HashMap::new();
        let mut sample_offsets: HashMap<*const f32, u32> = HashMap::new();
        let mut total_regions: usize = 0;
        for preset in &presets {
            let pk = (preset.bank, preset.preset);
            let mut regions: Vec<RegionMeta> = Vec::new();
            for reg in &preset.regions {
                if reg.sample.is_empty() || reg.sample[0].is_empty() {
                    continue;
                }
                let ptr = reg.sample[0].as_ptr();
                let off = match sample_offsets.get(&ptr) {
                    Some(&existing_off) => existing_off,
                    None => {
                        let new_off = flat.len() as u32;
                        flat.extend_from_slice(&reg.sample[0]);
                        sample_offsets.insert(ptr, new_off);
                        new_off
                    }
                };
                regions.push(RegionMeta {
                    key_low: *reg.keyrange.start(),
                    key_high: *reg.keyrange.end(),
                    vel_low: *reg.velrange.start(),
                    vel_high: *reg.velrange.end(),
                    buf_offset: off,
                    buf_length: reg.sample[0].len() as u32,
                    loop_start: reg.loop_start,
                    loop_end: reg.loop_end,
                    loop_mode: reg.loop_mode,
                    root_key: reg.root_key,
                    fine_tune: reg.fine_tune,
                    coarse_tune: reg.coarse_tune,
                    volume: reg.volume,
                    pan: reg.pan,
                });
                total_regions += 1;
            }
            // 构建 key 二级索引：每个 region 覆盖的 key range 都加入对应 bucket
            let mut key_idx: Vec<Vec<u16>> = (0..256).map(|_| Vec::new()).collect();
            for (ri, r) in regions.iter().enumerate() {
                let lo = r.key_low as usize;
                let hi = r.key_high as usize;
                for k in lo..=hi.min(255) {
                    key_idx[k].push(ri as u16);
                }
            }
            regions_by_preset.insert(pk, PresetRegions { regions, key_idx });
        }
        tracing::info!(
            "[GPU] {} presets, {} regions, {} sample frames",
            regions_by_preset.len(),
            total_regions,
            flat.len()
        );

        let gpu = GpuRenderer::new(device, queue, &flat, channels)?;
        let output = vec![0.0f32; (block_samples * channels as u32) as usize];

        Ok(Self {
            gpu,
            regions_by_preset,
            voices: Vec::with_capacity(MAX_VOICES),
            programs: [0; 16],
            banks: [0; 16],
            pitch_bends: [0.0; 16],
            channel_volumes: [1.0; 16],
            sample_rate,
            _channels: channels,
            block_samples,
            output,
        })
    }

    /// 发送通用 RenderCommand
    pub(crate) fn send_command(&mut self, cmd: &super::block_render::RenderCommand) {
        match *cmd {
            super::block_render::RenderCommand::NoteOn { key, vel, channel } => {
                self.send_note_on(channel, key, vel);
            }
            super::block_render::RenderCommand::NoteOff { key, channel } => {
                self.send_note_off(channel, key);
            }
            super::block_render::RenderCommand::ProgramChange { program, channel } => {
                self.send_program_change(channel, program);
            }
            super::block_render::RenderCommand::ControlChange {
                controller,
                value,
                channel,
            } => {
                self.send_control_change(channel, controller, value);
            }
            super::block_render::RenderCommand::PitchBend { value, channel } => {
                self.send_pitch_bend(channel, value);
            }
        }
    }

    /// 发送 MIDI 事件
    pub(crate) fn send_note_on(&mut self, channel: u32, key: u8, vel: u8) {
        if vel == 0 {
            return self.send_note_off(channel, key);
        }
        // swap_remove(0) = 用末尾 voice 替换最旧 voice → O(1) 淘汰
        // 相比 Vec::remove(0) 的 O(N) memmove，对黑乐谱密集 NoteOn 区别巨大
        if self.voices.len() >= MAX_VOICES {
            self.voices.swap_remove(0);
        }
        let ch = channel as usize;
        // O(1) 按 (bank, program) 定位 preset，再用 key 二级索引定位 ~100 候选 region
        let preset_key = (self.banks[ch], self.programs[ch] as u16);
        let meta = self.regions_by_preset.get(&preset_key).and_then(|pr| {
            let kidx = key as usize;
            pr.key_idx.get(kidx).and_then(|indices| {
                indices.iter().find_map(|&ri| {
                    let r = &pr.regions[ri as usize];
                    if vel >= r.vel_low && vel <= r.vel_high {
                        Some(r)
                    } else {
                        None
                    }
                })
            })
        });
        let Some(meta) = meta else { return };

        let semis = (key as f32 - meta.root_key as f32)
            + meta.coarse_tune as f32
            + meta.fine_tune as f32 / 100.0
            + self.pitch_bends[ch];
        let pitch_ratio = 2.0f32.powf(semis / 12.0);
        let pan = ((meta.pan as f32) - 64.0) / 63.0;

        self.voices.push(Voice {
            channel,
            key,
            sample_offset: meta.buf_offset,
            sample_length: meta.buf_length,
            loop_start: meta.loop_start,
            loop_end: meta.loop_end,
            is_looping: matches!(
                meta.loop_mode,
                LoopMode::LoopContinuous | LoopMode::LoopSustain
            ),
            position: 0.0,
            envelope: 0.0,
            env_phase: EnvPhase::Attack,
            target_volume: (vel as f32 / 127.0) * meta.volume * self.channel_volumes[ch],
            pan,
            pitch_ratio,
        });
    }

    pub(crate) fn send_note_off(&mut self, channel: u32, key: u8) {
        // 大多数 NoteOff 匹配恰好一个 voice，find 命中即 break，避免 O(512) 全扫
        for v in self.voices.iter_mut() {
            if v.channel == channel
                && v.key == key
                && v.env_phase != EnvPhase::Release
                && v.env_phase != EnvPhase::Done
            {
                v.env_phase = EnvPhase::Release;
                break;
            }
        }
    }

    pub(crate) fn send_program_change(&mut self, channel: u32, program: u8) {
        if let Some(p) = self.programs.get_mut(channel as usize) {
            *p = program;
        }
    }

    pub(crate) fn send_control_change(&mut self, channel: u32, controller: u8, value: u8) {
        let ch = channel as usize;
        match controller {
            0 => {
                if let Some(b) = self.banks.get_mut(ch) {
                    *b = value as u16;
                }
            }
            7 => {
                if let Some(v) = self.channel_volumes.get_mut(ch) {
                    *v = value as f32 / 127.0;
                }
            }
            10 => {
                let pan = (value as f32 - 64.0) / 63.0;
                for v in &mut self.voices {
                    if v.channel == channel {
                        v.pan = pan;
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn send_pitch_bend(&mut self, channel: u32, value: i16) {
        if let Some(pb) = self.pitch_bends.get_mut(channel as usize) {
            *pb = (value as f32 / 8192.0) * 2.0;
        }
    }

    /// 渲染一个 block 的音频
    pub(crate) fn render_block(&mut self, delta_sec: f64) {
        let _t0 = std::time::Instant::now();
        let sample_rate = self.sample_rate as f64;
        let frames = delta_sec * sample_rate;

        // 更新所有 voice 的位置和包络
        let nv_before = self.voices.len();
        self.voices.retain_mut(|v| {
            v.position += v.pitch_ratio * frames as f32;
            match v.env_phase {
                EnvPhase::Attack => {
                    v.envelope += (1.0 - v.envelope) * 0.3;
                    if v.envelope > 0.99 {
                        v.env_phase = EnvPhase::Decay;
                    }
                }
                EnvPhase::Decay => {
                    v.envelope += (0.8 - v.envelope) * 0.05;
                    if (v.envelope - 0.8).abs() < 0.01 {
                        v.env_phase = EnvPhase::Sustain;
                    }
                }
                EnvPhase::Sustain => {}
                EnvPhase::Release => {
                    v.envelope *= 0.995;
                    if v.envelope < 0.001 {
                        return false;
                    }
                }
                EnvPhase::Done => return false,
            }
            if !v.is_looping && v.position as u32 >= v.sample_length {
                return false;
            }
            true
        });
        let _t1 = std::time::Instant::now();

        // GPU 渲染
        let gpu_out = self.gpu.render(&self.voices, self.sample_rate);
        let _t2 = std::time::Instant::now();

        self.output = gpu_out;

        if nv_before > 0 {
            tracing::debug!(
                "[GPU.block] env={:?} gpu={:?} nv={}→{}",
                _t1 - _t0,
                _t2 - _t1,
                nv_before,
                self.voices.len(),
            );
        }
    }

    pub(crate) fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.output)
    }

    pub(crate) fn voice_count(&self) -> usize {
        self.voices.len()
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.voices.is_empty()
    }
}

// ── WGSL 着色器 ───────────────────────────────────────

const SHADER_SRC: &str = r#"
const WGS: u32 = 256u;

struct VP {
    pos: f32,
    pitch: f32,
    vol: f32,
    pan: f32,
    s_start: u32,
    s_end: u32,
    l_start: u32,
    l_end: u32,
    ena: u32,
    looping: u32,
}

struct U {
    nv: u32,
    ns: u32,
    sr: u32,
    _pad: u32,
}

@group(0) @binding(0) var<storage, read> params: array<VP>;
@group(0) @binding(1) var<storage, read> samples: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;
@group(0) @binding(3) var<uniform> u: U;

var<workgroup> sl: array<f32, 256>;
var<workgroup> sr: array<f32, 256>;

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    let sidx = id.x;
    if sidx >= u.ns { return; }

    // 每个线程用 stride 循环处理多个 voice
    // 例如 256 threads × 512 voices → 每个线程处理 2 voices
    var L = 0.0f;
    var R = 0.0f;
    var v = li;
    loop {
        if v >= u.nv { break; }
        let p = params[v];
        if p.ena != 0u {
            let pos = p.pos + f32(sidx) * p.pitch;
            var pi = u32(pos);
            let fr = pos - f32(pi);

            if p.looping != 0u && p.l_end > p.l_start {
                let llen = p.l_end - p.l_start;
                if llen > 0u && pi >= p.l_end {
                    pi = p.l_start + (pi - p.l_start) % llen;
                }
            }

            if pi < p.s_end - 1u {
                let i0 = p.s_start + pi;
                let i1 = i0 + 1u;
                let s0 = samples[i0];
                let s1 = samples[i1];
                let sv = s0 + (s1 - s0) * fr;

                let lg = p.vol * sqrt(max(1.0 - p.pan, 0.0));
                let rg = p.vol * sqrt(max(1.0 + p.pan, 0.0));

                L += sv * lg;
                R += sv * rg;
            }
        }
        v += WGS;
    }

    sl[li] = L;
    sr[li] = R;

    workgroupBarrier();

    // thread 0 归约全部 256 个线程的贡献
    if li == 0u {
        var tL = 0.0f;
        var tR = 0.0f;
        for (var i = 0u; i < WGS; i++) {
            tL += sl[i];
            tR += sr[i];
        }
        out[sidx * 2u] = tL;
        out[sidx * 2u + 1u] = tR;
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::SHADER_SRC;

    #[test]
    fn validate_wgsl_shader() {
        // 编译期验证 WGSL 语法，不用等运行时才炸
        naga::front::wgsl::parse_str(SHADER_SRC).expect("WGSL shader 语法错误");
    }
}
