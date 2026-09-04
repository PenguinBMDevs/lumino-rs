// lumino-gpu-synth render kernel (pass 1).
//
// One invocation renders one voice for the whole block (BLOCK frames),
// producing `BLOCK * 2` interleaved stereo samples into `voice_out`.
//
// The voice state (playback position, envelope stage, filter state) is
// read from_val `states` at the start of the block and written back at the end,
// so voices can span arbitrary many blocks.
//
// Signal chain (mirrors XSynth's stereo voice):
//   sample(L/R) * amp(velocity volume) * pan_gain(L/R) * envelope
//   -> per-channel biquad low-pass (if enabled)
// The channel volume/expression/pan controllers are applied in the mix pass.

struct VoiceParams {
    is_active: u32,          // 0 = slot unused
    sample_offset: u32,   // offset of the sample data inside `samples`
    sample_offset_r: u32,  // offset of the right-channel sample data
    sample_len: u32,      // length of the (first channel) sample data
    offset: u32,          // playback start offset (converted domain)
    sample_end: u32,      // voice ends when time >= sample_end (conv(sample_end) - conv(offset))
    loop_mode: u32,       // 0 = no loop, 1 = continuous, 2 = sustain
    loop_start: u32,      // data-relative loop start (converted domain)
    loop_end: u32,        // data-relative loop end (converted domain)
    speed: f32,           // samples advanced per output frame
    amp: f32,             // static amplitude (volume * velocity curve)
    pan_l: f32,           // left gain from_val the zone pan
    pan_r: f32,           // right gain from_val the zone pan
    filter_on: u32,
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    env_base: u32,        // index of this voice's first stage in `env_stages`
    env_count: u32,       // number of stages
    release_idx: u32,     // stage index (relative to env_base) to jump to on release
    finished_idx: u32,    // index of the terminal stage
    release_at: u32,      // absolute global frame at which release starts (0xFFFFFFFF = none)
    base_frame: u32,      // absolute global frame of this block's first sample
    interp: u32,          // 0 = linear, 1 = 64-point sinc
    channels: u32,        // 1 = mono, 2 = stereo pair
    start_at: u32,        // absolute global frame at which the voice starts (gated before)
    channel: u32,         // MIDI channel (0-15), used by the mix pass
}

struct VoiceState {
    int_time: u32,        // integer part of playback position (f64-equivalent)
    frac: f32,            // fractional part of playback position
    env_stage: u32,       // current stage index (relative to env_base)
    env_t: u32,           // samples elapsed in the current stage
    env_from: f32,        // value at the start of the current stage
    lx1: f32, lx2: f32, ly1: f32, ly2: f32,  // biquad state (left channel)
    rx1: f32, rx2: f32, ry1: f32, ry2: f32,  // biquad state (right channel)
    last_loop_pos: u32,   // loop position at release (loop sustain mode)
    is_released: u32,     // sampler-side release flag
    ended: u32,           // 1 when the voice has finished (sample end or env done)
}

struct EnvStageGpu {
    kind: u32,            // 0 lerp, 1 concave, 2 convex, 3 hold
    target_val: f32,
    duration: u32,
}

@group(0) @binding(0) var<storage, read> params: array<VoiceParams>;
// Sample data lives in fixed-size chunks so that no single storage binding
// exceeds the device's per-binding size limit. CHUNK_F32 must match
// SAMPLES_CHUNK_BYTES / 4 in gpu.rs (1 GiB of f32).
const CHUNK_F32: u32 = 268435456u;
@group(0) @binding(1) var<storage, read> samples0: array<f32>;
@group(0) @binding(2) var<storage, read> samples1: array<f32>;
@group(0) @binding(3) var<storage, read> samples2: array<f32>;
@group(0) @binding(4) var<storage, read> samples3: array<f32>;
@group(0) @binding(5) var<storage, read> sinc_table: array<f32>;
@group(0) @binding(6) var<storage, read> env_stages: array<EnvStageGpu>;
@group(0) @binding(7) var<storage, read_write> states: array<VoiceState>;
@group(0) @binding(8) var<storage, read_write> voice_out: array<f32>;

const VOICES_PER_GROUP: u32 = 128u;
const SINC_PHASES: u32 = 4096u;
const SINC_TAPS: u32 = 64u;
const BLOCK: u32 = 512u;

// ---------- helpers ----------

fn env_eval(kind: u32, from_val: f32, target_val: f32, f_in: f32) -> f32 {
    // Clamp the stage progress to [0, 1]. A stale `env_t` (larger than the
    // stage duration, e.g. after a long-gap resume) would otherwise feed
    // prog > 1 into the curve formulas: CONCAVE's ((1-prog)^2)^4 explodes
    // to hundreds of millions and the voice outputs a single-sample pop
    // (measured: 9.2e8 in the mix at high polyphony - the "crackle at
    // 800-1000 voices" report).
    let f = clamp(f_in, 0.0, 1.0);
    if kind == 0u {
        return from_val + (target_val - from_val) * f;
    }
    if kind == 1u {
        let m = (1.0 - f) * (1.0 - f);
        let m2 = m * m;
        let mult = m2 * m2;
        return (from_val - target_val) * mult + target_val;
    }
    if kind == 2u {
        let m = f * f;
        let m2 = m * m;
        let mult = m2 * m2;
        return from_val + (target_val - from_val) * mult;
    }
    return target_val;
}

// Reads one f32 from the chunked samples buffer; out-of-bounds reads
// yield 0.0. CHUNK_F32 = 1<<28, so shift/mask is cheaper than div/mod
// (flame graph: raw_sample is hottest GPU function at 25k voices).
fn raw_sample(offset: u32, idx: u32) -> f32 {
    let i = offset + idx;
    let chunk = i >> 28u;
    let off = i & 268435455u;
    if (chunk == 0u) {
        if (off < arrayLength(&samples0)) { return samples0[off]; }
        return 0.0;
    }
    if (chunk == 1u) {
        if (off < arrayLength(&samples1)) { return samples1[off]; }
        return 0.0;
    }
    if (chunk == 2u) {
        if (off < arrayLength(&samples2)) { return samples2[off]; }
        return 0.0;
    }
    if (chunk == 3u) {
        if (off < arrayLength(&samples3)) { return samples3[off]; }
    }
    return 0.0;
}

// Computes the looped position for a data-relative absolute index.
fn loop_pos(p: VoiceParams, pos_abs: u32, released: u32, last_loop: u32) -> u32 {
    if p.loop_mode == 1u {
        var pos = pos_abs;
        if pos > p.loop_end {
            pos = (pos - p.loop_end - 1u) % (p.loop_end - p.loop_start) + p.loop_start;
        }
        return pos;
    }
    if p.loop_mode == 2u {
        if released == 0u {
            var pos = pos_abs;
            if pos > p.loop_end {
                pos = (pos - p.loop_end - 1u) % (p.loop_end - p.loop_start) + p.loop_start;
            }
            return pos;
        }
        // Released: continue from_val loop_end with the same elapsed time.
        let elapsed = pos_abs - last_loop;
        return p.loop_end + elapsed;
    }
    return pos_abs;
}

fn linear_interp(p: VoiceParams, pos_abs: u32, frac: f32, released: u32, last_loop: u32) -> f32 {
    let a = loop_pos(p, pos_abs, released, last_loop);
    let v0 = raw_sample(p.sample_offset, a);
    let v1 = raw_sample(p.sample_offset, a + 1u);
    return v0 * (1.0 - frac) + v1 * frac;
}

fn sinc_interp(p: VoiceParams, pos_abs: u32, frac: f32, released: u32, last_loop: u32) -> f32 {
    let phase = u32(frac * f32(SINC_PHASES)) & (SINC_PHASES - 1u);
    var acc = 0.0;
    for (var k = 0u; k < SINC_TAPS; k = k + 1u) {
        let c = sinc_table[phase * SINC_TAPS + k];
        let base = i32(pos_abs) + i32(k) - 31i;
        if base >= 0 {
            let a = loop_pos(p, u32(base), released, last_loop);
            acc += c * raw_sample(p.sample_offset, a);
        }
    }
    return acc;
}

// ---------- state advance ----------

// The result of advancing a voice by one frame.
struct AdvanceResult {
    st: VoiceState,
    env_value: f32,
}

// Advances the voice state by exactly one output frame at absolute `frame`.
// Mirrors the per-frame logic of the old monolithic loop: release
// scheduling, envelope progression, sample-end check and position advance.
// The filter is disabled in this configuration (`use_effects = false`), so
// the state advance has no signal dependency and can be replayed by every
// segment thread to fast-forward to its start.
fn advance_frame(p: VoiceParams, st: VoiceState, env_value: f32, frame: u32) -> AdvanceResult {
    var s = st;
    var ev = env_value;

    // --- release scheduling (sample-accurate) ---
    if (p.release_at != 0xFFFFFFFFu && frame >= p.release_at && s.is_released == 0u) {
        s.is_released = 1u;
        s.env_stage = p.release_idx;
        s.env_t = 0u;
        s.env_from = ev;
        // Capture the loop position at the instant of release so a loop-sustain
        // (mode 2) voice continues from `loop_end` for the correct number of
        // samples after release; otherwise the release tail is computed from a
        // stale/block-start position and jumps.
        s.last_loop_pos = s.int_time;
    }

    // --- envelope ---
    let stage_idx = s.env_stage;
    if (stage_idx >= p.finished_idx) {
        // Terminal stage reached (or passed): the envelope is done and
        // the voice ends, mirroring XSynth's `envelope.ended()`.
        s.ended = 1u;
    } else if (stage_idx < p.env_count) {
        let es = env_stages[p.env_base + stage_idx];
        if (es.kind == 3u) {
            ev = es.target_val; // hold
        } else {
            // Defensive: a zero-duration stage must never divide by zero
            // (the CPU collapses those; this is a last-resort guard so a
            // pathological envelope cannot poison the output with NaN).
            let denom = max(es.duration, 1u);
            let prog = f32(s.env_t) / f32(denom);
            ev = env_eval(es.kind, s.env_from, es.target_val, prog);
            s.env_t = s.env_t + 1u;
            if (s.env_t >= es.duration) {
                s.env_from = ev;
                s.env_stage = stage_idx + 1u;
                s.env_t = 0u;
            }
        }
    }

    // Sample ended check (no-loop only): time >= sample_end (already
    // reduced by offset on the CPU side).
    if (p.loop_mode == 0u && s.int_time >= p.sample_end) {
        s.ended = 1u;
    }

    // Release-tail silence cut: once a released voice's envelope falls
    // below the silence threshold, end it. Mirrors the offline renderer's
    // "render until silent" semantics and keeps long-release soundfonts
    // from accumulating thousands of inaudible tail voices. The output
    // below the threshold is indistinguishable from the reference.
    if (s.is_released != 0u && ev < 0.0002) {
        s.ended = 1u;
    }

    // --- advance position ---
    var carry: u32 = 0u;
    var frac = s.frac + p.speed;
    if (frac >= 1.0) {
        let n = u32(frac);
        frac = frac - f32(n);
        carry = n;
    }
    s.int_time = s.int_time + carry;
    s.frac = frac;

    return AdvanceResult(s, ev);
}

// ---------- main ----------

// Each workgroup (y = SEGS segments) renders one voice; the shader
// fast-forwards the state to its segment start, so the GPU parallelism is
// voices x segments. The default below is replaced at pipeline creation
// with the engine's `RENDER_SEGMENTS` (see `create_render_pipeline`).
const SEGS: u32 = 16u;
const SEG_LEN: u32 = BLOCK / SEGS;

@compute
@workgroup_size(VOICES_PER_GROUP)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let voice = gid.x;
    if (voice >= arrayLength(&params)) {
        return;
    }
    let p = params[voice];
    if (p.is_active == 0u) {
        return;
    }

    var st = states[voice];
    let out_base = voice * BLOCK * 2u;
    let seg = gid.y;
    // Filtered voices (use_effects = true) have signal-dependent biquad
    // state that cannot be fast-forwarded, so they run single-segment:
    // segment 0 renders the whole block, the other segments just zero their
    // output range.
    let is_filtered = p.filter_on != 0u;
    let seg_start = select(seg * SEG_LEN, 0u, is_filtered);
    let seg_end = select(min(seg_start + SEG_LEN, BLOCK), BLOCK, is_filtered);
    if (is_filtered && seg > 0u) {
        // Filtered (biquad) voices are rendered entirely by segment 0: the
        // filter state is signal-dependent and cannot be fast-forwarded, so the
        // other segments must leave this voice's output untouched. The old code
        // zeroed the WHOLE block here, racing with segment 0's real writes and
        // corrupting the signal at the segment boundary (a "chunk" boundary in
        // the voice block) - a source of crackle/pops for filtered voices.
        return;
    }

    // NOTE: the loop-sustain release position is captured at the moment of
    // release inside `advance_frame` (so it tracks the true loop position),
    // NOT here - capturing it at the block start (as a previous version did)
    // froze `last_loop_pos` at the block-start time for the whole block and,
    // worse, overwrote the value persisted from the block where the voice was
    // actually released, producing wrong release tails / discontinuities.

    var env_value = st.env_from;

    // Fast-forward the state to this segment's start. No sample reads, only
    // release/envelope/position logic; frames before `start_at` are gated
    // (no state advance, like the original loop's `continue`).
    for (var k = 0u; k < seg_start; k = k + 1u) {
        let frame = p.base_frame + k;
        if (frame < p.start_at) {
            continue;
        }
        let adv = advance_frame(p, st, env_value, frame);
        st = adv.st;
        env_value = adv.env_value;
        if (st.ended == 1u) {
            break; // finished voices stay silent; no more state to advance
        }
    }

    var f = seg_start;
    while (f < seg_end) {
        let frame = p.base_frame + f;

        // --- start gating: the voice is silent before its note-on frame ---
        if (frame < p.start_at) {
            let idx = out_base + f * 2u;
            voice_out[idx] = 0.0;
            voice_out[idx + 1u] = 0.0;
            f = f + 1u;
            continue;
        }

        var sample_l = 0.0;
        var sample_r = 0.0;

        if (st.ended == 0u) {
            // --- sample position & interpolation ---
            let pos_abs = st.int_time + p.offset;
            if (p.interp == 1u) {
                sample_l = sinc_interp(p, pos_abs, st.frac, st.is_released, st.last_loop_pos);
            } else {
                sample_l = linear_interp(p, pos_abs, st.frac, st.is_released, st.last_loop_pos);
            }
            if (p.channels == 2u) {
                var p_r = p;
                p_r.sample_offset = p.sample_offset_r;
                if (p.interp == 1u) {
                    sample_r = sinc_interp(p_r, pos_abs, st.frac, st.is_released, st.last_loop_pos);
                } else {
                    sample_r = linear_interp(p_r, pos_abs, st.frac, st.is_released, st.last_loop_pos);
                }
            } else {
                // Mono sample duplicated to both channels (XSynth behaviour).
                sample_r = sample_l;
            }
        }

        // --- gain chain: sample * amp * pan * env (mirrors XSynth) ---
        var value_l = sample_l * p.amp * p.pan_l * env_value;
        var value_r = sample_r * p.amp * p.pan_r * env_value;

        // --- per-channel biquad low-pass (XSynth stereo voice structure;
        // only active for filtered voices, which run single-segment) ---
        if (p.filter_on != 0u) {
            let yl = p.b0 * value_l + p.b1 * st.lx1 + p.b2 * st.lx2
                   - p.a1 * st.ly1 - p.a2 * st.ly2;
            st.lx2 = st.lx1; st.lx1 = value_l;
            st.ly2 = st.ly1; st.ly1 = yl;
            value_l = yl;

            let yr = p.b0 * value_r + p.b1 * st.rx1 + p.b2 * st.rx2
                   - p.a1 * st.ry1 - p.a2 * st.ry2;
            st.rx2 = st.rx1; st.rx1 = value_r;
            st.ry2 = st.ry1; st.ry1 = yr;
            value_r = yr;
        }

        let out_idx = out_base + f * 2u;
        voice_out[out_idx] = value_l;
        voice_out[out_idx + 1u] = value_r;

        // --- advance to the next frame ---
        let adv = advance_frame(p, st, env_value, frame);
        st = adv.st;
        env_value = adv.env_value;

        f = f + 1u;
    }

    // Persist the state from the last segment only (all segments compute
    // the same value; writing once avoids needless storage traffic).
    // Filtered voices are single-segment and persist from segment 0.
    if (seg == SEGS - 1u || (is_filtered && seg == 0u)) {
        states[voice] = st;
    }
}










