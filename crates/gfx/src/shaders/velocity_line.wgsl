// 速度/CC/Bend 折线段渲染着色器
// 使用实例化绘制折线段：每个实例 = start → end 的一条 2px 宽线段
// 顶点生成方式：vertex_index 映射到 4 个角点

struct ViewportUniform {
    viewport_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: ViewportUniform;

struct LineInstance {
    @location(0) start: vec2<f32>,  // 线段起点
    @location(1) end: vec2<f32>,    // 线段终点
    @location(2) color: vec4<f32>, // 颜色 RGBA
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: LineInstance,
) -> VertexOutput {
    let dir = instance.end - instance.start;
    let len = length(dir);
    let thickness = 2.0;

    // 单位方向向量和法向量
    let ndir = select(vec2(1.0, 0.0), dir / len, len > 0.001);
    let normal = vec2(-ndir.y, ndir.x);

    // vertex_index: 0→start- 1→start+ 2→end- 3→end+
    let start_t = f32(1 - (vertex_index >> 1u));    // vi 0,1→1.0; vi 2,3→0.0
    let end_t = f32(vertex_index >> 1u);             // vi 0,1→0.0; vi 2,3→1.0
    let normal_sign = f32((vertex_index & 1u) * 2u - 1u); // vi 0,2→-1; vi 1,3→1

    let pos = instance.start * start_t + instance.end * end_t + normal * normal_sign * (thickness / 2.0);

    // 转换到 NDC
    let ndc_x = (pos.x / viewport.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pos.y / viewport.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = instance.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
