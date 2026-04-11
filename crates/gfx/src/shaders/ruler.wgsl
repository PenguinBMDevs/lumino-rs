// 时间轴标尺渲染着色器
// 使用实例化渲染高效绘制标尺刻度

// 视口 Uniform
struct ViewportUniform {
    viewport_size: vec2<f32>,
    ruler_height: f32,
    keyboard_width: f32,
    scroll_x: f32,
    zoom_x: f32,
    ticks_per_measure: f32,
    ticks_per_beat: f32,
};

@group(0) @binding(0)
var<uniform> viewport: ViewportUniform;

// 标尺刻度实例数据
struct RulerTickInstance {
    @location(0) position: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) tick_type: f32,
    @location(4) tick_value: f32,
};

// 顶点输出
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) tick_type: f32,
    @location(2) uv: vec2<f32>,
};

// 顶点着色器
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: RulerTickInstance,
) -> VertexOutput {
    // 矩形顶点顺序: 0(0,0) -> 1(1,0) -> 2(0,1) -> 3(1,1)
    let x = f32(vertex_index & 1u);
    let y = f32((vertex_index >> 1u) & 1u);
    
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
    output.tick_type = instance.tick_type;
    output.uv = vec2<f32>(x, y);
    
    return output;
}

// 片段着色器
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = input.color;
    
    // 根据刻度类型添加不同的视觉效果
    if (input.tick_type < 0.5) {
        // 小节线 - 添加顶部高亮
        if (input.uv.y < 0.1) {
            color = vec4<f32>(color.rgb * 1.2, color.a);
        }
    } else if (input.tick_type < 1.5) {
        // 拍线 - 稍微暗一点
        color = vec4<f32>(color.rgb * 0.9, color.a);
    } else {
        // 细分线 - 更淡
        color = vec4<f32>(color.rgb * 0.8, color.a * 0.7);
    }
    
    return color;
}
