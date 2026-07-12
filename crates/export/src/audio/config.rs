//! 音频渲染配置 — 将 Lumino UI 状态转换为 xsynth 配置

use std::path::PathBuf;
use std::sync::Arc;

use xsynth_core::{
    AudioStreamParams, ChannelCount,
    channel::ChannelInitOptions,
    channel_group::{ChannelGroupConfig, ParallelismOptions, SynthFormat, ThreadCount},
    soundfont::{EnvelopeCurveType, EnvelopeOptions, Interpolator, SoundfontInitOptions},
};

/// 进度回调函数类型
pub type ProgressCallback = Arc<dyn Fn(String, f64) + Send + Sync>;

/// Lumino 侧的音频导出 UI 状态快照，用于构造 xsynth 配置
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
    /// 是否使用 GPU 加速渲染（默认打开）
    pub use_gpu: bool,

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
            .field("use_gpu", &self.use_gpu)
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
                decay_curve: EnvelopeCurveType::Linear,
                release_curve: EnvelopeCurveType::Linear,
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
