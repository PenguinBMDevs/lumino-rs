// CC / 自动化曲线实例渲染着色器
// 使用实例化渲染高效绘制 CC 事件柱状条、自动化线段与圆角锚点

// 视口 Uniform（只需视口尺寸用于 NDC 转换）
struct ViewportUniform {
    viewport_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: ViewportUniform;

// CC / 自动化实例数据
struct CcBarInstance {
    @location(0) position: vec2<f32>,  // x, y (top-left)
    @location(1) size: vec2<f32>,      // width, height
    @location(2) color: vec4<f32>,     // rgba
    @location(3) corner_radius: f32,   // 圆角半径（像素）
    @location(4) border_width: f32,    // 边框宽度（像素，0=无）
};

// 顶点输出
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) corner_radius: f32,
    @location(4) border_width: f32,
};

// 顶点着色器
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: CcBarInstance,
) -> VertexOutput {
    // 矩形顶点顺序: 0(0,0) -> 1(1,0) -> 2(0,1) -> 3(1,1)
    let x = f32(vertex_index & 1u);
    let y = f32((vertex_index >> 1u) & 1u);

    // 计算顶点位置（屏幕空间）
    let pos = vec2<f32>(
        instance.position.x + x * instance.size.x,
        instance.position.y + y * instance.size.y,
    );

    // 转换到 NDC 空间 (-1 to 1)
    let ndc_x = (pos.x / viewport.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pos.y / viewport.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = instance.color;
    output.local_pos = vec2<f32>(x * instance.size.x, y * instance.size.y);
    output.size = instance.size;
    output.corner_radius = instance.corner_radius;
    output.border_width = instance.border_width;

    return output;
}

// 片段着色器：支持圆角矩形（SDF）与边框
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = input.size;
    let cr = min(input.corner_radius, min(size.x, size.y) * 0.5);
    let bw = input.border_width;

    // 直角矩形快速路径
    if cr <= 0.0 && bw <= 0.0 {
        return input.color;
    }

    // 中心为原点的局部坐标
    let p = input.local_pos - size * 0.5;
    let d = sd_rounded_box(p, size, cr);

    // 无圆角但可能有边框：用圆角逻辑处理也能正确工作（cr=0 退化为普通矩形）
    if bw <= 0.0 {
        // 内部填充：平滑抗锯齿
        let alpha = 1.0 - smoothstep(-0.5, 0.5, d);
        if alpha <= 0.0 {
            discard;
        }
        return vec4<f32>(input.color.rgb, input.color.a * alpha);
    }

    // 边框：内部镂空
    let outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d);
    let inner_alpha = 1.0 - smoothstep(-0.5, 0.5, d + bw);
    let alpha = outer_alpha - inner_alpha;
    if alpha <= 0.0 {
        discard;
    }
    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}

// 圆角矩形有符号距离函数
// p: 以矩形中心为原点的像素坐标
// b: 矩形半尺寸
// r: 圆角半径
fn sd_rounded_box(p: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let b = size * 0.5;
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}
