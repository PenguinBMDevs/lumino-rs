// 弯音编辑模式弯曲音符着色器
// 在钢琴卷帘区域渲染"被弯音曲线弯曲"的音符段矩形（梯形近似）。
// CPU 端将每个音符按 tick 细分为多个段，段间采样弯音曲线得到起止 y 偏移，
// 每个段实例渲染一个梯形（上下边平行于键位方向，左右边为斜线），
// 段足够窄时视觉上呈现"音符随曲线柔性弯曲"（曲线模式）或"阶梯折断"（直线模式）。

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

// 弯曲音符段实例 (32 bytes)
// position = [tick_start, y0_top]（逻辑坐标，y 为 key 单位，已含弯音偏移）
// end_tick / y1_top 为段的终点（tick / key）
// 上边 = y_top，下边 = y_top - 1.0（音符厚度 1 key）
struct BendNoteInstance {
    @location(0) position: vec2<f32>,
    @location(1) end_tick: f32,
    @location(2) y1_top: f32,
    @location(3) color_packed: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

fn unpack_color(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 24) & 0xFFu) / 255.0;
    let g = f32((packed >> 16) & 0xFFu) / 255.0;
    let b = f32((packed >> 8) & 0xFFu) / 255.0;
    let a = f32(packed & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: BendNoteInstance,
) -> VertexOutput {
    // 四角（三角形带顺序）：
    // 0: (tick_start, y0_top - 1.0)  起点下边
    // 1: (tick_start, y0_top)        起点上边
    // 2: (end_tick, y1_top - 1.0)    终点下边
    // 3: (end_tick, y1_top)          终点上边
    var tick: f32;
    var key: f32;
    switch vertex_index {
        case 0u: { tick = instance.position.x; key = instance.position.y - 1.0; }
        case 1u: { tick = instance.position.x; key = instance.position.y; }
        case 2u: { tick = instance.end_tick;   key = instance.y1_top - 1.0; }
        default: { tick = instance.end_tick;   key = instance.y1_top; }
    }

    // 逻辑坐标 (tick, key) 转换为屏幕像素坐标
    let screen_x = tick * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - key) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;

    let ndc_x = (screen_x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_y / camera.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = unpack_color(instance.color_packed);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
