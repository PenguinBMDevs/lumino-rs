use super::db_to_amp;

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
