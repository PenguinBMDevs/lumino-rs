// 洋葱皮实例化渲染着色器 — 参考 Wasabi 瀑布流实现，旋转为钢琴卷帘方向（X=time, Y=pitch）
//
// 相比旧版：
// - 移除了 compute shader 间接绘制（不再使用 instance_indices buffer）
// - 直接从 note_pool 读取 OnionNote 数据，vertex shader 生成 4 角
// - GPU 自动裁剪超出 [-1, 1] 的音符
//
// 参考 Wasabi:
// - notes.geom: 瀑布流方向（Y=time, X=pitch），这里旋转为 X=time, Y=pitch
// - notes.frag: 颜色 + 边框渲染
//
// 坐标系：
// - X: 时间轴 (start_tick → end_tick)
// - Y: 音高轴 (key 从低到高，Y 坐标从下到上)

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
@group(0) @binding(1) var<storage, read> note_pool: array<OnionNote>;

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

fn unpack_track_idx(packed: u32) -> u32 {
    return (packed >> 8u) & 0xFFFFu;
}

fn unpack_color_rgba(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 24u) & 0xFFu) / 255.0;
    let g = f32((packed >> 16u) & 0xFFu) / 255.0;
    let b = f32((packed >> 8u) & 0xFFu) / 255.0;
    let a = f32(packed & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

// Wasabi-style: 4 corners of a note quad
// vertex_index 0-3 maps to (left,top), (right,top), (left,bottom), (right,bottom)
// Piano roll direction: X=time, Y=pitch
fn get_corner_offset(idx: u32) -> vec2<f32> {
    switch idx {
        case 0u: { return vec2<f32>(0.0, 0.0); } // top-left
        case 1u: { return vec2<f32>(1.0, 0.0); } // top-right
        case 2u: { return vec2<f32>(0.0, 1.0); } // bottom-left
        case 3u: { return vec2<f32>(1.0, 1.0); } // bottom-right
        default: { return vec2<f32>(0.0, 0.0); }
    }
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_id: u32,
) -> VertexOutput {
    let note = note_pool[instance_id];
    let track_idx = unpack_track_idx(note.packed);

    // 排除当前编辑音轨
    if (track_idx == viewport.current_track) {
        // 返回不可见顶点
        var output: VertexOutput;
        output.position = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        output.color = vec4<f32>(0.0);
        output.tex_coord = vec2<f32>(0.0);
        output.note_size = vec2<f32>(0.0);
        output.win_size = vec2<f32>(0.0);
        output.border_width = 0.0;
        return output;
    }

    let start_tick_f = f32(note.start_tick);
    let end_tick_f = f32(note.end_tick);
    let length = end_tick_f - start_tick_f;
    let pitch = unpack_pitch(note.packed);

    // 坐标变换：钢琴卷帘方向 (X=time, Y=pitch)
    // 与 note_renderer 的 CameraUniform 坐标计算保持一致
    let screen_x = start_tick_f * viewport.zoom_x - viewport.scroll_x
                   + viewport.keyboard_width + viewport.canvas_offset_x;
    let screen_y = (viewport.max_key_index - f32(pitch)) * viewport.zoom_y
                   - viewport.scroll_y + viewport.ruler_height + viewport.canvas_offset_y;

    let screen_w = length * viewport.zoom_x;
    let screen_h = viewport.zoom_y;

    // Wasabi-style: 用 corner offset 展开 4 顶点
    let corner = get_corner_offset(vertex_index);
    let screen_pos = vec2<f32>(screen_x, screen_y) + corner * vec2<f32>(screen_w, screen_h);

    // 屏幕坐标 → NDC
    let ndc_x = (screen_pos.x / viewport.canvas_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / viewport.canvas_height) * 2.0;

    let color = unpack_color_rgba(note.color_packed);

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.color = color;
    output.tex_coord = corner;
    output.note_size = vec2<f32>(screen_w, screen_h);
    output.win_size = vec2<f32>(viewport.canvas_width, viewport.canvas_height);
    output.border_width = 1.0; // 默认 1px 边框
    return output;
}

// Fragment shader: 纯色 + 边框（无渐变 — 移除 Wasabi 的余弦效果）
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let v_uv = input.tex_coord;
    var color = input.color.rgb;

    // 边框计算
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

    // sRGB approximization
    color *= color;
    return vec4<f32>(color, input.color.a);
}
