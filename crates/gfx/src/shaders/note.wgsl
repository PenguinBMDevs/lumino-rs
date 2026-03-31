// 音符渲染着色器
// 是社么写的吗不是

// Viewport 尺寸 uniform
@group(0) @binding(0)
var<uniform> viewport: vec2<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// 实例数据
struct NoteInstance {
    @location(0) position: vec2<f32>,  // 左上角位置（屏幕像素）
    @location(1) size: vec2<f32>,      // 宽高（像素）
    @location(2) color: vec4<f32>,     // 颜色
}

// 将屏幕像素坐标转换为 NDC (-1 到 1)
fn screen_to_ndc(screen_pos: vec2<f32>) -> vec2<f32> {
    // NDC.x = (screen_x / viewport_width) * 2 - 1
    // NDC.y = 1 - (screen_y / viewport_height) * 2  (Y轴翻转)
    let ndc_x = (screen_pos.x / viewport.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / viewport.y) * 2.0;
    return vec2<f32>(ndc_x, ndc_y);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: NoteInstance,
) -> VertexOutput {
    // 根据顶点索引生成矩形的四个角（三角形带顺序）
    // 0: 左上, 1: 左下, 2: 右上, 3: 右下
    var local_offset: vec2<f32>;

    switch vertex_index {
        case 0u: { // 左上
            local_offset = vec2<f32>(0.0, 0.0);
        }
        case 1u: { // 左下
            local_offset = vec2<f32>(0.0, 1.0);
        }
        case 2u: { // 右上
            local_offset = vec2<f32>(1.0, 0.0);
        }
        case 3u: { // 右下
            local_offset = vec2<f32>(1.0, 1.0);
        }
        default: {
            local_offset = vec2<f32>(0.0, 0.0);
        }
    }

    // 计算屏幕空间位置
    let screen_pos = instance.position + local_offset * instance.size;

    // 转换为 NDC
    let ndc_pos = screen_to_ndc(screen_pos);

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_pos, 0.0, 1.0);
    output.color = instance.color;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
