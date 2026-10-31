use super::*;

/// REF-LGS-008（A2）：limiter 的内部处理窗口（帧）。
///
/// 增益轨迹必须只依赖**信号与绝对帧位置**，不依赖渲染块怎么切：
/// `limit_block` 会在窗口末尾 256 帧（`LOOKAHEAD`）把窗口外的未来样本回退成
/// 「整窗峰值」，如果窗口边界跟着渲染块走，块长一变增益就变（实测开限幅时
/// 512 vs 2048 的首个差异在帧 304、96% 样本不同）。
///
/// 因此 render block ≥ 512 帧时，`apply_limiter` 把它切成绝对帧对齐的 512 帧
/// 窗口逐个处理——这与 block=512 的处理序列**完全相同**，所以大块长的限幅
/// 输出与 512 档逐位一致。block < 512（实时小缓冲）时窗口 = 块长，行为与
/// 历史完全一致（不引入额外延迟）。
pub(crate) const LIMITER_WINDOW: usize = 512;

/// Splits a stereo stream into limiter windows and runs [`limit_block`] on each.
///
/// `window` is the processing granularity in frames. The caller passes
/// `min(block_frames, LIMITER_WINDOW)`, so:
/// - `block >= 512`: every block decomposes into the same absolute-aligned
///   512-frame windows regardless of the block size (A2 invariant);
/// - `block < 512`: one window per block, i.e. the historical behaviour.
///
/// Bit-exactness with `block = 512` is guaranteed by construction: the window
/// sequence, the carried delay-line `tail` and the carried `gain` are identical.
pub(crate) fn limit_stream(
    out: &mut [f32],
    tail: &mut Vec<f32>,
    gain: &mut f32,
    sample_rate: f32,
    window: usize,
) {
    let frames = out.len() / 2;
    if window == 0 || window >= frames {
        limit_block(out, tail, gain, sample_rate);
        return;
    }
    let stride = window * 2;
    for chunk in out.chunks_mut(stride) {
        limit_block(chunk, tail, gain, sample_rate);
    }
}

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
    let tail_bytes = LOOKAHEAD * 2;
    if tail.len() < tail_bytes {
        tail.resize(tail_bytes, 0.0);
    }

    // DEBT-04 #121：通用延迟线推进——下一块的 tail = (旧 tail ++ 本块 raw) 的最后
    // LOOKAHEAD 帧。长块（n >= LOOKAHEAD）时与旧实现 `raw.last(512)` 完全等价；
    // 短块（n < LOOKAHEAD，UI 允许 64/128 帧 buffer）时正确保留"尚未播出的
    // 上一段尾巴"。旧实现在 n < 256 时对 `raw.len() - 512` 负索引 panic。
    let mut next_tail: Vec<f32> = Vec::with_capacity(tail.len() + raw.len());
    next_tail.extend_from_slice(tail);
    next_tail.extend_from_slice(&raw);
    let next_tail = next_tail.split_off(next_tail.len() - tail_bytes);

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
        // The delay-line head for the next block is the generalized
        // `(old_tail ++ raw).last(LOOKAHEAD)` (unscaled): the next block
        // scales it with ITS gain.
        tail.copy_from_slice(&next_tail);
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
    tail.copy_from_slice(&next_tail);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic stereo test signal with quiet and loud stretches, so the
    /// limiter actually attacks and releases inside the stream.
    fn signal(frames: usize) -> Vec<f32> {
        let mut state = 0x1234_5678u32;
        let mut out = Vec::with_capacity(frames * 2);
        for f in 0..frames {
            // Loud burst in the middle, quiet elsewhere: exercises clamp-free
            // unity regions, attack and release.
            let amp = if (1000..1400).contains(&f) { 3.5 } else { 0.3 };
            for _ in 0..2 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = ((state >> 8) as f32 / 8_388_608.0) - 1.0;
                out.push(noise * amp);
            }
        }
        out
    }

    fn run_per_window(input: &[f32], window: usize) -> (Vec<f32>, Vec<f32>, f32) {
        let mut out = input.to_vec();
        let mut tail = Vec::new();
        let mut gain = 1.0f32;
        for chunk in out.chunks_mut(window * 2) {
            limit_block(chunk, &mut tail, &mut gain, 48_000.0);
        }
        (out, tail, gain)
    }

    /// A2 核心不变量：把整段一次性喂给 `limit_stream` 必须与逐窗调用
    /// `limit_block` **逐位**相同（增益/延迟线都一致）。
    #[test]
    fn limit_stream_equals_per_window_calls() {
        let input = signal(2048);
        let (want, want_tail, want_gain) = run_per_window(&input, LIMITER_WINDOW);

        let mut got = input.clone();
        let mut tail = Vec::new();
        let mut gain = 1.0f32;
        limit_stream(&mut got, &mut tail, &mut gain, 48_000.0, LIMITER_WINDOW);

        assert_eq!(
            got.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            want.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            "windowed stream must be bit-identical to per-window blocks"
        );
        assert_eq!(gain.to_bits(), want_gain.to_bits());
        assert_eq!(
            tail.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            want_tail.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
    }

    /// 外层怎么切块不影响结果：一段 2048 帧，一次处理 vs 两次 1024 帧处理。
    #[test]
    fn limit_stream_ignores_outer_block_split() {
        let input = signal(2048);

        let mut one = input.clone();
        let (mut tail_a, mut gain_a) = (Vec::new(), 1.0f32);
        limit_stream(&mut one, &mut tail_a, &mut gain_a, 48_000.0, LIMITER_WINDOW);

        let mut split = input.clone();
        let (mut tail_b, mut gain_b) = (Vec::new(), 1.0f32);
        for chunk in split.chunks_mut(1024 * 2) {
            limit_stream(chunk, &mut tail_b, &mut gain_b, 48_000.0, LIMITER_WINDOW);
        }

        assert_eq!(
            one.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            split.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        assert_eq!(gain_a.to_bits(), gain_b.to_bits());
    }

    /// block = 512 时窗口等于块长，行为必须与改动前（单次 `limit_block`）
    /// 完全一致——这是唯一权威基线。
    #[test]
    fn block_512_is_unchanged() {
        let input = signal(512);
        let mut a = input.clone();
        let (mut tail_a, mut gain_a) = (Vec::new(), 1.0f32);
        limit_block(&mut a, &mut tail_a, &mut gain_a, 48_000.0);

        let mut b = input.clone();
        let (mut tail_b, mut gain_b) = (Vec::new(), 1.0f32);
        limit_stream(&mut b, &mut tail_b, &mut gain_b, 48_000.0, 512);

        assert_eq!(
            a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            b.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        assert_eq!(gain_a.to_bits(), gain_b.to_bits());
    }

    /// 小块（窗口 ≥ 块长）退化为单次 `limit_block`：实时路径零行为变化。
    #[test]
    fn small_block_falls_back_to_single_call() {
        for frames in [16usize, 64, 128, 256] {
            let input = signal(frames);
            let mut a = input.clone();
            let (mut tail_a, mut gain_a) = (Vec::new(), 1.0f32);
            limit_block(&mut a, &mut tail_a, &mut gain_a, 48_000.0);

            let mut b = input.clone();
            let (mut tail_b, mut gain_b) = (Vec::new(), 1.0f32);
            limit_stream(&mut b, &mut tail_b, &mut gain_b, 48_000.0, frames);

            assert_eq!(
                a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                b.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                "frames={frames} must keep the historical single-call path"
            );
            assert_eq!(gain_a.to_bits(), gain_b.to_bits());
        }
    }
}
