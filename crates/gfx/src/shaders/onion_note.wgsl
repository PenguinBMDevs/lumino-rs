// 洋葱皮音符渲染着色器 — 基于 note.wgsl，不透明 + 无边框
//
// 与 note.wgsl 的差异：
//   - alpha = 1.0（不透明参考层，用户要求改为不透明以便清晰查看）
//   - 无边框绘制（洋葱皮是背景参考，不需要边框强调）
//   - 无预览哨兵分支（洋葱皮无预览音符）
//
// 复用 NoteInstance 16 bytes 布局（与 wasabi NoteVertex 严格对齐）
// VS 用 instancing + 4 顶点 quad 复刻 wasabi GS「点扩展为 quad」逻辑

const ONION_SKIN_ALPHA: f32 = 1.0;

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

// 实例数据（16 bytes，与 wasabi NoteVertex 字段对齐）
struct NoteInstance {
    @location(0) start_length: vec2<f32>,  // [start_tick, length_tick]
    @location(1) key_color: u32,           // 低8位=key, 高24位=RGB
    @location(2) border_width: u32,        // 洋葱皮不使用（保留字段对齐）
};

/// 解包 key_color → vec4 RGBA（alpha 恒为 1.0，与 wasabi 一致）
fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
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
    let tick = instance.start_length.x;
    let length = instance.start_length.y;
    // key 从 key_color 低 8 位解码（与 wasabi 一致）
    let key = f32(instance.key_color & 0xFFu);

    let screen_x = tick * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - key) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    let screen_size = vec2<f32>(length * camera.zoom.x, camera.zoom.y);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    // 转换为 NDC
    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = unpack_key_color(instance.key_color);

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 洋葱皮：不透明，无边框，无 gamma（背景参考层）
    return vec4<f32>(input.color.rgb, ONION_SKIN_ALPHA);
}
