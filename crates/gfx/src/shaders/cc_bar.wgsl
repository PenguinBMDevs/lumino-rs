// CC 控制器柱状图渲染着色器
// 使用实例化渲染高效绘制 CC 事件柱状条
// 计算方式与 yinhe 一致：底部对齐，高度 = value/127 * panel_height

// 视口 Uniform（只需视口尺寸用于 NDC 转换）
struct ViewportUniform {
    viewport_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: ViewportUniform;

// CC 柱状条实例数据
struct CcBarInstance {
    @location(0) position: vec2<f32>,  // x, y (top-left)
    @location(1) size: vec2<f32>,      // width, height
    @location(2) color: vec4<f32>,     // rgba
};

// 顶点输出
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
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
    
    return output;
}

// 片段着色器
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
