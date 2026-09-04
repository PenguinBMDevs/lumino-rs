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
use super::control::SharedControl;

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

    // ── 后端选择 ──
    /// 音频渲染后端（CPU / GPU）
    pub backend: AudioBackendKind,

    // ── 进度回调 ──
    /// 进度回调函数（可选）
    pub progress_callback: Option<ProgressCallback>,

    // ── 控制句柄（暂停/中止）──
    /// 导出控制句柄（暂停/中止），`None` 为无控制（测试/单次导出）
    pub control: Option<SharedControl>,
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
            .field("backend", &self.backend)
            .field(
                "progress_callback",
                &self.progress_callback.as_ref().map(|_| "..."),
            )
            .field("control", &self.control.as_ref().map(|_| "SharedControl"))
            .finish()
    }
}

/// 声道模式（映射到 xsynth ChannelCount）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioChannelMode {
    /// 单声道
    Mono,
    /// 双声道（立体声）
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
    /// 不开启多线程
    None,
    /// 自动选择线程数
    Auto,
    /// 手动指定线程数
    Manual(u32),
}

/// 插值算法（映射到 xsynth Interpolator）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioInterpolation {
    /// 最近邻插值
    Nearest,
    /// 线性插值
    Linear,
}

/// 音频渲染后端
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioBackendKind {
    /// CPU 后端（xsynth，默认）
    #[default]
    Cpu,
    /// GPU 后端（lumino-gpu-synth）
    Gpu,
}

impl std::fmt::Display for AudioBackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cpu => write!(f, "CPU"),
            Self::Gpu => write!(f, "GPU"),
        }
    }
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
    fn from(thread_mode: ThreadMode) -> Self {
        match thread_mode {
            ThreadMode::None => ThreadCount::None,
            ThreadMode::Auto => ThreadCount::Auto,
            ThreadMode::Manual(n) => ThreadCount::Manual(n as usize),
        }
    }
}

impl From<AudioInterpolation> for Interpolator {
    fn from(interp: AudioInterpolation) -> Self {
        match interp {
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
        // linear_envelope 为 true 时使用线性衰减/释音，匹配 OmniConverter 的 LinearEnvelope 模式
        // 否则保持指数曲线（默认，与旧行为一致）
        let vol_envelope_options = if self.linear_envelope {
            EnvelopeOptions {
                attack_curve: EnvelopeCurveType::Exponential,
                decay_curve: EnvelopeCurveType::Linear,
                release_curve: EnvelopeCurveType::Linear,
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

impl Default for AudioRenderConfig {
    fn default() -> Self {
        Self {
            midi_path: PathBuf::new(),
            soundfonts: Vec::new(),
            output_path: PathBuf::new(),
            // 与 UI 默认 48k 对齐（视频/音频通用），避免 UI 48k 与 config 44.1k 分歧导致语义困惑
            sample_rate: 48000,
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
            backend: AudioBackendKind::Cpu,
            progress_callback: None,
            control: None,
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
