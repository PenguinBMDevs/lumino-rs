// lumino-gpu-synth mix kernel (pass 2).
//
// Sums all voices produced by the render pass into the stereo output,
// grouped per MIDI channel, and applies each channel's volume/expression/pan
// controllers with the same 10 ms linear smoothing (`ValueLerp`) semantics
// as XSynth.
//
// Controller events are applied frame-exactly: every mix thread (one per
// output frame) starts from the per-channel block-start lerp state and
// replays this block's events with `frame <= f`, exactly like XSynth's CPU
// loop. The output therefore does not depend on the block size nor on how
// many events a block contains.
//
//   vol  = lerp(vol, vol_step, vol_end, frames advanced)
//   amp  = (vol * expr)^2
//   pan  = lerp(pan, pan_step, pan_end, frames advanced)
//   outL = sum * amp * cos(pan * PI/2)
//   outR = sum * amp * sin(pan * PI/2)

struct MixParams {
    voice_count: u32,
    block_size: u32,
    channel_count: u32,
    event_count: u32,
    lerp_len: f32, // sample_rate * 0.01 (10 ms)
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    starts: array<MixStart, 16>,
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

const MAX_CHANNELS: u32 = 16u;

@group(0) @binding(0) var<storage, read> voice_out: array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;
@group(0) @binding(2) var<storage, read> voice_chans: array<u32>;
@group(0) @binding(3) var<storage, read> mix_events: array<MixEvent>;
@group(0) @binding(4) var<uniform> mix_params: MixParams;

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
// whole block, preserving the waveform exactly. A per-sample saturation in
// the shader was tried first but had to squeeze the entire 1.0..~700 range
// into the 1.0..1.05 band, flat-topping every overloaded block into
// square-wave distortion - worse than clipping.

@compute
@workgroup_size(128)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.x;
    if (f >= mix_params.block_size) {
        return;
    }

    // Accumulate voices grouped by channel. Released voices are summed
    // separately so the grouping stays correct, but BOTH groups get the
    // per-channel amp: XSynth applies the channel volume (CC7/CC11) as a
    // post-mix gain over the WHOLE channel buffer (`apply_channel_effects`,
    // vol^3 over every sample including release tails), so skipping the amp
    // for released voices would step the output the moment a voice enters
    // release (amplitude jumps from vol^2 to 1.0) - an audible click that
    // shows up in dense MIDI as a continuous crackle.
    var acc_l: array<f32, MAX_CHANNELS>;
    var acc_r: array<f32, MAX_CHANNELS>;
    var acc_rel_l: array<f32, MAX_CHANNELS>;
    var acc_rel_r: array<f32, MAX_CHANNELS>;
    for (var c = 0u; c < MAX_CHANNELS; c = c + 1u) {
        acc_l[c] = 0.0;
        acc_r[c] = 0.0;
        acc_rel_l[c] = 0.0;
        acc_rel_r[c] = 0.0;
    }

    for (var v = 0u; v < mix_params.voice_count; v = v + 1u) {
        let base = (v * mix_params.block_size + f) * 2u;
        let vc = voice_chans[v];
        let ch = vc & (MAX_CHANNELS - 1u);
        let released = (vc >> 7u) & 1u;
        if (released == 0u) {
            acc_l[ch] = acc_l[ch] + voice_out[base];
            acc_r[ch] = acc_r[ch] + voice_out[base + 1u];
        } else {
            acc_rel_l[ch] = acc_rel_l[ch] + voice_out[base];
            acc_rel_r[ch] = acc_rel_r[ch] + voice_out[base + 1u];
        }
    }

    // Per-channel lerp state machines, starting from the block-start state
    // and replaying every event with frame <= f.
    var cur: array<MixStart, MAX_CHANNELS>;
    var frames: array<u32, MAX_CHANNELS>;
    for (var c = 0u; c < MAX_CHANNELS; c = c + 1u) {
        cur[c] = mix_params.starts[c];
        frames[c] = 0u;
    }

    var i = 0u;
    while (i < mix_params.event_count && mix_events[i].frame <= f) {
        let ev = mix_events[i];
        let c = ev.channel & (MAX_CHANNELS - 1u);
        let n = f32(ev.frame - frames[c]);
        cur[c].vol = lerp_advance(cur[c].vol, cur[c].vol_step, cur[c].vol_end, n);
        cur[c].expr = lerp_advance(cur[c].expr, cur[c].expr_step, cur[c].expr_end, n);
        cur[c].pan = lerp_advance(cur[c].pan, cur[c].pan_step, cur[c].pan_end, n);
        frames[c] = ev.frame;
        if (ev.cc == 7u) {
            cur[c].vol_step = (ev.value - cur[c].vol) / mix_params.lerp_len;
            cur[c].vol_end = ev.value;
        } else if (ev.cc == 11u) {
            cur[c].expr_step = (ev.value - cur[c].expr) / mix_params.lerp_len;
            cur[c].expr_end = ev.value;
        } else if (ev.cc == 10u || ev.cc == 8u) {
            cur[c].pan_step = (ev.value - cur[c].pan) / mix_params.lerp_len;
            cur[c].pan_end = ev.value;
        }
        i = i + 1u;
    }

    let half_pi = 1.5707963267948966;

    var out_l = 0.0;
    var out_r = 0.0;

    for (var ch = 0u; ch < mix_params.channel_count; ch = ch + 1u) {
        if (ch >= MAX_CHANNELS) {
            break;
        }
        // Advance each controller to frame f.
        let n = f32(f - frames[ch]);
        let vol = lerp_advance(cur[ch].vol, cur[ch].vol_step, cur[ch].vol_end, n);
        let expr = lerp_advance(cur[ch].expr, cur[ch].expr_step, cur[ch].expr_end, n);
        let amp = (vol * expr) * (vol * expr);

        let pan = lerp_advance(cur[ch].pan, cur[ch].pan_step, cur[ch].pan_end, n);
        let pan_angle = pan * half_pi;
        let pan_l = cos(pan_angle);
        let pan_r = sin(pan_angle);

        out_l = out_l + (acc_l[ch] + acc_rel_l[ch]) * amp * pan_l;
        out_r = out_r + (acc_r[ch] + acc_rel_r[ch]) * amp * pan_r;
    }

    output[f * 2u] = out_l;
    output[f * 2u + 1u] = out_r;
}
