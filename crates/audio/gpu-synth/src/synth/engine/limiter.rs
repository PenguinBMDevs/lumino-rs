use super::*;

/// Block limiter core, factored out of `GpuSynth::apply_limiter` so it can be
/// unit-tested without a GPU device.
///
/// Processes ONE stereo block (`out` is interleaved L,R,L,R...). `tail` is the
/// previous block's last `LOOKAHEAD` stereo frames (the delay-line head,
/// unscaled) carried across blocks; `gain` is the carried gain state. Both are
/// updated in place for the next block.
///
/// The limiter delays the signal by `LOOKAHEAD` frames and applies a lookahead
/// peak limiter so the signal never exceeds ~0.98. Crucially, the per-frame
/// gain peak-detects the EMITTED sample stream (which reaches into `tail` for a
/// spike that ended the previous block), so a brief single-sample transient is
/// attenuated at the exact output frame that outputs it - this is what stops the
/// "super-high short pop columns" that escaped the old forward-only window.
pub(crate) fn limit_block(out: &mut [f32], tail: &mut Vec<f32>, gain: &mut f32, sample_rate: f32) {
    const LOOKAHEAD: usize = 256; // frames; ~4 ms @ 64 kHz
    let n = out.len() / 2;
    if n == 0 {
        return;
    }
    let rate = sample_rate.max(1.0);

    // Pre-limiter mix (the whole block is in RAM, so the limiter can look
    // ahead). Sanitize non-finite samples first: NaN/Inf is a corrupted
    // artifact (e.g. a GPU compute blow-up at a chunk/segment boundary)
    // and must not leak a "super-high short pop" through the limiter or the
    // delay line - replace it with silence.
    let mut raw: Vec<f32> = out.to_vec();
    for s in raw.iter_mut() {
        if !s.is_finite() {
            *s = 0.0;
        }
    }

    // Peak of this block (for the fast path and the truncated tail).
    // `raw` is already sanitized above, so every sample is finite.
    let mut peak = 0.0f32;
    for &s in raw.iter() {
        let a = s.abs();
        peak = peak.max(a);
    }

    // `tail` holds the previous block's RAW input samples (the unscaled
    // delay-line head): the first LOOKAHEAD output frames are those samples
    // scaled by THIS block's gains - the gain applies at the output time,
    // exactly like the delay line itself, so the block boundary is seamless.
    if tail.len() < LOOKAHEAD * 2 {
        tail.resize(LOOKAHEAD * 2, 0.0);
    }

    let mut g = *gain;
    if peak <= 0.98 && g == 1.0 {
        // Fast path: nothing exceeds full scale AND the gain is fully
        // recovered - pure delay, no scaling.
        for i in 0..n {
            if i < LOOKAHEAD {
                out[i * 2] = tail[i * 2];
                out[i * 2 + 1] = tail[i * 2 + 1];
            } else {
                out[i * 2] = raw[(i - LOOKAHEAD) * 2];
                out[i * 2 + 1] = raw[(i - LOOKAHEAD) * 2 + 1];
            }
        }
        // The delay-line head for the next block is this block's RAW input
        // tail (unscaled): the next block scales it with ITS gain.
        tail.copy_from_slice(&raw[raw.len() - LOOKAHEAD * 2..]);
        *gain = 1.0;
        return;
    }

    let atk = 1.0 - (-1.0 / (0.0005 * rate)).exp(); // 0.5 ms attack
    let rel = 1.0 - (-1.0 / (0.080 * rate)).exp(); // 80 ms release
    let mut gains = vec![1.0f32; n];

    // Emitted-sample stream: output frame `f` emits the previous block's
    // delay-line `tail` for f < LOOKAHEAD, otherwise this block's `raw`
    // shifted by LOOKAHEAD. The gain for frame `i` MUST peak-detect THIS
    // stream around the sample it actually outputs (emit[i]) - NOT `raw`
    // alone: a spike sitting in the last LOOKAHEAD frames of the previous
    // block lives in `tail` and is emitted at a small `i` here, so the
    // window has to reach into `tail` to see it.
    let emit = |f: usize, t: &[f32], r: &[f32]| -> (f32, f32) {
        if f < LOOKAHEAD {
            (t[f * 2], t[f * 2 + 1])
        } else {
            (r[(f - LOOKAHEAD) * 2], r[(f - LOOKAHEAD) * 2 + 1])
        }
    };

    if peak > 0.98 {
        // Lookahead gain for EVERY frame i: g[i] approaches
        // 0.98 / (peak of emitted samples emit[i .. i + LOOKAHEAD]).
        //
        // The limiter delays the signal by LOOKAHEAD frames, so the sample
        // EMITTED at output frame `i` is emit[i]. Centring the window on
        // emit[i] (backward half reaches into `tail` for a previous-block
        // spike, forward half is true lookahead) guarantees a brief
        // transient is ducked BEFORE the frame that outputs it (anticipation).
        //
        // The previous window `raw[i+1 .. i+LOOKAHEAD]` looked ~2*LOOKAHEAD
        // AHEAD of the emitted sample and MISSED short spikes, so single-
        // sample "super-high pops" escaped the limiter entirely - the bug
        // behind the exported waveform's tall short pop columns. Both window
        // ends truncate at the block edge, where the block peak is the safe
        // fallback.
        for g_i in gains.iter_mut().enumerate() {
            let i = g_i.0;
            let mut l = 0.0f32;
            let start = i; // look AHEAD from the emitted sample (anticipation)
            let end = (i + LOOKAHEAD).min(n - 1);
            for f in start..=end {
                let (a, b) = emit(f, tail, &raw);
                l = l.max(a.abs()).max(b.abs());
            }
            // Forward window truncated at block end: next block unseen,
            // fall back to the block peak.
            if end < i + LOOKAHEAD {
                l = l.max(peak);
            }
            let target = if l > 0.98 { 0.98 / l } else { 1.0 };
            let k = if target < g { atk } else { rel };
            g += (target - g) * k;
            *g_i.1 = g;
        }
    } else {
        // Overloaded some time ago (g < 1) but this block is quiet:
        // the gain recovers exponentially; no lookahead scan needed.
        for g_i in gains.iter_mut() {
            g += (1.0 - g) * rel;
            *g_i = g;
        }
    }

    // Apply: output[i] = delayed raw input (previous block's tail for
    // i < LOOKAHEAD, else raw[i - LOOKAHEAD]) * gains[i], where gains[i]
    // is THIS block's gain at output time i.
    for i in 0..n {
        let (l, r) = if i < LOOKAHEAD {
            (tail[i * 2], tail[i * 2 + 1])
        } else {
            (raw[(i - LOOKAHEAD) * 2], raw[(i - LOOKAHEAD) * 2 + 1])
        };
        let gg = gains[i];
        // Final insurance for the truncated tail window: a soft knee,
        // never a hard flat-top.
        let vl = if gg == 1.0 { l } else { soft_knee(l * gg) };
        let vr = if gg == 1.0 { r } else { soft_knee(r * gg) };
        out[i * 2] = vl;
        out[i * 2 + 1] = vr;
    }
    tail.copy_from_slice(&raw[raw.len() - LOOKAHEAD * 2..]);
    *gain = g;
}

/// Slope-continuous soft ceiling for the limiter's attack window (samples
/// that still exceed 0.98 while the gain descends).
///
/// `y(0.98) = 0.98` with derivative 1.0 (matches the linear region), then
/// smoothly approaches 1.0 as `|x| -> inf`. Unlike a hard `clamp`, it never
/// produces flat-topped square-wave harmonics; unlike a per-sample soft clip
/// applied to the WHOLE signal it only engages above 0.98, so normal-range
/// audio is untouched (and reference renders stay bit-identical).
pub(crate) fn soft_knee(v: f32) -> f32 {
    let x = v.abs();
    // Below the knee the signal passes through untouched - the limiter's
    // gain scaling already did its job there.
    if x <= 0.98 {
        return v;
    }
    // t in [0, inf); tanh(0)=0 with slope 1, so the knee is slope-continuous.
    let t = (x - 0.98) / 0.02;
    let y = 0.98 + 0.02 * t.tanh();
    y.copysign(v)
}

/// Writes resampled sample bytes across the fixed-size sample chunks,
/// splitting at chunk boundaries. Returns `true` when any chunk grew
/// (bind groups must be rebuilt).
///
/// A free function so the caller can borrow `sf` (soundfont) and
/// `samples_chunks` as disjoint fields of the engine.
pub(crate) fn write_samples(
    chunks: &mut [GrowableBuffer],
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    byte_offset: u64,
    data: &[u8],
) -> Result<bool, SynthError> {
    let mut off = byte_offset;
    let mut remaining = data;
    let mut grown = false;
    while !remaining.is_empty() {
        let chunk = (off / SAMPLES_CHUNK_BYTES) as usize;
        let Some(buf) = chunks.get_mut(chunk) else {
            return Err(SynthError::Gpu(format!(
                "sample data exceeds the chunked samples buffer capacity \
                 ({} chunks of {} MiB)",
                SAMPLES_CHUNKS,
                SAMPLES_CHUNK_BYTES / (1024 * 1024)
            )));
        };
        let in_chunk = off % SAMPLES_CHUNK_BYTES;
        let take = ((SAMPLES_CHUNK_BYTES - in_chunk) as usize).min(remaining.len());
        grown |= buf.write(device, queue, in_chunk, &remaining[..take])?;
        off += take as u64;
        remaining = &remaining[take..];
    }
    Ok(grown)
}
