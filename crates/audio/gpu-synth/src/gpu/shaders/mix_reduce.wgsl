// lumino-gpu-synth mix kernel (pass 2b: channel reduce).
//
// REF-LGS-001: sums the per-channel partials produced by `mix.wgsl` in
// ASCENDING channel order - the same order the old single-thread kernel used
// for its `out_l/out_r` loop - so the final output stays bit-identical.
//
//   outL = (((p[0].l + p[1].l) + p[2].l) + ...)
//   outR = (((p[0].r + p[1].r) + p[2].r) + ...)

struct MixParams {
    voice_count: u32,
    block_size: u32,
    channel_count: u32,
    event_count: u32,
    lerp_len: f32,
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

@group(0) @binding(1) var<storage, read_write> output: array<f32>;
@group(0) @binding(4) var<uniform> mix_params: MixParams;
@group(0) @binding(5) var<storage, read_write> mix_partial: array<f32>;

@compute
@workgroup_size(128)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let f = gid.x;
    let channels = mix_params.channel_count;
    if (f >= mix_params.block_size) {
        return;
    }

    var out_l = 0.0;
    var out_r = 0.0;
    let base = f * channels * 6u;
    for (var c = 0u; c < channels; c = c + 1u) {
        let o = base + c * 6u;
        out_l = out_l + mix_partial[o] * mix_partial[o + 2u] * mix_partial[o + 3u];
        out_r = out_r + mix_partial[o + 1u] * mix_partial[o + 2u] * mix_partial[o + 4u];
    }

    output[f * 2u] = out_l;
    output[f * 2u + 1u] = out_r;
}
