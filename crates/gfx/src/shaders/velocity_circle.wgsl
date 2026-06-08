// 速度/CC/Bend 控制点渲染着色器
// 每个实例 = 一个圆点，用 quad + fragment 裁剪实现圆形

struct ViewportUniform {
    viewport_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: ViewportUniform;

struct CircleInstance {
    @location(0) center: vec2<f32>, // 圆心
    @location(1) radius: f32,       // 半径
    @location(2) color: vec4<f32>,  // 颜色 RGBA
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,     // quad 局部坐标 [-1, 1]
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: CircleInstance,
) -> VertexOutput {
    // quad 顶点: 0(-,-) 1(+,-) 2(-,+) 3(+,+)
    let x = f32(vertex_index & 1u) * 2.0 - 1.0;  // -1 或 +1
    let y = f32((vertex_index >> 1u) & 1u) * 2.0 - 1.0; // -1 或 +1

    let pos = vec2<f32>(
        instance.center.x + x * instance.radius,
        instance.center.y + y * instance.radius,
    );

    // NDC
    let ndc_x = (pos.x / viewport.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pos.y / viewport.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.uv = vec2<f32>(x, y);
    output.color = instance.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 距圆心距离 > 1 → 丢弃（圆形裁剪）
    let dist_sq = input.uv.x * input.uv.x + input.uv.y * input.uv.y;
    if dist_sq > 1.0 {
        discard;
    }
    return input.color;
}
