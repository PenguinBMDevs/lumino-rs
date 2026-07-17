//! 音频渲染配置 — 将 Lumino UI 状态转换为 xsynth 配置
//!
//! 参考 OmniConverter 的 Settings / RenderSettings 设计：
//! - 统一的配置结构，包含编码器、渲染器、事件处理等参数

use std::path::PathBuf;
use std::sync::Arc;

use xsynth_core::{
    AudioStreamParams, ChannelCount,
    channel::ChannelInitOptions,
    channel_group::{ChannelGroupConfig, ParallelismOptions, SynthFormat, ThreadCount},
    soundfont::{EnvelopeCurveType, EnvelopeOptions, Interpolator, SoundfontInitOptions},
};

use super::codec::AudioCodec;

/// 进度回调函数类型
pub type ProgressCallback = Arc<dyn Fn(String, f64) + Send + Sync>;

/// Lumino 侧的音频导出配置，用于构造 xsynth 和编码器参数
#[derive(Clone)]
pub struct AudioRenderConfig {
    /// MIDI 文件路径
    pub midi_path: PathBuf,
    /// SF2 音色库文件路径
    pub soundfonts: Vec<PathBuf>,
    /// 输出音频文件路径
    pub output_path: PathBuf,

    // ── xsynth 核心参数 ──
    /// 采样率（Hz）
    pub sample_rate: u32,
    /// 声道数
    pub channels: AudioChannelMode,
    /// 每通道最大层数（None = 不限）
    pub layer_limit: Option<usize>,
    /// 通道级多线程
    pub channel_threading: ThreadMode,
    /// 按键级多线程
    pub key_threading: ThreadMode,
    /// 插值算法
    pub interpolation: AudioInterpolation,
    /// 是否应用限制器
    pub apply_limiter: bool,
    /// 是否禁用杀死 voice 时的淡出
    pub disable_fade_out: bool,
    /// 是否使用线性包络（线性衰减/释音）
    pub linear_envelope: bool,

    // ── 编码器参数（参考 OmniConverter EncoderSettings） ──
    /// 输出音频编码器
    pub audio_codec: AudioCodec,
    /// 编码比特率（kbps，仅 MP3/Vorbis 使用）
    pub audio_bitrate: u32,

    // ── 事件处理参数（参考 OmniConverter EventSettings） ──
    /// 忽略音色变化事件
    pub ignore_program_changes: bool,
    /// 音符力度过滤 - 最低力度
    pub velocity_low: u8,
    /// 音符力度过滤 - 最高力度
    pub velocity_high: u8,
    /// 启用音符过滤
    pub filter_velocity: bool,
    /// 音符力度过滤 - 最低键位
    pub key_low: u8,
    /// 音符力度过滤 - 最高键位
    pub key_high: u8,
    /// 启用键位过滤
    pub filter_key: bool,
    /// 音符结束后额外延迟（毫秒）
    pub note_force_end_delay: u32,

    // ── 进度回调 ──
    /// 进度回调函数（可选）
    pub progress_callback: Option<ProgressCallback>,
}

impl std::fmt::Debug for AudioRenderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioRenderConfig")
            .field("midi_path", &self.midi_path)
            .field("soundfonts", &self.soundfonts)
            .field("output_path", &self.output_path)
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("layer_limit", &self.layer_limit)
            .field("channel_threading", &self.channel_threading)
            .field("key_threading", &self.key_threading)
            .field("interpolation", &self.interpolation)
            .field("apply_limiter", &self.apply_limiter)
            .field("disable_fade_out", &self.disable_fade_out)
            .field("linear_envelope", &self.linear_envelope)
            .field("audio_codec", &self.audio_codec)
            .field("audio_bitrate", &self.audio_bitrate)
            .field(
                "progress_callback",
                &self.progress_callback.as_ref().map(|_| "..."),
            )
            .finish()
    }
}

/// 声道模式（映射到 xsynth ChannelCount）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioChannelMode {
    Mono,
    Stereo,
}

impl AudioChannelMode {
    /// 返回该模式对应的声道数量。
    pub fn channel_count(self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

/// 线程模式（映射到 xsynth ThreadCount）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadMode {
    None,
    Auto,
    Manual(u32),
}

/// 插值算法（映射到 xsynth Interpolator）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioInterpolation {
    Nearest,
    Linear,
}

impl From<AudioChannelMode> for ChannelCount {
    fn from(m: AudioChannelMode) -> Self {
        match m {
            AudioChannelMode::Mono => ChannelCount::Mono,
            AudioChannelMode::Stereo => ChannelCount::Stereo,
        }
    }
}

impl From<ThreadMode> for ThreadCount {
    fn from(t: ThreadMode) -> Self {
        match t {
            ThreadMode::None => ThreadCount::None,
            ThreadMode::Auto => ThreadCount::Auto,
            ThreadMode::Manual(n) => ThreadCount::Manual(n as usize),
        }
    }
}

impl From<AudioInterpolation> for Interpolator {
    fn from(i: AudioInterpolation) -> Self {
        match i {
            AudioInterpolation::Nearest => Interpolator::Nearest,
            AudioInterpolation::Linear => Interpolator::Linear,
        }
    }
}

impl AudioRenderConfig {
    /// 构造 xsynth 的 ChannelGroupConfig
    pub fn build_group_config(&self) -> ChannelGroupConfig {
        let audio_params =
            AudioStreamParams::new(self.sample_rate, ChannelCount::from(self.channels));

        ChannelGroupConfig {
            channel_init_options: ChannelInitOptions {
                fade_out_killing: !self.disable_fade_out,
            },
            format: SynthFormat::Midi,
            audio_params,
            parallelism: ParallelismOptions {
                channel: ThreadCount::from(self.channel_threading),
                key: ThreadCount::from(self.key_threading),
            },
        }
    }

    /// 构造 xsynth 的 SoundfontInitOptions
    pub fn build_sf_options(&self) -> SoundfontInitOptions {
        let vol_envelope_options = if self.linear_envelope {
            EnvelopeOptions {
                attack_curve: EnvelopeCurveType::Exponential,
                decay_curve: EnvelopeCurveType::Exponential,
                release_curve: EnvelopeCurveType::Exponential,
            }
        } else {
            EnvelopeOptions {
                attack_curve: EnvelopeCurveType::Exponential,
                decay_curve: EnvelopeCurveType::Exponential,
                release_curve: EnvelopeCurveType::Exponential,
            }
        };

        SoundfontInitOptions {
            bank: None,
            preset: None,
            vol_envelope_options,
            use_effects: true,
            interpolator: Interpolator::from(self.interpolation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_channel_mode_channel_count() {
        assert_eq!(AudioChannelMode::Mono.channel_count(), 1);
        assert_eq!(AudioChannelMode::Stereo.channel_count(), 2);
    }

    #[test]
    fn test_audio_channel_mode_into_channel_count() {
        let mono = AudioChannelMode::Mono;
        let stereo = AudioChannelMode::Stereo;
        assert_eq!(ChannelCount::from(mono).count(), 1);
        assert_eq!(ChannelCount::from(stereo).count(), 2);
    }
}

impl Default for AudioRenderConfig {
    fn default() -> Self {
        Self {
            midi_path: PathBuf::new(),
            soundfonts: Vec::new(),
            output_path: PathBuf::new(),
            sample_rate: 44100,
            channels: AudioChannelMode::Stereo,
            layer_limit: None,
            channel_threading: ThreadMode::Auto,
            key_threading: ThreadMode::None,
            interpolation: AudioInterpolation::Linear,
            apply_limiter: false,
            disable_fade_out: false,
            linear_envelope: false,
            audio_codec: AudioCodec::Pcm,
            audio_bitrate: 320,
            ignore_program_changes: false,
            velocity_low: 0,
            velocity_high: 127,
            filter_velocity: false,
            key_low: 0,
            key_high: 127,
            filter_key: false,
            note_force_end_delay: 0,
            progress_callback: None,
        }
    }
}
