//! # lumino-gpu-synth
//!
//! GPU-accelerated MIDI audio synthesis and rendering (SF2 soundfonts).
//!
//! This crate renders MIDI events into audio samples using **wgpu compute
//! shaders** (WGSL). Sample playback, interpolation (linear or 64-point
//! windowed sinc), volume envelopes, resonant low-pass filtering and stereo
//! mixing are all executed on the GPU, while the CPU keeps track of the MIDI
//! timeline, voice allocation and per-block parameter updates.
//!
//! ## Feature overview
//!
//! - **Offline rendering**: render a complete MIDI file to a WAV file (or to
//!   raw samples) with [`GpuSynth::render_midi_file`].
//! - **Realtime playback**: play live MIDI input through the default audio
//!   device with [`AudioPlayback`] (a `cpal`-backed helper that owns the
//!   engine on a render thread). The device sample rate is negotiated
//!   automatically; when it differs from the engine's, the output is
//!   linearly resampled.
//! - **SF2 soundfonts**: parsed with `xsynth-soundfonts`, so the synthesis
//!   model matches the XSynth engine (volume envelopes, cutoff filter,
//!   velocity/key modulation curves, stereo pan).
//! - **GPU-side interpolation**: linear (XSynth-compatible) or high-quality
//!   64-point windowed sinc, both implemented as WGSL compute kernels.
//!
//! ## Quick start
//!
//! ```no_run
//! use lumino_gpu_synth::{GpuSynth, SynthConfig};
//!
//! let mut synth = GpuSynth::new(SynthConfig::default())?;
//! synth.load_soundfont("assets/test.sf2", 0, 0)?;
//! let result = synth.render_midi_file("assets/right-example.mid")?;
//! lumino_gpu_synth::audio::wav::write_f32_wav(
//!     "out.wav",
//!     &result.samples,
//!     result.sample_rate,
//! )?;
//! # Ok::<(), lumino_gpu_synth::SynthError>(())
//! ```
//!
//! ## Design
//!
//! The crate is split into the following layers:
//!
//! - [`midi`]: MIDI file parsing (`lumino-midly`) and sample-accurate event
//!   scheduling.
//! - [`soundfont`]: SF2 loading (`xsynth-soundfonts`) and lazy sample
//!   resampling into the output sample rate.
//! - [`synth`]: the `GpuSynth` engine - CPU-side voice state machine and
//!   DSP parameter computation.
//! - [`gpu`]: wgpu device management and the WGSL compute pipelines.
//! - [`audio`]: WAV I/O and realtime playback helpers.
//! - [`compare`]: waveform comparison metrics used for validation.
//!
//! The synthesis behaviour intentionally follows the XSynth engine so that
//! renders are comparable to XSynth output (see [`synth::dsp`] for the exact
//! DSP formulas).

pub mod audio;
pub mod compare;
pub mod config;
pub mod error;
pub mod gpu;
pub mod midi;
pub mod soundfont;
pub mod synth;

pub use audio::playback::AudioPlayback;
pub use audio::wav;
pub use config::{ChannelMode, InterpolationMode, SynthConfig};
pub use error::{SoundFontError, SynthError};
pub use midi::parser::MidiFile;
pub use midi::stream::MidiStream;
pub use soundfont::SoundFont;
pub use synth::RenderResult;
pub use synth::engine::GpuSynth;

/// Re-exported convenience version of the crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
