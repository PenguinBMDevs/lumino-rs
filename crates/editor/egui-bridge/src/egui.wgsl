// 与 egui-wgpu 0.37 的 gamma 路径一致的 egui 绘制着色器。
// 目标为 sRGB 时在片元阶段把 gamma 颜色转成线性，交给硬件重新编码。
//
// 本桥固定使用确定性双线性过滤 + ClampToEdge（只上传字体图集与线性图标），
// 因此不引入 egui-wgpu 的纹理 flag 分支，也不做 dithering。

struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @location(1) color: vec4<f32>,
    @builtin(position) position: vec4<f32>,
};

struct Locals {
    screen_size: vec2<f32>,
    pixels_per_point: f32,
    _padding: f32,
};
@group(0) @binding(0) var<uniform> r_locals: Locals;

@group(1) @binding(0) var r_tex_color: texture_2d<f32>;
@group(1) @binding(1) var r_tex_sampler: sampler;

// 0-1 linear from 0-1 sRGB gamma
fn linear_from_gamma_rgb(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = srgb < vec3<f32>(0.04045);
    let lower = srgb / vec3<f32>(12.92);
    let higher = pow((srgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, cutoff);
}

// [u8; 4] sRGB as u32 -> [r, g, b, a] in 0..1
fn unpack_color(color: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(color & 255u),
        f32((color >> 8u) & 255u),
        f32((color >> 16u) & 255u),
        f32((color >> 24u) & 255u),
    ) / 255.0;
}

@vertex
fn vs_main(
    @location(0) a_pos: vec2<f32>,
    @location(1) a_tex_coord: vec2<f32>,
    @location(2) a_color: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let screen_pos = a_pos * r_locals.pixels_per_point;
    out.tex_coord = a_tex_coord;
    out.color = unpack_color(a_color);
    out.position = vec4<f32>(
        2.0 * screen_pos.x / r_locals.screen_size.x - 1.0,
        1.0 - 2.0 * screen_pos.y / r_locals.screen_size.y,
        0.0,
        1.0,
    );
    return out;
}

// 与 egui-wgpu `predictable_texture_filtering` 相同的四抽头双线性 + clamp。
fn sample_texture(in: VertexOutput) -> vec4<f32> {
    let texture_size = vec2<i32>(textureDimensions(r_tex_color, 0));
    let texture_size_f = vec2<f32>(texture_size);
    let max_coord = texture_size - vec2<i32>(1, 1);

    let pixel_coord = in.tex_coord * texture_size_f - 0.5;
    let pixel_fract = fract(pixel_coord);
    let pixel_floor = vec2<i32>(floor(pixel_coord));

    let p00 = clamp(pixel_floor + vec2<i32>(0, 0), vec2<i32>(0, 0), max_coord);
    let p10 = clamp(pixel_floor + vec2<i32>(1, 0), vec2<i32>(0, 0), max_coord);
    let p01 = clamp(pixel_floor + vec2<i32>(0, 1), vec2<i32>(0, 0), max_coord);
    let p11 = clamp(pixel_floor + vec2<i32>(1, 1), vec2<i32>(0, 0), max_coord);

    let tl = textureLoad(r_tex_color, p00, 0);
    let tr = textureLoad(r_tex_color, p10, 0);
    let bl = textureLoad(r_tex_color, p01, 0);
    let br = textureLoad(r_tex_color, p11, 0);

    let top = mix(tl, tr, pixel_fract.x);
    let bottom = mix(bl, br, pixel_fract.x);
    return mix(top, bottom, pixel_fract.y);
}

@fragment
fn fs_main_gamma(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color * sample_texture(in);
}

@fragment
fn fs_main_srgb(in: VertexOutput) -> @location(0) vec4<f32> {
    let out_color_gamma = in.color * sample_texture(in);
    return vec4<f32>(linear_from_gamma_rgb(out_color_gamma.rgb), out_color_gamma.a);
}
