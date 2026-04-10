// 音符渲染着色器
// CPU 传逻辑坐标 (tick, key, length)，GPU 负责变换到屏幕/NDC 空间

struct CameraUniform {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// 实例数据（逻辑坐标）
struct NoteInstance {
    @location(0) position: vec2<f32>,  // [tick, key]
    @location(1) size: vec2<f32>,      // [length, 1.0]
    @location(2) color: vec4<f32>,     // 颜色
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: NoteInstance,
) -> VertexOutput {
    // 根据顶点索引生成矩形的四个角（三角形带顺序）
    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 2u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 0.0); }
    }

    // 将逻辑坐标 (tick, key) 转换为屏幕像素坐标
    let screen_x = instance.position.x * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - instance.position.y) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    let screen_size = vec2<f32>(instance.size.x * camera.zoom.x, camera.zoom.y);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    // 转换为 NDC
    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = instance.color;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
