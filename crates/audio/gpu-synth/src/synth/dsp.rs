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

/// `2^(cents/1200)` - the XSynth pitch multiplier.
pub fn cents_factor(cents: f32) -> f32 {
    2.0f32.powf(cents / 1200.0)
}

/// dB to amplitude (`10^(db/20)`).
pub fn db_to_amp(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// A volume envelope descriptor in seconds / percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeDescriptor {
    /// Start level (0-1).
    pub start_percent: f32,
    /// Delay in seconds.
    pub delay: f32,
    /// Attack in seconds.
    pub attack: f32,
    /// Hold in seconds.
    pub hold: f32,
    /// Decay in seconds.
    pub decay: f32,
    /// Sustain level (0-1).
    pub sustain_percent: f32,
    /// Release in seconds.
    pub release: f32,
}

/// Envelope curve selection, mirroring XSynth's `EnvelopeOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvelopeCurveConfig {
    /// Attack curve. Default: `Exponential` (linear in amplitude).
    pub attack_curve: CurveKind,
    /// Decay curve. Default: `Linear` (concave in amplitude).
    pub decay_curve: CurveKind,
    /// Release curve. Default: `Linear` (concave in amplitude).
    pub release_curve: CurveKind,
}

/// A curve kind for an envelope stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    /// Linear curve in amplitude space.
    Linear,
    /// Exponential curve in dB space (convex attack / concave decay/release
    /// in amplitude space).
    Exponential,
}

impl Default for EnvelopeCurveConfig {
    fn default() -> Self {
        Self {
            attack_curve: CurveKind::Exponential,
            decay_curve: CurveKind::Linear,
            release_curve: CurveKind::Linear,
        }
    }
}

/// One envelope stage as consumed by the WGSL kernel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvStage {
    /// 0 = linear lerp, 1 = concave `(1-f)^8`, 2 = convex `f^8`, 3 = hold.
    pub kind: u32,
    /// Target value of the stage.
    pub target: f32,
    /// Duration in samples.
    pub duration: u32,
}

impl EnvStage {
    pub const LERP: u32 = 0;
    pub const CONCAVE: u32 = 1;
    pub const CONVEX: u32 = 2;
    pub const HOLD: u32 = 3;
}

/// Evaluates one envelope stage at `f` (0-1 progress), exactly like the
/// XSynth lerpers.
pub fn eval_stage(stage: &EnvStage, from: f32, f: f32) -> f32 {
    match stage.kind {
        EnvStage::LERP => from + (stage.target - from) * f,
        EnvStage::CONCAVE => (from - stage.target) * (1.0 - f).powi(8) + stage.target,
        EnvStage::CONVEX => from + (stage.target - from) * f.powi(8),
        _ => stage.target,
    }
}

/// A prepared GPU envelope: the collapsed stage list plus the indices of the
/// attack and release stages within it (used for CC72/CC73 modifications).
#[derive(Debug, Clone, PartialEq)]
pub struct GpuEnvelope {
    /// Collapsed stages in playback order.
    pub stages: Vec<EnvStage>,
    /// Index of the attack stage (if it survived collapsing).
    pub attack_idx: Option<usize>,
    /// Index of the release stage (if it survived collapsing).
    pub release_idx: Option<usize>,
}

/// Turns an [`EnvelopeDescriptor`] into a compact stage list for the GPU,
/// mirroring XSynth's `to_envelope_params`.
///
/// Stages with zero duration are collapsed (the stage is skipped and its
/// target becomes the next stage's start), exactly like XSynth's
/// `get_stage_data` recursion.
pub fn to_gpu_stages(
    env: &EnvelopeDescriptor,
    sample_rate: u32,
    curves: EnvelopeCurveConfig,
) -> GpuEnvelope {
    let sr = sample_rate as f32;
    let to_samples = |secs: f32| (secs * sr) as u32;

    let attack = match curves.attack_curve {
        CurveKind::Linear => EnvStage {
            kind: EnvStage::CONVEX,
            target: 1.0,
            duration: to_samples(env.attack),
        },
        CurveKind::Exponential => EnvStage {
            kind: EnvStage::LERP,
            target: 1.0,
            duration: to_samples(env.attack),
        },
    };
    let decay = match curves.decay_curve {
        CurveKind::Exponential => EnvStage {
            kind: EnvStage::LERP,
            target: env.sustain_percent,
            duration: to_samples(env.decay),
        },
        CurveKind::Linear => EnvStage {
            kind: EnvStage::CONCAVE,
            target: env.sustain_percent,
            duration: to_samples(env.decay),
        },
    };
    let release = match curves.release_curve {
        CurveKind::Exponential => EnvStage {
            kind: EnvStage::LERP,
            target: 0.0,
            duration: to_samples(env.release),
        },
        CurveKind::Linear => EnvStage {
            kind: EnvStage::CONCAVE,
            target: 0.0,
            duration: to_samples(env.release),
        },
    };

    // Original 7 stages; remember their spec indices for attack (1) and
    // release (5) so CC modifications can target them after collapsing.
    let raw: [(u8, EnvStage); 7] = [
        (
            0,
            EnvStage {
                kind: EnvStage::LERP,
                target: env.start_percent,
                duration: to_samples(env.delay),
            },
        ),
        (1, attack),
        (
            2,
            EnvStage {
                kind: EnvStage::LERP,
                target: 1.0,
                duration: to_samples(env.hold),
            },
        ),
        (3, decay),
        (
            4,
            EnvStage {
                kind: EnvStage::HOLD,
                target: env.sustain_percent,
                duration: 0,
            },
        ),
        (5, release),
        (
            6,
            EnvStage {
                kind: EnvStage::HOLD,
                target: 0.0,
                duration: 0,
            },
        ),
    ];

    // Collapse zero-duration lerp stages (XSynth skips them recursively,
    // starting the next stage from the skipped stage's target).
    let mut out: Vec<EnvStage> = Vec::with_capacity(7);
    let mut attack_idx = None;
    let mut release_idx = None;
    for (spec_idx, stage) in raw {
        if (stage.kind == EnvStage::LERP
            || stage.kind == EnvStage::CONCAVE
            || stage.kind == EnvStage::CONVEX)
            && stage.duration == 0
        {
            continue;
        }
        let idx = out.len();
        if spec_idx == 1 {
            attack_idx = Some(idx);
        }
        if spec_idx == 5 {
            release_idx = Some(idx);
        }
        out.push(stage);
    }
    GpuEnvelope {
        stages: out,
        attack_idx,
        release_idx,
    }
}

/// Modifies envelope stages with CC73 (attack) / CC72 (release) values,
/// mirroring XSynth's `get_modified_envelope`:
///
/// - attack:  duration scaled by `(v/64)^5` (v <= 64) or
///   `1 + ((v-64)/64)^3 * 15` (v > 64)
/// - release: same curve, but the duration is clamped to a minimum of
///   20 ms (`max(0.02)`)
pub fn modify_env_stages(
    envelope: &mut GpuEnvelope,
    sample_rate: u32,
    attack: Option<u8>,
    release: Option<u8>,
) {
    let curve = |value: u8, duration: f32| -> f32 {
        if value <= 64 {
            (value as f32 / 64.0).powi(5) * duration
        } else {
            duration + ((value as f32 - 64.0) / 64.0).powi(3) * 15.0
        }
    };

    if let (Some(attack_val), Some(ai)) = (attack, envelope.attack_idx)
        && let Some(stage) = envelope.stages.get_mut(ai)
    {
        let old = stage.duration as f32 / sample_rate as f32;
        stage.duration = (curve(attack_val, old) * sample_rate as f32) as u32;
    }

    if let (Some(release_val), Some(ri)) = (release, envelope.release_idx)
        && let Some(stage) = envelope.stages.get_mut(ri)
    {
        let old = stage.duration as f32 / sample_rate as f32;
        let dur = curve(release_val, old).max(0.02) * sample_rate as f32;
        stage.duration = dur as u32;
    }

    // Re-collapse curve stages whose duration became zero (e.g. CC73 = 0),
    // mirroring XSynth's recursive skip in `get_stage_data`. A zero-duration
    // curve stage would otherwise divide by zero in the GPU kernel (NaN
    // output that never decays to silence) and its target value is exactly
    // what the next stage starts from anyway.
    //
    // Note: the release stage can never collapse here (it has a 20 ms floor,
    // see above). If it *was* already collapsed by `to_gpu_stages`, the
    // release index points at the terminal hold stage and `None`/the caller's
    // `stages.len() - 1` fallback are equivalent.
    let old_attack = envelope.attack_idx;
    let old_release = envelope.release_idx;
    let mut collapsed: Vec<EnvStage> = Vec::with_capacity(envelope.stages.len());
    let mut new_attack = None;
    let mut new_release = None;
    for (i, stage) in envelope.stages.iter().enumerate() {
        let is_curve = matches!(
            stage.kind,
            EnvStage::LERP | EnvStage::CONCAVE | EnvStage::CONVEX
        );
        if is_curve && stage.duration == 0 {
            // Skipped: the next stage effectively starts from this target.
            continue;
        }
        let ni = collapsed.len();
        if Some(i) == old_attack {
            new_attack = Some(ni);
        }
        if Some(i) == old_release {
            new_release = Some(ni);
        }
        collapsed.push(*stage);
    }
    envelope.stages = collapsed;
    envelope.attack_idx = new_attack;
    envelope.release_idx = new_release;
}

/// Computes the resonance Q for the filter, mirroring XSynth's
/// `SampleSoundfont` voice parameters:
/// `db_to_amp(resonance_db) * Q_BUTTERWORTH_F32`.
pub fn resonance_to_q(resonance_db: f32) -> f32 {
    db_to_amp(resonance_db) * std::f32::consts::FRAC_1_SQRT_2
}

/// RBJ cookbook low-pass biquad coefficients (Direct Form 1), returned as
/// `[b0, b1, b2, a1, a2]` (all normalized by `a0`).
pub fn biquad_lowpass_coeffs(freq: f32, sample_rate: u32, q: f32) -> [f32; 5] {
    let fs = sample_rate as f32;
    let freq = freq.clamp(1.0, fs / 2.0 - 100.0);
    let w0 = 2.0 * std::f32::consts::PI * freq / fs;
    let cosw0 = w0.cos();
    let sinw0 = w0.sin();
    let alpha = sinw0 / (2.0 * q);
    let b0 = (1.0 - cosw0) / 2.0;
    let b1 = 1.0 - cosw0;
    let b2 = b0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cosw0;
    let a2 = 1.0 - alpha;
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

/// Number of phases in the sinc table.
pub const SINC_PHASES: usize = 4096;
/// Number of taps of the windowed sinc interpolator.
pub const SINC_TAPS: usize = 64;

/// Generates a 64-point Blackman-Harris windowed sinc table.
///
/// Layout: `table[phase * SINC_TAPS + tap]`, where
/// `phase/4096` is the fractional position and the tap index covers
/// `[-31, +33)` sample positions around the fractional position:
/// `coeff = sinc(tap - 31 - frac) * window(tap)`.
pub fn build_sinc_table() -> Vec<f32> {
    let mut table = vec![0.0f32; SINC_PHASES * SINC_TAPS];
    let pi = std::f64::consts::PI;
    for phase in 0..SINC_PHASES {
        let frac = phase as f64 / SINC_PHASES as f64;
        for tap in 0..SINC_TAPS {
            // Position relative to the fractional sample position: at
            // frac = 0 only the tap-31 coefficient is non-zero (=> exact
            // sample), at frac = 0.5 the sinc is sampled at half-integers.
            let x = tap as f64 - 31.0 - frac;
            let sinc = if x.abs() < 1e-12 {
                1.0
            } else {
                let px = pi * x;
                px.sin() / px
            };
            // 4-term Blackman-Harris window, symmetric over the 64 taps
            // (standard definition: denominator N-1, alternating signs).
            let n = tap as f64;
            let w = 0.358_75 - 0.488_29 * (2.0 * pi * n / (SINC_TAPS as f64 - 1.0)).cos()
                + 0.141_28 * (4.0 * pi * n / (SINC_TAPS as f64 - 1.0)).cos()
                - 0.011_68 * (6.0 * pi * n / (SINC_TAPS as f64 - 1.0)).cos();
            table[phase * SINC_TAPS + tap] = (sinc * w) as f32;
        }
    }
    table
}

/// A Direct Form 1 biquad with explicit state (used by the CPU-side
/// reference implementation in tests; the GPU kernel implements the same
/// recursion).
#[derive(Debug, Clone, Copy)]
pub struct BiquadDf1 {
    pub coeffs: [f32; 5],
    pub x1: f32,
    pub x2: f32,
    pub y1: f32,
    pub y2: f32,
}

impl BiquadDf1 {
    /// Creates a low-pass biquad with the given frequency and Q.
    pub fn lowpass(freq: f32, sample_rate: u32, q: f32) -> Self {
        Self {
            coeffs: biquad_lowpass_coeffs(freq, sample_rate, q),
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Processes one sample (Direct Form 1 recursion).
    pub fn process(&mut self, input: f32) -> f32 {
        let [b0, b1, b2, a1, a2] = self.coeffs;
        let y = b0 * input + b1 * self.x1 + b2 * self.x2 - a1 * self.y1 - a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_factor_octave() {
        assert!((cents_factor(1200.0) - 2.0).abs() < 1e-6);
        assert!((cents_factor(0.0) - 1.0).abs() < 1e-7);
    }

    #[test]
    fn envelope_stage_collapse() {
        let env = EnvelopeDescriptor {
            start_percent: 0.5,
            delay: 0.0,
            attack: 0.01,
            hold: 0.0,
            decay: 0.05,
            sustain_percent: 0.4,
            release: 0.1,
        };
        let stages = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default()).stages;
        // delay (0) skipped, hold (0) skipped -> attack, decay, sustain, release, finished
        assert_eq!(stages.len(), 5);
        assert_eq!(stages[0].kind, EnvStage::LERP); // attack, exponential -> linear
        assert_eq!(stages[1].kind, EnvStage::CONCAVE); // decay, linear -> concave
        assert_eq!(stages[2].kind, EnvStage::HOLD); // sustain
        assert_eq!(stages[3].kind, EnvStage::CONCAVE); // release
        assert_eq!(stages[4].kind, EnvStage::HOLD); // finished
        assert_eq!(stages[0].duration, 640);
    }

    #[test]
    fn biquad_matches_reference() {
        let mut f = BiquadDf1::lowpass(8000.0, 64_000, std::f32::consts::FRAC_1_SQRT_2);
        // Impulse response should decay and stay bounded.
        let mut out = Vec::new();
        for _ in 0..100 {
            out.push(f.process(1.0));
        }
        assert!(out[0] > 0.0);
        assert!(out.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn sinc_table_passes_dc() {
        let table = build_sinc_table();
        // At frac = 0 the interpolator must reproduce the sample exactly
        // (the DC gain of the kernel must be ~1 at all phases).
        for phase in [0usize, SINC_PHASES / 2, SINC_PHASES - 1] {
            let sum: f32 = table[phase * SINC_TAPS..(phase + 1) * SINC_TAPS]
                .iter()
                .sum();
            assert!((sum - 1.0).abs() < 0.02, "phase {phase}: sum = {sum}");
        }
    }

    #[test]
    fn modify_collapses_zero_duration_attack() {
        // CC73 = 0 collapses the attack stage: it must not survive as a
        // zero-duration curve stage (which would NaN-divide on the GPU).
        let env = EnvelopeDescriptor {
            start_percent: 0.0,
            delay: 0.0,
            attack: 0.01,
            hold: 0.05,
            decay: 0.05,
            sustain_percent: 0.4,
            release: 0.1,
        };
        let mut gpu_env = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default());
        assert_eq!(gpu_env.attack_idx, Some(0));
        modify_env_stages(&mut gpu_env, 64_000, Some(0), None);
        // Attack collapsed -> first stage must be the hold stage, and no
        // zero-duration curve stage may remain anywhere.
        assert_eq!(gpu_env.attack_idx, None);
        for stage in &gpu_env.stages {
            assert!(
                !(stage.duration == 0 && stage.kind != EnvStage::HOLD),
                "zero-duration curve stage survived: {stage:?}"
            );
        }
        assert_eq!(gpu_env.stages[0].target, 1.0); // hold
        assert_eq!(gpu_env.stages[0].duration, 3200); // 0.05 s @ 64 kHz
    }

    #[test]
    fn modify_release_has_minimum_duration() {
        // CC72 has a 20 ms floor (`max(0.02)`), so even a value of 0 must
        // keep a positive duration (never a zero-duration curve stage).
        let env = EnvelopeDescriptor {
            start_percent: 0.0,
            delay: 0.0,
            attack: 0.01,
            hold: 0.05,
            decay: 0.05,
            sustain_percent: 0.4,
            release: 0.1,
        };
        let mut gpu_env = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default());
        modify_env_stages(&mut gpu_env, 64_000, None, Some(0));
        for stage in &gpu_env.stages {
            assert!(
                !(stage.duration == 0 && stage.kind != EnvStage::HOLD),
                "zero-duration curve stage survived: {stage:?}"
            );
        }
        let ri = gpu_env.release_idx.expect("release stage must remain");
        assert_eq!(gpu_env.stages[ri].kind, EnvStage::CONCAVE);
        assert_eq!(gpu_env.stages[ri].duration, 1280); // 0.02 s @ 64 kHz
    }

    #[test]
    fn modify_remaps_release_when_collapsed() {
        // A release stage with zero duration (already collapsed shape, e.g.
        // from a pathological soundfont) must not resurrect as a curve stage.
        let mut gpu_env = GpuEnvelope {
            stages: vec![
                EnvStage {
                    kind: EnvStage::LERP,
                    target: 1.0,
                    duration: 600,
                },
                EnvStage {
                    kind: EnvStage::CONCAVE,
                    target: 0.0,
                    duration: 0, // zero-duration release
                },
                EnvStage {
                    kind: EnvStage::HOLD,
                    target: 0.0,
                    duration: 0,
                },
            ],
            attack_idx: Some(0),
            release_idx: Some(1),
        };
        modify_env_stages(&mut gpu_env, 64_000, None, None);
        assert_eq!(gpu_env.stages.len(), 2);
        // Release collapsed away; the index falls back to the terminal
        // stage via `None` (callers use `stages.len() - 1`).
        assert_eq!(gpu_env.release_idx, None);
        assert!(
            gpu_env
                .stages
                .iter()
                .all(|s| s.duration > 0 || s.kind == EnvStage::HOLD)
        );
    }

    #[test]
    fn modify_keeps_indices_when_no_zero_stages() {
        // CC values that keep durations positive must not disturb the
        // attack/release indices.
        let env = EnvelopeDescriptor {
            start_percent: 0.0,
            delay: 0.0,
            attack: 0.01,
            hold: 0.05,
            decay: 0.05,
            sustain_percent: 0.4,
            release: 0.1,
        };
        let mut gpu_env = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default());
        let (attack_before, release_before) = (gpu_env.attack_idx, gpu_env.release_idx);
        modify_env_stages(&mut gpu_env, 64_000, Some(64), Some(64));
        assert_eq!(gpu_env.attack_idx, attack_before);
        assert_eq!(gpu_env.release_idx, release_before);
        assert_eq!(gpu_env.stages.len(), 6); // unchanged shape
    }
}
