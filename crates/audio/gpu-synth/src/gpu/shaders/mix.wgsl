// lumino-gpu-synth mix kernel (pass 2a: per-channel gather).
//
// REF-LGS-001: the old kernel was one thread per output frame looping over
// ALL voices -> block 512 gave only 4 workgroups (128 threads each) and the
// pass cost ~11.5 ms per block on the 5080 (measured: skipping the pass
// drops `collect` from ~12.8 ms to ~1.2 ms).
//
// This version keeps the EXACT floating-point accumulation order while
// raising parallelism: the host builds stable per-(channel, released) voice
// buckets each block (voice order preserved), and one thread handles one
// (frame, channel) pair. Each thread sums only its channel's voices in the
// original v-ascending order, applies the same controller lerp/amp/pan
// formula, and writes a per-channel partial. A tiny reduce pass (pass 2b)
// adds the channels in ascending order - the same order the old kernel used
// for `out_l/out_r` - so the output stays bit-identical.

struct MixParams {
    voice_count: u32,
    block_size: u32,
    channel_count: u32,
    event_count: u32,
    lerp_len: f32, // sample_rate * 0.01 (10 ms)
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    starts: array<MixStart, 256>,
}

struct MixStart {
    vol: f32,
    vol_step: f32,
    vol_end: f32,
    expr: f32,
    expr_step: f32,
    expr_end: f32,
    pan: f32,
    pan_step: f32,
    pan_end: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

struct MixEvent {
    frame: u32,
    channel: u32,
    cc: u32,
    value: f32,
}

// Fixed prefix of the bucket buffer: 2 groups x (256 channels + 1) offsets.
// Indices start after this prefix (see `MIX_BUCKET_OFFSETS` on the host).
const BUCKET_OFFSETS: u32 = 514u;

@group(0) @binding(0) var<storage, read> voice_out: array<f32>;
@group(0) @binding(2) var<storage, read> mix_buckets: array<u32>;
@group(0) @binding(3) var<storage, read> mix_events: array<MixEvent>;
@group(0) @binding(4) var<uniform> mix_params: MixParams;
@group(0) @binding(5) var<storage, read_write> mix_partial: array<f32>;

// Advances a ValueLerp by `n` frames and clamps to its target.
fn lerp_advance(current: f32, step: f32, end: f32, n: f32) -> f32 {
    if (end > current) {
        return min(current + step * n, end);
    }
    if (end < current) {
        return max(current + step * n, end);
    }
    return current;
}

// NOTE: the mix output is the raw voice sum - possibly far past full scale
// (hundreds/thousands of voices accumulate with no headroom management).
// f32 holds it losslessly. Anti-crackle headroom is applied on the CPU side
// (engine.rs `apply_limiter`): a block-level peak limiter that scales the
// whole block, preserving the waveform exactly.

@compute
@workgroup_size(128)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.x;
    let ch = gid.y;
    let channels = mix_params.channel_count;
    if (f >= mix_params.block_size || ch >= channels) {
        return;
    }

    // Stable per-(channel, released) buckets: group 0 = sustained voices,
    // group 1 = released voices, each in the original ascending voice order.
    let stride = channels + 1u;
    let s0 = mix_buckets[ch];
    let s1 = mix_buckets[ch + 1u];
    let r0 = mix_buckets[stride + ch];
    let r1 = mix_buckets[stride + ch + 1u];

    var acc_l = 0.0;
    var acc_r = 0.0;
    for (var i = s0; i < s1; i = i + 1u) {
        let v = mix_buckets[BUCKET_OFFSETS + i];
        let base = (v * mix_params.block_size + f) * 2u;
        acc_l = acc_l + voice_out[base];
        acc_r = acc_r + voice_out[base + 1u];
    }
    var acc_rel_l = 0.0;
    var acc_rel_r = 0.0;
    for (var i = r0; i < r1; i = i + 1u) {
        let v = mix_buckets[BUCKET_OFFSETS + i];
        let base = (v * mix_params.block_size + f) * 2u;
        acc_rel_l = acc_rel_l + voice_out[base];
        acc_rel_r = acc_rel_r + voice_out[base + 1u];
    }

    // Per-channel lerp state machine: start from the block-start state and
    // replay this channel's events with frame <= f. Events of other channels
    // do not affect this channel's state, so filtering them out is exact.
    var cur = mix_params.starts[ch];
    var frame_c = 0u;
    var i = 0u;
    while (i < mix_params.event_count && mix_events[i].frame <= f) {
        let ev = mix_events[i];
        if (ev.channel == ch) {
            let n = f32(ev.frame - frame_c);
            cur.vol = lerp_advance(cur.vol, cur.vol_step, cur.vol_end, n);
            cur.expr = lerp_advance(cur.expr, cur.expr_step, cur.expr_end, n);
            cur.pan = lerp_advance(cur.pan, cur.pan_step, cur.pan_end, n);
            frame_c = ev.frame;
            if (ev.cc == 7u) {
                cur.vol_step = (ev.value - cur.vol) / mix_params.lerp_len;
                cur.vol_end = ev.value;
            } else if (ev.cc == 11u) {
                cur.expr_step = (ev.value - cur.expr) / mix_params.lerp_len;
                cur.expr_end = ev.value;
            } else if (ev.cc == 10u || ev.cc == 8u) {
                cur.pan_step = (ev.value - cur.pan) / mix_params.lerp_len;
                cur.pan_end = ev.value;
            }
        }
        i = i + 1u;
    }

    // Advance each controller to frame f.
    let n = f32(f - frame_c);
    let vol = lerp_advance(cur.vol, cur.vol_step, cur.vol_end, n);
    let expr = lerp_advance(cur.expr, cur.expr_step, cur.expr_end, n);
    let amp = (vol * expr) * (vol * expr);

    let pan = lerp_advance(cur.pan, cur.pan_step, cur.pan_end, n);
    let pan_angle = pan * 1.5707963267948966;
    let pan_l = cos(pan_angle);
    let pan_r = sin(pan_angle);

    // Store the raw per-channel sum and the amp/pan factors separately: the
    // reduce pass must evaluate the SAME expression as the old kernel
    // (`out + sum * amp * pan`) so the compiler can fuse it identically.
    // Storing the final product would force an extra f32 rounding and break
    // bit-exactness at the 1-ulp level on multi-channel mixes.
    let out_base = (f * channels + ch) * 6u;
    mix_partial[out_base] = acc_l + acc_rel_l;
    mix_partial[out_base + 1u] = acc_r + acc_rel_r;
    mix_partial[out_base + 2u] = amp;
    mix_partial[out_base + 3u] = pan_l;
    mix_partial[out_base + 4u] = pan_r;
    mix_partial[out_base + 5u] = 0.0;
}
