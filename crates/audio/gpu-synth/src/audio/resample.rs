//! Sample-rate conversion for realtime playback.
//!
//! The engine renders at its configured sample rate (e.g. 64 kHz); output
//! devices usually run at a different rate (e.g. 48 kHz). The converter is
//! a windowed-sinc (Blackman-Harris) polyphase FIR with a cutoff at 0.9x
//! the output Nyquist, so downsampling is anti-aliased: without a low-pass,
//! content between the output Nyquist and the input Nyquist (24-32 kHz at
//! 64k -> 48k) folds back into the audible band. On dense high-frequency
//! content (synthetic test soundfonts, black-MIDI) that alias is audible as
//! crackle - one of the "crackle with healthy render time" sources.
//!
//! The FIR is causal over a one-block window: each `process` call outputs
//! the audio of the PREVIOUS input block (the current block supplies the
//! "future" taps), so the filter is never truncated at block boundaries.
//! The one-block delay (~32 ms at 2048 frames) is inaudible behind the
//! playback queue's cushion. The cost is ~32 MACs per output sample per
//! channel (negligible: ~0.05 ms per 2048-frame block).

/// Number of FIR taps. 32 taps with the Blackman-Harris window gives ~90 dB
/// stopband attenuation at a fraction over 2x the transition width.
const TAPS: usize = 32;
/// Number of phase steps per input sample for the polyphase lookup.
const PHASES: usize = 256;
/// Cutoff relative to the input rate (fraction of the output Nyquist).
const CUTOFF_FRAC: f64 = 0.9;
/// FIR taps that reach into the "future" (half the window).
const HALF: i64 = (TAPS / 2) as i64;

/// Windowed-sinc polyphase resampler (anti-aliased).
pub(crate) struct SincResampler {
    ratio: f64,
    channels: usize,
    /// Polyphase coefficient table: `[phase * TAPS + tap]`.
    table: Vec<f32>,
    /// The previous input block, whose audio the next `process` call emits
    /// (with the current block providing the future taps).
    pending: Vec<f32>,
    /// The last `HALF` frames before `pending` (per channel), so the FIR
    /// taps before the block are satisfied across three-block boundaries.
    history: Vec<f32>,
}

impl SincResampler {
    pub(crate) fn new(from: u32, to: u32, channels: usize) -> Self {
        let ratio = if from == 0 {
            1.0
        } else {
            to as f64 / from as f64
        };
        // The FIR cutoff sits at 0.9x the smaller Nyquist (the anti-alias
        // point when downsampling; the imaging suppressor when upsampling).
        let fc = CUTOFF_FRAC * 0.5 * from.min(to).max(1) as f64 / from.max(1) as f64;
        let table = build_table(fc);
        Self {
            ratio,
            channels,
            table,
            pending: Vec::new(),
            history: vec![0.0; channels * HALF as usize],
        }
    }

    /// Resamples one interleaved block; output length = round(input * ratio).
    ///
    /// Emits the PREVIOUS call's input (one-block latency), so the FIR
    /// always has the future taps it needs.
    pub(crate) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if (self.ratio - 1.0).abs() < 1e-9 {
            return input.to_vec();
        }
        let chs = self.channels;
        let pending = std::mem::take(&mut self.pending);
        if pending.is_empty() {
            // First call: buffer the input, emit silence (the playback
            // pre-fill already covers this with silent blocks).
            self.pending = input.to_vec();
            return vec![0.0f32; ((input.len() / chs) as f64 * self.ratio) as usize * chs];
        }

        let n_in = pending.len() / chs;
        let n_out = ((n_in as f64) * self.ratio) as usize;
        let mut out = vec![0.0f32; n_out * chs];
        let future = HALF as usize;
        let hist = HALF as usize;

        for (o, chunk) in out.chunks_exact_mut(chs).enumerate() {
            // Output sample o sits at input position o * (from/to) = o/ratio
            // (e.g. 1.3333 input samples per output sample at 64k -> 48k).
            // The +0.5 offset aligns the FIR center (tap 15.5 + frac) with
            // the exact interpolation position.
            let p = 0.5 + (o as f64) / self.ratio;
            let i0 = p.floor() as i64;
            let frac = p - i0 as f64;
            let phase = ((frac * PHASES as f64) as usize).min(PHASES - 1);
            let tap_base = i0 - HALF;
            let tbl = &self.table[phase * TAPS..phase * TAPS + TAPS];
            for (c, dst) in chunk.iter_mut().enumerate() {
                let mut acc = 0.0f32;
                for (k, &h) in tbl.iter().enumerate() {
                    let idx = tap_base + k as i64;
                    let v = if idx < 0 {
                        // The taps before the block come from `history`
                        // (the previous-pending tail).
                        let hi = (idx + HALF) as usize;
                        if hi < hist {
                            self.history[hi * chs + c]
                        } else {
                            0.0
                        }
                    } else if (idx as usize) < n_in {
                        pending[idx as usize * chs + c]
                    } else if (idx as usize) - n_in < future {
                        // The next block supplies the future taps.
                        let j = (idx as usize) - n_in;
                        input.get(j * chs + c).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    acc += h * v;
                }
                *dst = acc;
            }
        }

        // The tail of `pending` becomes the history for the next call.
        for (i, dst) in self.history.iter_mut().enumerate() {
            let src = (n_in - hist + i / chs).max(0) as usize;
            *dst = pending[src * chs + i % chs];
        }
        self.pending = input.to_vec();
        out
    }
}

/// Builds the Blackman-Harris windowed-sinc polyphase table.
///
/// Each phase's coefficients are normalized to sum to 1.0, so the DC gain
/// is exactly unity (window truncation otherwise leaves ~0.6% gain error,
/// audible as a ~0.05 dB level tilt on everything).
fn build_table(fc: f64) -> Vec<f32> {
    let n_1 = (TAPS as f64) - 1.0;
    let center = n_1 / 2.0; // 15.5: the FIR center (sinc AND window align here)
    let mut table: Vec<f64> = Vec::with_capacity(PHASES * TAPS);
    for ph in 0..PHASES {
        let mut sum = 0.0f64;
        for k in 0..TAPS {
            let t = (k as f64) - center - (ph as f64) / PHASES as f64;
            // Normalized sinc with cutoff fc (cycles per input sample).
            let x = 2.0 * std::f64::consts::PI * fc * t;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { x.sin() / x };
            // 4-term Blackman-Harris window over n = 0..1, centered on the
            // FIR center (t = 0), so the coefficients are symmetric.
            let n = (t + center) / n_1;
            let w = 0.35875 - 0.48829 * (2.0 * std::f64::consts::PI * n).cos()
                + 0.14128 * (4.0 * std::f64::consts::PI * n).cos()
                - 0.01168 * (6.0 * std::f64::consts::PI * n).cos();
            table.push(sinc * w * 2.0 * fc);
            sum += sinc * w * 2.0 * fc;
        }
        for h in table[ph * TAPS..ph * TAPS + TAPS].iter_mut() {
            *h /= sum;
        }
    }
    // Convert to f32 for the shader-side lookup.
    table.into_iter().map(|v| v as f32).collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_identity_rate() {
        let mut r = SincResampler::new(48_000, 48_000, 2);
        let input = vec![1.0f32, -1.0, 0.5, -0.5];
        let out = r.process(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn resampler_output_length_tracks_ratio() {
        let mut r = SincResampler::new(48_000, 44_100, 2);
        let n = 10_000;
        let mut input = Vec::with_capacity(n * 2);
        for i in 0..n {
            input.push((i as f32 * 0.01).sin());
            input.push((i as f32 * 0.013).cos());
        }
        let _ = r.process(&input); // warm-up (emits silence)
        let out = r.process(&input);
        let expected = (n as f64 * (44_100.0 / 48_000.0)) as usize;
        assert_eq!(out.len(), expected * 2);
    }

    #[test]
    fn resampler_preserves_low_frequencies() {
        // A 1 kHz sine must survive 64k -> 48k with negligible error.
        let mut r = SincResampler::new(64_000, 48_000, 1);
        let n = 64_000;
        let input: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 64_000.0).sin())
            .collect();
        let _ = r.process(&input); // warm-up (emits silence)
        let out = r.process(&input);
        // Compare against a directly-sampled 1 kHz sine at 48k.
        let mut err = 0.0f64;
        for (i, &s) in out.iter().enumerate() {
            let refv = (i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 48_000.0).sin();
            err += (s as f64 - refv as f64).powi(2);
        }
        let rms = (err / out.len() as f64).sqrt();
        assert!(rms < 0.002, "1 kHz preservation rms={rms}");
    }

    #[test]
    fn resampler_continuous_across_blocks() {
        // Two consecutive blocks must not jump at the seam: feed a linear
        // ramp in blocks and check the interpolated values stay continuous.
        // The first call emits silence (one-block latency), so we need one
        // extra warm-up call at the start.
        let mut r = SincResampler::new(64_000, 48_000, 1);
        let warm = vec![0.0f32; 512]; // warm-up: emits silence, buffers it
        let _ = r.process(&warm);
        let mut prev_end = 0.0f32;
        for block in 0..4 {
            let start = block * 512;
            let input: Vec<f32> = (0..512).map(|i| (start + i) as f32 * 0.001).collect();
            let out = r.process(&input);
            let first = out.first().copied().unwrap_or(0.0);
            println!(
                "iter {block}: len={} first={first:.5} last={:.5} prev_end={prev_end:.5}",
                out.len(),
                out.last().copied().unwrap_or(0.0)
            );
            assert!(
                (first - prev_end).abs() < 0.01,
                "seam jump {first} vs {prev_end}"
            );
            prev_end = *out.last().unwrap_or(&0.0);
        }
    }

    #[test]
    fn resampler_attenuates_above_output_nyquist() {
        // A 28 kHz tone (below the 64k Nyquist) must be strongly attenuated
        // by 64k -> 48k resampling: without an anti-alias filter it would
        // fold back to 20 kHz. The FIR cutoff is 0.9x24k = 21.6 kHz, so a
        // 28 kHz tone should come out ~60+ dB down.
        let mut r = SincResampler::new(64_000, 48_000, 1);
        let n = 64_000;
        let input: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 28_000.0 / 64_000.0).sin())
            .collect();
        let _ = r.process(&input); // warm-up
        let out = r.process(&input);
        // RMS of the output: the 28 kHz tone must be gone (would be 0.707
        // without filtering; the alias to 20 kHz would give ~0.7 too).
        let rms =
            (out.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / out.len() as f64).sqrt();
        assert!(
            rms < 0.01,
            "28 kHz leaked through 64k->48k resampler: rms={rms} (anti-alias broken)"
        );
    }

    #[test]
    fn fir_table_is_normalized_symmetric() {
        // Phase 0 (integer positions) must be a symmetric low-pass: mirror
        // taps equal and the sum = 1.0 (DC gain unity). The FIR center is
        // at tap 15.5, so no single tap equals 2*fc.
        let table = build_table(0.3375);
        let sum: f32 = table[0..TAPS].iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "DC gain {sum} != 1.0");
        for k in 0..16 {
            let err = (table[15 - k] - table[16 + k]).abs();
            assert!(err < 1e-5, "asymmetry at tap {k}: {err}");
        }
    }
}
