// 高精度贴图着色器
// 每个整合组贴图覆盖一个 area 矩形（framebuffer 像素坐标），
// vertex shader 将 area 映射到 NDC，fragment shader 直接采样贴图。

struct Uniform {
    area_x: f32,
    area_y: f32,
    area_w: f32,
    area_h: f32,
    canvas_w: f32,
    canvas_h: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0)
var<uniform> u: Uniform;

@group(0) @binding(1)
var texture: texture_2d<f32>;

@group(0) @binding(2)
var tex_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // 两个三角形组成的 quad（6 顶点）
    let positions = array(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let p = positions[idx];

    // area 矩形内的 framebuffer 像素坐标
    let px = u.area_x + p.x * u.area_w;
    let py = u.area_y + p.y * u.area_h;

    // framebuffer 像素 → NDC（Y 轴翻转：framebuffer Y 向下，NDC Y 向上）
    let ndc_x = px / u.canvas_w * 2.0 - 1.0;
    let ndc_y = 1.0 - py / u.canvas_h * 2.0;

    var output: VertexOutput;
    output.position = vec4(ndc_x, ndc_y, 0.0, 1.0);
    // UV: x 直接（左到右），y 翻转（贴图 row 0 = key 0 在底部）
    output.uv = vec2(p.x, 1.0 - p.y);
    return output;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(texture, tex_sampler, in.uv);
}
