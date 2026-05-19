// 洋葱皮实例化渲染着色器
// 使用 draw_indexed_indirect 绘制单位矩形
// 顶点着色器通过实例 ID 读取实例索引缓冲区 → 原始音符索引 → 计算屏幕坐标和颜色

struct CameraUniform {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: f32,
};

struct TrackColor {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
};

struct OnionTrackColors {
    colors: array<TrackColor, 64>,
};

struct OnionNote {
    start_tick: u32,
    end_tick: u32,
    packed: u32,
    _padding: u32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> track_colors: OnionTrackColors;
@group(0) @binding(2) var<storage, read> instance_indices: array<u32>;
@group(0) @binding(3) var<storage, read> note_pool: array<OnionNote>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

fn unpack_pitch(packed: u32) -> u32 {
    return packed & 0xFFu;
}

fn unpack_track_idx(packed: u32) -> u32 {
    return (packed >> 8u) & 0xFFFFu;
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    // 从实例索引缓冲区读取原始音符索引
    let note_index = instance_indices[instance_id];
    let note = note_pool[note_index];

    let start_tick_f = f32(note.start_tick);
    let end_tick_f = f32(note.end_tick);
    let length = end_tick_f - start_tick_f;
    let pitch = unpack_pitch(note.packed);
    let track = unpack_track_idx(note.packed);

    // 单位矩形顶点偏移（三角形带：6个顶点）
    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 2u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 4u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 1.0); }
    }

    let key_f = f32(pitch);
    let screen_x = start_tick_f * camera.zoom.x - camera.scroll.x
                   + camera.keyboard_width + camera.canvas_offset.x;
    let screen_y = (camera.max_key_index - key_f) * camera.zoom.y
                   - camera.scroll.y + camera.ruler_height + camera.canvas_offset.y;
    let screen_size = vec2<f32>(length * camera.zoom.x, camera.zoom.y);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    // 从轨道颜色表查找颜色
    let color_idx = min(track, 63u);
    let tc = track_colors.colors[color_idx];

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = vec4<f32>(tc.r, tc.g, tc.b, tc.a);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}