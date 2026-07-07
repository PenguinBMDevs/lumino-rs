//! GpuSynth — GPU 合成器高阶 API
//!
//! 负责 SF2 加载、voice params 的 CPU 侧管理（position 推进、release_elapsed 累计）、
//! 以及异步 submit/readback 编排。

use super::renderer::PendingRender;
use super::{GPU_BLOCK_SAMPLES, MAX_VOICES};
use super::{GpuRenderer, GpuVoiceParams, RawEvent};
use bytemuck::Zeroable;
use std::path::Path;
use xsynth_soundfonts::sf2::load_soundfont_with_samples;

// ── GpuSynth ────────────────────────────────────────
pub(crate) struct GpuSynth {
    gpu: GpuRenderer,
    sample_rate: u32,
    /// GPU 端管理的 voice params 的 CPU 副本（初始为零，每块读回覆盖）
    _params: Vec<GpuVoiceParams>,
    /// GPU_BLOCK_SAMPLES 常量
    block_samples: u32,
    /// 预分配的 params staging buffer（避免 readback_params 每块创建临时 buffer）
    buf_params_staging: wgpu::Buffer,
}

impl GpuSynth {
    pub(crate) fn new(
        sf2_path: &Path,
        sample_rate: u32,
        channels: u16,
        max_voices: u32,
    ) -> Result<Self, String> {
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
        let mut regions: Vec<super::GpuRegion> = Vec::new();
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
                regions.push(super::GpuRegion {
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

        let gpu = GpuRenderer::new(
            device,
            queue,
            &flat,
            &regions,
            channels,
            sample_rate,
            max_voices,
        )?;
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
            block_samples: GPU_BLOCK_SAMPLES,
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
    /// 然后推进每个活跃 voice 的 position（render shader 渲染了 ns 个输出样本）。
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

        // [Bug Fix] 推进 position：render shader 已经处理了 ns 个输出样本，
        // 每个 voice 实际播放了 (ns - sf) 个样本。对于持续 voice (sf=0)
        // 推进 ns * pitch；对于新 voice (sf=to) 推进 (ns - to) * pitch。
        let ns = self.block_samples;
        for v in self.gpu.params_cpu.iter_mut() {
            if v.enabled != 0 {
                let played = ns.saturating_sub(v.start_frame);
                v.position += played as f32 * v.pitch_ratio;
                v.start_frame = 0; // 下 block 不再有 start_frame 门控

                if v.released != 0 {
                    // [Bug Fix] 跨块 release envelope 连续：累加 release 开始后
                    // 渲染的 sample 数，避免跨块 saturating_sub 导致 envelope 重启。
                    // release_start = 本块内 release 开始位置（rf 和 sf 取大值）
                    let release_start = v.release_frame.max(v.start_frame);
                    let release_rendered = ns.saturating_sub(release_start);
                    v.release_elapsed += release_rendered as f32;

                    // [Bug Fix] 跨块 release_frame 衰减：release_frame 是原始 block 内的
                    // sample offset，下一块 render shader 比较 sidx >= rf 会不命中。
                    // 减去 ns 后衰减为块内等效偏移。
                    v.release_frame = v.release_frame.saturating_sub(ns);

                    // [Bug Fix] release envelope 衰减到阈值以下后主动释放 voice slot，
                    // 避免 silent voice 永久占用 slot 导致新音符被丢弃。
                    const RELEASE_CUTOFF: f32 = 0.001;
                    const RELEASE_COEF: f32 = 0.999;
                    let env = RELEASE_COEF.powf(v.release_elapsed);
                    if env < RELEASE_CUTOFF {
                        *v = GpuVoiceParams::zeroed();
                    }
                }
            }
        }
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
