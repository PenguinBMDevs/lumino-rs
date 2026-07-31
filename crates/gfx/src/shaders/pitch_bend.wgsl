// 弯音编辑模式专用着色器
// 在钢琴卷帘区域渲染：半透明遮罩 + 锚点圆 + 曲线连线 + 控制柄
// 使用实例化渲染，每个实例是一个弯音图元（遮罩/锚点/线段）

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

// 弯音图元类型
const TYPE_MASK: u32 = 0u;   // 半透明遮罩
const TYPE_ANCHOR: u32 = 1u; // 锚点圆
const TYPE_LINE: u32 = 2u;   // 曲线连线段（水平+垂直阶梯）
const TYPE_BASELINE: u32 = 3u; // 基准线
const TYPE_HANDLE: u32 = 4u; // 贝塞尔控制柄（实心圆点）

// 弯音实例数据 (48 bytes)
struct PitchBendInstance {
    @location(0) screen_pos: vec2<f32>,   // 屏幕空间位置（左上角或起点）
    @location(1) screen_size: vec2<f32>,  // 屏幕空间尺寸（宽高）
    @location(2) color: vec4<f32>,        // RGBA
    @location(3) prim_type: u32,          // 图元类型
    @location(4) radius: f32,             // 锚点半径（仅 TYPE_ANCHOR 使用）
    @location(5) _pad: vec3<f32>,         // 对齐填充
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) screen_size: vec2<f32>,
    @location(3) prim_type: u32,
    @location(4) radius: f32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: PitchBendInstance,
) -> VertexOutput {
    // 矩形四角: 0(0,0) -> 1(1,0) -> 2(0,1) -> 3(1,1)
    let x = f32(vertex_index & 1u);
    let y = f32((vertex_index >> 1u) & 1u);

    // 屏幕空间位置
    let pos = vec2<f32>(
        instance.screen_pos.x + x * instance.screen_size.x,
        instance.screen_pos.y + y * instance.screen_size.y,
    );

    // 转换到 NDC
    let ndc_x = (pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pos.y / camera.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = instance.color;
    output.local_pos = vec2<f32>(x * instance.screen_size.x, y * instance.screen_size.y);
    output.screen_size = instance.screen_size;
    output.prim_type = instance.prim_type;
    output.radius = instance.radius;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (input.prim_type == TYPE_MASK) {
        // 半透明遮罩：直接返回颜色
        return input.color;
    }

    if (input.prim_type == TYPE_BASELINE) {
        // 基准线：直接返回颜色
        return input.color;
    }

    if (input.prim_type == TYPE_LINE) {
        // 曲线连线段：直接返回颜色
        return input.color;
    }

    if (input.prim_type == TYPE_ANCHOR || input.prim_type == TYPE_HANDLE) {
        // 锚点/控制柄圆：用 SDF 渲染（控制柄与锚点共用实心圆逻辑）
        let center = input.screen_size * 0.5;
        let p = input.local_pos - center;
        let dist = length(p);

        // 抗锯齿边缘
        let edge = input.radius - 0.5;
        let alpha = 1.0 - smoothstep(edge, edge + 1.0, dist);

        if (alpha <= 0.0) {
            discard;
        }

        return vec4<f32>(input.color.rgb, input.color.a * alpha);
    }

    return input.color;
}
