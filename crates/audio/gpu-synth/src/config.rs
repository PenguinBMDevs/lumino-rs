//! Configuration types for the synthesizer.

use crate::synth::dsp::EnvelopeCurveConfig;

/// The sample interpolation algorithm used inside the GPU render kernels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InterpolationMode {
    /// Linear interpolation between adjacent samples.
    ///
    /// This matches the XSynth engine used to produce the reference audio
    /// and is therefore the default.
    #[default]
    Linear,
    /// High quality 64-point windowed sinc interpolation.
    ///
    /// This uses a precomputed 64-tap Blackman-Harris windowed sinc table and
    /// is the highest quality mode; it is slightly more expensive on the GPU.
    Point64Sinc,
}

/// Output channel layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    /// Stereo output (interleaved L/R samples).
    #[default]
    Stereo,
    /// Mono output (sum of both channels is down-mixed).
    Mono,
}

impl ChannelMode {
    /// The number of output channels.
    pub fn channel_count(self) -> usize {
        match self {
            ChannelMode::Stereo => 2,
            ChannelMode::Mono => 1,
        }
    }
}

/// Configuration for a [`crate::GpuSynth`] instance.
///
/// # Example
///
/// ```
/// use lumino_gpu_synth::{InterpolationMode, SynthConfig};
///
/// let config = SynthConfig {
///     sample_rate: 64_000,
///     ..SynthConfig::default()
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SynthConfig {
    /// Output sample rate in Hz.
    ///
    /// Default: `64_000` (matches the reference render).
    pub sample_rate: u32,

    /// Maximum number of concurrently active voices.
    ///
    /// This is the GPU voice pool size AND the polyphony ceiling: when the
    /// number of sounding voices would exceed it, the oldest note groups
    /// are faded out (XSynth's `global_voice_limit` + voice stealing) so a
    /// fresh note always sounds.
    ///
    /// Set to `0` for **unlimited** polyphony (black-MIDI mode) — no global
    /// trimming is performed and the GPU buffers grow on demand; the only
    /// remaining limit is the physical `MAX_VOICE_OUT_BYTES` batch window
    /// (handled by chunked dispatch). This is the recommended setting for
    /// black MIDI where every note must sound.
    ///
    /// The default (4096, like XSynth's) keeps the simultaneous-voice noise
    /// floor low: N voices mix with ~sqrt(N) noise density, so 16k voices
    /// sound like white noise even when the peak is limited. Raise it only
    /// if a specific file really needs more simultaneous notes.
    ///
    /// Default: `0` (unlimited, black-MIDI mode). Set e.g. `4096` to cap.
    pub max_voices: usize,

    /// Maximum number of simultaneously sounding note *groups* for the same
    /// key **within one MIDI port** - all 16 channels of the port share the
    /// cap (REND-002 #87 / REND-008 #94 semantics).
    ///
    /// Release tails count toward the cap and are evicted first (REND-011
    /// #105): over-cap trimming fades tails, then the quietest/oldest
    /// sustained groups; the newest group is always protected. `0` disables
    /// the limit entirely.
    ///
    /// Default: `8` (XSynth uses 4; 8 keeps fast trills/rolls clean).
    pub max_voices_per_key: usize,

    /// 全局 MIDI 通道空间大小（必须是 16 的倍数，16..=256）。
    ///
    /// `16` = 单端口（历史行为，零变化）；多端口 =
    /// `(实际使用端口数上限+1)*16`，事件通道按 `(port, ch) → port*16+ch`
    /// 映射（REND-002 #87）。默认 16。
    pub midi_channels: usize,

    /// Number of audio frames rendered per GPU dispatch (per channel).
    ///
    /// Must be a power of two and at least 16. Smaller blocks keep the
    /// per-block voice population (and therefore upload/GPU cost) low for
    /// dense MIDI; larger blocks amortize dispatch overhead. Default: `1024`.
    pub block_size: usize,

    /// Sample interpolation mode used by the GPU render kernel.
    ///
    /// Default: [`InterpolationMode::Linear`].
    pub interpolation: InterpolationMode,

    /// Whether voice processing effects (the resonant low-pass filter) are
    /// enabled, mirroring XSynth's `use_effects` option.
    ///
    /// Default: `true`.
    pub use_effects: bool,

    /// Envelope curve selection for the volume envelope stages, mirroring
    /// XSynth's `EnvelopeOptions`. Set `decay_curve`/`release_curve` to
    /// `CurveKind::Exponential` to match OmniConverter's "LinearEnvelope"
    /// mode.
    ///
    /// Default: XSynth defaults (attack Exponential, decay/release Linear).
    pub envelope_curves: EnvelopeCurveConfig,

    /// Output channel layout. Default: [`ChannelMode::Stereo`].
    pub channels: ChannelMode,

    /// The absolute silence threshold (per sample) used by offline rendering
    /// to decide when the tail has decayed and rendering can stop. Mirrors
    /// XSynth's offline renderer (`0.0001`).
    ///
    /// Default: `0.0001`.
    pub render_silence_threshold: f32,

    /// Maximum number of seconds to keep rendering *after* the last MIDI
    /// event before aborting with [`crate::SynthError::RenderTimeout`].
    ///
    /// This is a safety valve against infinite offline renders caused by
    /// voices that can never finish (a held damper pedal, a missing note-off
    /// at the end of the file, a zero-duration envelope stage...). It does
    /// not limit legitimate files: a normal render ends as soon as the
    /// output goes silent, which is always well before this budget.
    ///
    /// Default: `120.0` seconds.
    pub max_tail_seconds: f32,

    /// Whether offline rendering prints a progress bar to stderr.
    ///
    /// Used by the render examples so long exports are visibly alive; the
    /// bar is a single `\r`-rewritten line, so it never floods the log.
    ///
    /// Default: `false` (library callers stay quiet).
    pub show_progress: bool,
}

impl Default for SynthConfig {
    fn default() -> Self {
        Self {
            sample_rate: 64_000,
            max_voices: 0,         // 0 = unlimited — black MIDI must never drop a voice
            max_voices_per_key: 4, // 4 per (ch,key) as requested
            midi_channels: 16,
            block_size: 512,
            interpolation: InterpolationMode::Linear,
            use_effects: true,
            envelope_curves: EnvelopeCurveConfig::default(),
            channels: ChannelMode::Stereo,
            render_silence_threshold: 0.0001,
            max_tail_seconds: 120.0,
            show_progress: false,
        }
    }
}

impl SynthConfig {
    /// Validates the configuration and returns a descriptive error if it is
    /// unusable.
    pub fn validate(&self) -> Result<(), crate::SynthError> {
        if self.sample_rate == 0 {
            return Err(crate::SynthError::Config(
                "sample_rate must be non-zero".into(),
            ));
        }
        if self.max_voices > 1_000_000 {
            return Err(crate::SynthError::Config(format!(
                "max_voices must be within 0..=1_000_000 (0 = unlimited), got {}",
                self.max_voices
            )));
        }
        if self.midi_channels < 16
            || self.midi_channels > 256
            || !self.midi_channels.is_multiple_of(16)
        {
            return Err(crate::SynthError::Config(format!(
                "midi_channels must be a multiple of 16 within 16..=256, got {}",
                self.midi_channels
            )));
        }
        if !self.block_size.is_power_of_two() || self.block_size < 16 {
            return Err(crate::SynthError::Config(format!(
                "block_size must be a power of two >= 16, got {}",
                self.block_size
            )));
        }
        // DEBT-04 #121：GPU 渲染管线（shader/readback/limiter）按立体声交错布局
        // 硬编码；Mono 在此层无法正确产出。需要单声道请在导出/上层做真降混
        // （0.5*(L+R)），不要打开这个开关——旧实现静默产出错误音频。
        if self.channels == ChannelMode::Mono {
            return Err(crate::SynthError::Config(
                "GPU 引擎仅支持立体声（ChannelMode::Stereo）；单声道请在写出前降混".into(),
            ));
        }
        if !self.render_silence_threshold.is_finite() || self.render_silence_threshold <= 0.0 {
            return Err(crate::SynthError::Config(
                "render_silence_threshold must be positive".into(),
            ));
        }
        if !self.max_tail_seconds.is_finite() || self.max_tail_seconds <= 0.0 {
            return Err(crate::SynthError::Config(
                "max_tail_seconds must be positive".into(),
            ));
        }
        Ok(())
    }
}
