// 钢琴键盘渲染着色器
// 使用实例化渲染高效绘制大量琴键

// 视口 Uniform
struct ViewportUniform {
    viewport_size: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    scroll_y: f32,
    zoom_y: f32,
    visible_key_count: f32,
};

@group(0) @binding(0)
var<uniform> viewport: ViewportUniform;

// 琴键实例数据
struct KeyInstance {
    @location(0) position: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) is_black: f32,
    @location(4) key_index: f32,
};

// 顶点输出
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) is_black: f32,
    @location(2) key_index: f32,
    @location(3) uv: vec2<f32>,
};

// 顶点着色器
// 每个实例绘制一个矩形（4个顶点，TriangleStrip）
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: KeyInstance,
) -> VertexOutput {
    // 矩形顶点顺序: 0(0,0) -> 1(1,0) -> 2(0,1) -> 3(1,1)
    let x = f32(vertex_index & 1u);  // 0 or 1
    let y = f32((vertex_index >> 1u) & 1u);  // 0 or 1
    
    // 计算顶点位置
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
    output.is_black = instance.is_black;
    output.key_index = instance.key_index;
    output.uv = vec2<f32>(x, y);
    
    return output;
}

// 片段着色器
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = input.color;
    
    // 添加简单的边框效果
    let border_width = 0.02;
    let is_border = input.uv.x < border_width || 
                    input.uv.x > (1.0 - border_width) ||
                    input.uv.y < border_width || 
                    input.uv.y > (1.0 - border_width);
    
    if (is_border) {
        // 边框颜色（稍微暗一点）
        color = vec4<f32>(color.rgb * 0.8, color.a);
    }
    
    // 黑键添加渐变效果
    if (input.is_black > 0.5) {
        // 简单的垂直渐变
        let gradient = 1.0 - input.uv.y * 0.3;
        color = vec4<f32>(color.rgb * gradient, color.a);
    }
    
    return color;
}
