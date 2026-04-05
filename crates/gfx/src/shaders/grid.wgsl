// 网格线渲染着色器

// Viewport 尺寸 uniform
@group(0) @binding(0)
var<uniform> viewport: vec2<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// 网格线实例数据
struct GridLineInstance {
    @location(0) start: vec2<f32>,    // 起点（屏幕像素）
    @location(1) end: vec2<f32>,      // 终点（屏幕像素）
    @location(2) color: vec4<f32>,    // 颜色
    @location(3) width: f32,          // 线宽
}

// 将屏幕像素坐标转换为 NDC (-1 到 1)
fn screen_to_ndc(screen_pos: vec2<f32>) -> vec2<f32> {
    let ndc_x = (screen_pos.x / viewport.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / viewport.y) * 2.0;  // Y轴翻转
    return vec2<f32>(ndc_x, ndc_y);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: GridLineInstance,
) -> VertexOutput {
    // 计算线条方向
    let line_dir = instance.end - instance.start;
    let line_length = length(line_dir);
    
    // 避免零长度线条
    let dir = select(
        line_dir / line_length,
        vec2<f32>(1.0, 0.0),
        line_length < 0.001
    );
    
    // 计算垂直方向（用于线宽扩展）
    let perp = vec2<f32>(-dir.y, dir.x) * (instance.width * 0.5);
    
    // 根据顶点索引生成线段的四个角（三角形带）
    // 0: 起点-垂直偏移, 1: 起点+垂直偏移, 2: 终点-垂直偏移, 3: 终点+垂直偏移
    var pos: vec2<f32>;
    switch vertex_index {
        case 0u: { // 起点 - 垂直偏移
            pos = instance.start - perp;
        }
        case 1u: { // 起点 + 垂直偏移
            pos = instance.start + perp;
        }
        case 2u: { // 终点 - 垂直偏移
            pos = instance.end - perp;
        }
        case 3u: { // 终点 + 垂直偏移
            pos = instance.end + perp;
        }
        default: {
            pos = instance.start;
        }
    }

    var output: VertexOutput;
    output.position = vec4<f32>(screen_to_ndc(pos), 0.0, 1.0);
    output.color = instance.color;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
