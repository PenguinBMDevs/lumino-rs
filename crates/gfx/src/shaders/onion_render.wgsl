// 洋葱皮实例化渲染着色器 — GPU compute cull → indirect draw
//
// Vertex shader 通过 instance_indices buffer 获取被 cull 后的音符索引，
// 再从 note_pool 读取音符数据，生成 4 顶点 quad。

struct OnionViewportUniform {
    tick_start: f32,
    tick_end: f32,
    pitch_min: f32,
    pitch_max: f32,
    note_count: u32,
    current_track: u32,
    keyboard_width: f32,
    ruler_height: f32,
    canvas_width: f32,
    canvas_height: f32,
    canvas_offset_x: f32,
    canvas_offset_y: f32,
    scroll_x: f32,
    scroll_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    max_key_index: f32,
};

struct OnionNote {
    start_tick: u32,
    end_tick: u32,
    packed: u32,
    color_packed: u32,
};

@group(0) @binding(0) var<uniform> viewport: OnionViewportUniform;
@group(0) @binding(1) var<storage, read> instance_indices: array<u32>;
@group(0) @binding(2) var<storage, read> note_pool: array<OnionNote>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) note_size: vec2<f32>,
    @location(3) win_size: vec2<f32>,
    @location(4) border_width: f32,
};

fn unpack_pitch(packed: u32) -> u32 {
    return packed & 0xFFu;
}

fn unpack_color_rgba(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 24u) & 0xFFu) / 255.0;
    let g = f32((packed >> 16u) & 0xFFu) / 255.0;
    let b = f32((packed >> 8u) & 0xFFu) / 255.0;
    let a = f32(packed & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

fn get_corner_offset(idx: u32) -> vec2<f32> {
    switch idx {
        case 0u: { return vec2<f32>(0.0, 0.0); }
        case 1u: { return vec2<f32>(1.0, 0.0); }
        case 2u: { return vec2<f32>(0.0, 1.0); }
        case 3u: { return vec2<f32>(1.0, 1.0); }
        default: { return vec2<f32>(0.0, 0.0); }
    }
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    // 通过 instance_indices 间接索引 note_pool（compute shader 已做视口剔除）
    let note_index = instance_indices[instance_id];
    let note = note_pool[note_index];

    let start_tick_f = f32(note.start_tick);
    let end_tick_f = f32(note.end_tick);
    let length = end_tick_f - start_tick_f;
    let pitch = unpack_pitch(note.packed);

    let screen_x = start_tick_f * viewport.zoom_x - viewport.scroll_x
                   + viewport.keyboard_width + viewport.canvas_offset_x;
    let screen_y = (viewport.max_key_index - f32(pitch)) * viewport.zoom_y
                   - viewport.scroll_y + viewport.ruler_height + viewport.canvas_offset_y;

    let screen_w = length * viewport.zoom_x;
    let screen_h = viewport.zoom_y;

    let corner = get_corner_offset(vertex_index);
    let screen_pos = vec2<f32>(screen_x, screen_y) + corner * vec2<f32>(screen_w, screen_h);

    let ndc_x = (screen_pos.x / viewport.canvas_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / viewport.canvas_height) * 2.0;

    let color = unpack_color_rgba(note.color_packed);

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = color;
    output.tex_coord = corner;
    output.note_size = vec2<f32>(screen_w, screen_h);
    output.win_size = vec2<f32>(viewport.canvas_width, viewport.canvas_height);
    output.border_width = 1.0;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let v_uv = input.tex_coord;
    var color = input.color.rgb;

    let horiz_width_pixels = max(input.note_size.x / 2.0 * input.win_size.x, 1.0);
    let vert_width_pixels = max(input.note_size.y / 2.0 * input.win_size.y, 1.0);
    let horiz_margin = input.border_width / horiz_width_pixels;
    let vert_margin = input.border_width / vert_width_pixels;

    let is_border = v_uv.x < horiz_margin
                 || v_uv.x > 1.0 - horiz_margin
                 || v_uv.y < vert_margin
                 || v_uv.y > 1.0 - vert_margin;

    if (is_border) {
        color = input.color.rgb * 0.2;
    }

    color *= color;
    return vec4<f32>(color, input.color.a);
}
