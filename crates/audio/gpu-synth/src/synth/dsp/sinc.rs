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
