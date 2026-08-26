//! Error types for the crate.

use thiserror::Error;

/// Errors that can occur while loading, configuring or rendering with the
/// synthesizer.
#[derive(Debug, Error)]
pub enum SynthError {
    /// The GPU adapter/device could not be created (no suitable adapter or
    /// the device failed to initialize).
    #[error("failed to initialize the GPU device: {0}")]
    GpuInit(String),

    /// A wgpu resource (buffer, pipeline, shader) could not be created.
    #[error("wgpu error: {0}")]
    Gpu(String),

    /// The soundfont could not be parsed or loaded.
    #[error("soundfont error: {0}")]
    SoundFont(#[from] SoundFontError),

    /// The MIDI file could not be parsed.
    #[error("MIDI parse error: {0}")]
    Midi(String),

    /// The requested configuration is invalid (e.g. unsupported sample rate).
    #[error("invalid configuration: {0}")]
    Config(String),

    /// An I/O error occurred (reading files, writing WAV output...).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The number of concurrently active voices exceeded `max_voices`.
    #[error("voice limit exceeded: {0} active voices")]
    VoiceLimit(usize),

    /// Offline rendering exceeded the maximum allowed length. This happens
    /// when a voice never finishes (e.g. a held damper pedal, a missing
    /// note-off at the end of the MIDI file, or an envelope stage that
    /// cannot terminate), which would otherwise loop forever.
    #[error(
        "render timed out after {frames} frames: {active_voices} voices still active, last block peak {last_peak}"
    )]
    RenderTimeout {
        /// Frames rendered before the timeout fired.
        frames: u64,
        /// Number of voices that were still active at the timeout.
        active_voices: usize,
        /// Peak sample magnitude of the last rendered block.
        last_peak: f32,
    },
}

/// Errors that can occur while parsing or preparing a soundfont.
#[derive(Debug, Error)]
pub enum SoundFontError {
    /// The file could not be parsed as a supported soundfont.
    #[error("failed to parse soundfont: {0}")]
    Parse(String),

    /// The requested bank/preset does not exist in the soundfont.
    #[error("bank {0} preset {1} not found in soundfont")]
    MissingPreset(u16, u16),

    /// A sample could not be resampled to the output rate.
    #[error("sample resampling failed: {0}")]
    Resample(String),
}
