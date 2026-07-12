//! 音频渲染引擎 — 封装 xsynth 的 ChannelGroup 提供批量渲染
//!
//! # 渲染加速策略（自 OmniConverter xsynth）
//!
//! - **免零填充**: 使用 `read_samples_unchecked` 替代 `read_samples`，避免不必要的零填充
//! - **Vec 回收**: 写入线程消费完后将 Vec 归还，渲染线程复用，减少重复分配
//! - **扁平批处理**: 用固定大小批次的循环替代递归拆分，消除递归开销
//! - **并行合成**: 通过 `ChannelGroup` 的 `ParallelismOptions` 实现通道级/按键级并行

use std::path::Path;

use crossbeam_channel::bounded;
use xsynth_core::{
    AudioPipe, ChannelCount,
    channel_group::{ChannelGroup, SynthEvent},
    effects::VolumeLimiter,
};

use crate::error::ExportResult;

use super::{config::AudioRenderConfig, writer::AudioFileWriter};

/// 最大单批次渲染时长（秒）。超过此值则拆分为多个批次。
const MAX_BATCH_SECONDS: f64 = 10.0;

/// 亚样点精度累加器
struct BatchBuffer {
    output_vec: Vec<f32>,
    missed_samples: f64,
}

/// 音频渲染引擎
///
/// 对应 xsynth-render 中的 `XSynthRender`。
/// 封装 `ChannelGroup`（合成引擎）+ `AudioFileWriter`（输出）+ `VolumeLimiter`（可选限幅器）。
///
/// # 渲染加速
///
/// 与实时合成器不同，离线导出模式没有实时性约束，因此可以采用更激进的优化策略：
/// - 使用 `read_samples_unchecked` 跳过零填充（`read_samples` 会先零初始化缓冲区）
/// - Vec 回收池减少内存分配/释放开销
/// - 扁平循环替代递归，消除栈帧开销
/// - 通过 `ParallelismOptions` 启用通道级并行
pub struct AudioRenderer {
    channel_group: ChannelGroup,
    audio_writer: AudioFileWriter,
    limiter: Option<VolumeLimiter>,
    buffer: BatchBuffer,
    /// Vec 回收通道接收端（从写入线程回收）
    vec_recycle: crossbeam_channel::Receiver<Vec<f32>>,
    sample_rate: u32,
    channel_count: u16,
}

impl AudioRenderer {
    /// 创建新的渲染引擎
    ///
    /// 初始化 xsynth ChannelGroup 和 WAV 写入器。
    /// `config` 仅用于构造参数，不持有。
    pub fn new(config: &AudioRenderConfig, path: &Path) -> ExportResult<Self> {
        let group_config = config.build_group_config();
        let channel_count = ChannelCount::from(config.channels).count();
        let sample_rate = config.sample_rate;

        let channel_group = ChannelGroup::new(group_config.clone());

        // 创建 Vec 回收通道：写入线程消费完 Vec 后发回，本线程复用
        let (vec_recycle_tx, vec_recycle_rx) = bounded::<Vec<f32>>(2);
        let audio_writer = AudioFileWriter::new(sample_rate, channel_count, path, vec_recycle_tx)?;

        // 构建限幅器
        let limiter = if config.apply_limiter {
            Some(VolumeLimiter::new(channel_count))
        } else {
            None
        };

        Ok(Self {
            channel_group,
            audio_writer,
            limiter,
            buffer: BatchBuffer {
                output_vec: Vec::with_capacity(4096),
                missed_samples: 0.0,
            },
            vec_recycle: vec_recycle_rx,
            sample_rate,
            channel_count,
        })
    }

    /// 返回当前音频流参数（用于加载 Soundfont）
    pub fn stream_params(&self) -> xsynth_core::AudioStreamParams {
        *self.channel_group.stream_params()
    }

    /// 发送 xsynth 事件
    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.send_event(event);
    }

    /// 渲染指定时长的音频（秒）
    ///
    /// 将 `event_time` 秒的音频渲染到输出缓冲区，经过限幅器（可选）后写入文件。
    /// 支持亚样点精度累加。超过 [`MAX_BATCH_SECONDS`] 的块自动拆分为多个批次。
    ///
    /// # 性能
    ///
    /// - 使用 `read_samples_unchecked` 避免零填充
    /// - 优先从回收池获取 Vec，减少分配
    /// - 扁平循环替代递归，消除栈帧开销
    pub fn render_batch(&mut self, event_time: f64) {
        let mut remaining = event_time;

        while remaining > 0.0 {
            let batch = remaining.min(MAX_BATCH_SECONDS);

            // 计算样点数（含亚样点累加）
            let samples_f = self.sample_rate as f64 * batch + self.buffer.missed_samples;
            self.buffer.missed_samples = samples_f % 1.0;
            let samples = (samples_f as usize) * self.channel_count as usize;

            // 优先从回收池获取 Vec，避免重新分配
            self.buffer.output_vec = self
                .vec_recycle
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(samples));

            // 确保容量足够
            if self.buffer.output_vec.capacity() < samples {
                self.buffer
                    .output_vec
                    .reserve(samples - self.buffer.output_vec.capacity());
            }
            // SAFETY: `read_samples_unchecked` 会填充 `samples` 个样本，
            // 覆盖整个缓冲区，无需零初始化。
            unsafe {
                self.buffer.output_vec.set_len(samples);
            }

            self.channel_group
                .read_samples_unchecked(&mut self.buffer.output_vec);

            // 可应用限幅器
            if let Some(limiter) = &mut self.limiter {
                limiter.limit(&mut self.buffer.output_vec);
            }

            // 写入文件（忽略错误，由外部 finalize 统一处理）
            if let Err(e) = self.audio_writer.write_samples(&mut self.buffer.output_vec) {
                tracing::error!("渲染批次写入失败: {e}");
            }

            remaining -= batch;
        }
    }

    /// 完成渲染：持续渲染尾部直到静音，然后关闭写入器
    pub fn finalize(mut self) -> ExportResult<()> {
        // 持续渲染 1 秒块，直到所有样点接近静音
        loop {
            let samples = self.sample_rate as usize * self.channel_count as usize;

            // 优先从回收池获取 Vec
            self.buffer.output_vec = self
                .vec_recycle
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(samples));

            if self.buffer.output_vec.capacity() < samples {
                self.buffer
                    .output_vec
                    .reserve(samples - self.buffer.output_vec.capacity());
            }
            // SAFETY: `read_samples_unchecked` 会填充 `samples` 个样本
            unsafe {
                self.buffer.output_vec.set_len(samples);
            }

            self.channel_group
                .read_samples_unchecked(&mut self.buffer.output_vec);

            if let Some(limiter) = &mut self.limiter {
                limiter.limit(&mut self.buffer.output_vec);
            }

            // 检测是否静音
            let is_empty = self.buffer.output_vec.iter().all(|&s| s.abs() < 0.0001);

            if let Err(e) = self.audio_writer.write_samples(&mut self.buffer.output_vec) {
                tracing::error!("尾部渲染写入失败: {e}");
                break;
            }

            if is_empty {
                break;
            }
        }

        self.audio_writer.finalize()
    }

    /// 返回当前活跃 voice 数
    pub fn voice_count(&self) -> u64 {
        self.channel_group.voice_count()
    }
}
