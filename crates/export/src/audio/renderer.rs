//! 音频渲染引擎 — 封装 xsynth 的 ChannelGroup 提供批量渲染

use std::path::Path;

use xsynth_core::{
    AudioPipe, ChannelCount,
    channel_group::{ChannelGroup, SynthEvent},
    effects::VolumeLimiter,
};

use crate::error::ExportResult;

use super::{config::AudioRenderConfig, writer::AudioFileWriter};

/// 亚样点精度累加器与输出缓冲区
struct BatchBuffer {
    output_vec: Vec<f32>,
    missed_samples: f64,
}

/// 音频渲染引擎
///
/// 对应 xsynth-render 中的 `XSynthRender`。
/// 封装 `ChannelGroup`（合成引擎）+ `AudioFileWriter`（输出）+ `VolumeLimiter`（可选限幅器）。
pub struct AudioRenderer {
    channel_group: ChannelGroup,
    audio_writer: AudioFileWriter,
    limiter: Option<VolumeLimiter>,
    buffer: BatchBuffer,
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

        let audio_writer = AudioFileWriter::new(sample_rate, channel_count, path)?;

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
            sample_rate,
            channel_count,
        })
    }

    /// 返回当前音频流参数（用于加载 Soundfont）
    pub fn stream_params(&self) -> xsynth_core::AudioStreamParams {
        self.channel_group.stream_params().clone()
    }

    /// 发送 xsynth 事件
    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.send_event(event);
    }

    /// 渲染指定时长的音频（秒）
    ///
    /// 将 `event_time` 秒的音频渲染到输出缓冲区，经过限幅器（可选）后写入文件。
    /// 支持亚样点精度累加。超过 10 秒的块会递归拆分。
    pub fn render_batch(&mut self, event_time: f64) {
        if event_time > 10.0 {
            let mut remaining = event_time;
            while remaining > 10.0 {
                self.render_batch(10.0);
                remaining -= 10.0;
            }
            self.render_batch(remaining);
            return;
        }

        // 计算样点数（含亚样点累加）
        let samples_f = self.sample_rate as f64 * event_time + self.buffer.missed_samples;
        self.buffer.missed_samples = samples_f % 1.0;
        let samples = (samples_f as usize) * self.channel_count as usize;

        self.buffer.output_vec.resize(samples, 0.0);
        self.channel_group.read_samples(&mut self.buffer.output_vec);

        // 可应用限幅器
        if let Some(limiter) = &mut self.limiter {
            limiter.limit(&mut self.buffer.output_vec);
        }

        // 写入文件（忽略错误，由外部 finalize 统一处理）
        if let Err(e) = self.audio_writer.write_samples(&mut self.buffer.output_vec) {
            tracing::error!("渲染批次写入失败: {e}");
        }
    }

    /// 完成渲染：持续渲染尾部直到静音，然后关闭写入器
    pub fn finalize(mut self) -> ExportResult<()> {
        // 持续渲染 1 秒块，直到所有样点接近静音
        loop {
            let samples = self.sample_rate as usize * self.channel_count as usize;
            self.buffer.output_vec.resize(samples, 0.0);
            self.channel_group.read_samples(&mut self.buffer.output_vec);

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
