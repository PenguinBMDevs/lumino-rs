// 纵向卷帘音符着色器 — 横向 note.wgsl 转置版
// 复用同一 NoteInstance 布局与 GPU 缓冲，仅绘制坐标转置：
//   横向：x = tick*zoom_x - scroll_x + keyboard_width, y = (max_key - key)*zoom_y - scroll_y + ruler, size=(len*zoom_x, zoom_y)
//   纵向：x = key*zoom_y - scroll_y           , y = tick*zoom_x - scroll_x + ruler,        size=(zoom_y, len*zoom_x)
// 键盘位于底部，故 X 不再叠加 keyboard_width；Y 仍叠加 ruler_height。
// 样式完移植：同款 unpack、描边、预览哨兵、深度编码（主轨/洋葱皮/预览）、圆角/边框打包。

const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;
const PREVIEW_ALPHA: f32 = 0.7;
const BORDER_DARKEN_FACTOR: f32 = 0.4;

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

struct NoteInstance {
    start_length: vec2<f32>,
    key_color: u32,
    border_width: u32,
}
@group(0) @binding(2)
var<storage, read> all_instances: array<NoteInstance>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) screen_size: vec2<f32>,
    @location(3) border_width: u32,
};

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
    @location(0) visible_index: u32,
) -> VertexOutput {
    let instance = all_instances[visible_index];

    var local_offset: vec2<f32>;
    switch vertex_index {
        case 0u: { local_offset = vec2<f32>(0.0, 0.0); }
        case 1u: { local_offset = vec2<f32>(0.0, 1.0); }
        case 2u: { local_offset = vec2<f32>(1.0, 0.0); }
        case 3u: { local_offset = vec2<f32>(1.0, 1.0); }
        default: { local_offset = vec2<f32>(0.0, 0.0); }
    }

    let tick = instance.start_length.x;
    let length = instance.start_length.y;
    let key = f32(instance.key_color & 0xFFu);

    // 纵向转置：X = key * zoom_y - scroll_y, Y = tick * zoom_x - scroll_x + ruler
    let screen_x = key * camera.zoom.y - camera.scroll.y + camera.canvas_offset.x;
    let screen_y = tick * camera.zoom.x - camera.scroll.x + camera.ruler_height + camera.canvas_offset.y;
    let screen_size = vec2<f32>(camera.zoom.y, length * camera.zoom.x);

    let screen_pos = vec2<f32>(screen_x, screen_y) + local_offset * screen_size;

    let ndc_x = (screen_pos.x / camera.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_pos.y / camera.viewport_size.y) * 2.0;

    let track = instance.border_width >> 16u;
    let is_preview = instance.border_width == PREVIEW_BORDER_SENTINEL;
    let depth = select(
        f32(track + 1u) / 65536.0,
        0.0,
        is_preview || track == 0u,
    );

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    output.color = unpack_key_color(instance.key_color);
    output.uv = local_offset;
    output.screen_size = screen_size;
    output.border_width = instance.border_width;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (input.border_width == PREVIEW_BORDER_SENTINEL) {
        return vec4<f32>(input.color.rgb, input.color.a * PREVIEW_ALPHA);
    }

    var color = input.color.rgb;

    let half_width_pixels = input.screen_size.x * 0.5;
    let half_height_pixels = input.screen_size.y * 0.5;

    var is_border = false;
    if (half_width_pixels > 0.0 && half_height_pixels > 0.0) {
        let border_px = f32(input.border_width & 0xFFFFu);
        let horiz_margin = 1.0 / half_width_pixels * border_px;
        let vert_margin = 1.0 / half_height_pixels * border_px;
        is_border = input.uv.x < horiz_margin
                 || input.uv.x > 1.0 - horiz_margin
                 || input.uv.y < vert_margin
                 || input.uv.y > 1.0 - vert_margin;
    }

    if (is_border) {
        color = color * BORDER_DARKEN_FACTOR;
    }

    return vec4<f32>(color, input.color.a);
}
