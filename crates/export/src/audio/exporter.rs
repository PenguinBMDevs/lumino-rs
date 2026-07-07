//! 音频导出器——核心合成引擎封装

use std::sync::Arc;

use xsynth_core::channel::{ChannelConfigEvent, ChannelEvent};
use xsynth_core::channel_group::{ChannelGroup, ChannelGroupConfig, SynthEvent};
use xsynth_core::effects::VolumeLimiter;
use xsynth_core::soundfont::SoundfontBase;
use xsynth_core::{AudioPipe, AudioStreamParams, ChannelCount};

use crate::error::ExportResult;

use super::types::{AudioChannels, AudioExportOptions, MAX_RENDER_CHUNK_SECONDS};
use super::writer::AudioFileWriter;

/// 音频导出器
pub struct AudioExporter {
    channel_group: ChannelGroup,
    limiter: Option<VolumeLimiter>,
    sample_rate: u32,
    channels: AudioChannels,
    output_vec: Vec<f32>,
    missed_samples: f64,
}

impl AudioExporter {
    /// 创建新的音频导出器
    pub fn new(options: &AudioExportOptions, soundfont: Arc<dyn SoundfontBase>) -> Self {
        let audio_params = AudioStreamParams::new(options.sample_rate, ChannelCount::Stereo);

        // 配置 xsynth 初始化选项：将 lumino 选项映射到 xsynth 参数
        let channel_init_options = xsynth_core::channel::ChannelInitOptions {
            // disable_fade_out=false → fade_out_killing=true（启用淡出杀音）
            fade_out_killing: !options.disable_fade_out,
            // layers 默认为 8，限制每键最大 voice 数
            max_voices_per_key: Some(options.layers as usize),
            // 全局 voice 数量限制：None = 使用 xsynth 默认 (4096/通道)
            // 之前的实现: ((128*layers).max(4096)/16).max(128) = 256/通道
            // 这个值过小，4160 个音符在单通道上轻松突破 256 → 新 NoteOn 被静默丢弃
            // 这就是"漏音"的根因。
            global_voice_limit: None,
        };

        let group_options = ChannelGroupConfig {
            channel_init_options,
            format: xsynth_core::channel_group::SynthFormat::Midi,
            audio_params,
            parallelism: xsynth_core::channel_group::ParallelismOptions::default(),
        };

        let channel_group = ChannelGroup::new(group_options);

        let limiter = if options.apply_limiter {
            Some(VolumeLimiter::new(options.channels.count()))
        } else {
            None
        };

        let mut exporter = Self {
            channel_group,
            limiter,
            sample_rate: options.sample_rate,
            channels: options.channels,
            output_vec: Vec::new(),
            missed_samples: 0.0,
        };

        // 设置音色库
        exporter
            .channel_group
            .send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                ChannelConfigEvent::SetSoundfonts(vec![soundfont]),
            )));

        exporter
    }

    /// 发送合成事件
    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.send_event(event);
    }

    /// 渲染指定时间的音频样本
    /// 限制单次最大块大小为 MAX_RENDER_CHUNK_SECONDS，防止大块内存分配导致 OOM。
    pub fn render_batch(&mut self, event_time: f64) {
        let mut remaining = event_time;
        while remaining > 0.0 {
            let chunk = remaining.min(MAX_RENDER_CHUNK_SECONDS);
            remaining -= chunk;

            let samples = self.sample_rate as f64 * chunk + self.missed_samples;
            self.missed_samples = samples % 1.0;
            let samples = samples as usize * self.channels.count() as usize;

            self.output_vec.resize(samples, 0.0);
            self.channel_group.read_samples(&mut self.output_vec);

            if let Some(limiter) = &mut self.limiter {
                limiter.limit(&mut self.output_vec);
            }
        }
    }

    /// 获取当前渲染的样本引用
    pub fn get_samples(&self) -> &[f32] {
        &self.output_vec
    }

    /// 取出当前渲染的样本（避免克隆），用空 Vec 替换内部缓冲区
    pub fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.output_vec)
    }

    /// 完成渲染，将剩余衰减样本直接写入 writer
    /// 使用固定小块大小（FINALIZE_CHUNK）避免大块内存分配。
    pub fn finalize(&mut self, writer: &mut AudioFileWriter) -> ExportResult<()> {
        const FINALIZE_CHUNK: usize = 4096;

        loop {
            let chunk_frames = FINALIZE_CHUNK * self.channels.count() as usize;
            self.output_vec.resize(chunk_frames, 0.0);
            self.channel_group.read_samples(&mut self.output_vec);

            if let Some(limiter) = &mut self.limiter {
                limiter.limit(&mut self.output_vec);
            }

            let mut is_empty = true;
            for &s in &self.output_vec {
                if s.abs() > 0.0001 {
                    is_empty = false;
                    break;
                }
            }

            if is_empty {
                break;
            }

            writer.write_samples(&self.output_vec)?;
        }

        Ok(())
    }

    /// 获取活跃 voice 数量
    pub fn voice_count(&self) -> u64 {
        self.channel_group.voice_count()
    }
}
