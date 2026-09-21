//! DSP formulas shared by the CPU parameter computation and the WGSL kernels.
//!
//! Every formula here mirrors the XSynth engine exactly, so that a render
//! produced by this crate is comparable sample-by-sample to an XSynth render
//! (and therefore to the reference audio in the acceptance test):
//!
//! - pitch: `speed_mult = 2^(cents/1200)`
//! - volume envelope: 7 stages with linear / `f^8` convex / `(1-f)^8`
//!   concave curves, stage boundaries joining at the last emitted value
//! - resonant low-pass: RBJ cookbook biquad (Direct Form 1)
//! - 64-point windowed sinc interpolation table (Blackman-Harris window)

mod envelope;
mod filter;
mod sinc;

pub use envelope::{
    CurveKind, EnvStage, EnvelopeCurveConfig, EnvelopeDescriptor, GpuEnvelope, eval_stage,
    modify_env_stages, to_gpu_stages,
};
pub use filter::{BiquadDf1, biquad_lowpass_coeffs, resonance_to_q};
pub use sinc::{SINC_PHASES, SINC_TAPS, build_sinc_table};

/// `2^(cents/1200)` - the XSynth pitch multiplier.
pub fn cents_factor(cents: f32) -> f32 {
    2.0f32.powf(cents / 1200.0)
}

/// dB to amplitude (`10^(db/20)`).
pub fn db_to_amp(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests;
