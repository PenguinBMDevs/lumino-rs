// 走带视图音符着色器 —— 直接复用钢琴卷帘常驻 GPU 音符缓冲（NoteInstance 16 字节布局）
//
// 设计要点（字节味：超级简化 + 零第二份显存）：
//   1. 顶点缓冲直接 bind 钢琴卷帘 onion_skin 的 GpuNoteBuffer.instance_buffer；
//   2. border_width 高 16 位编码文档音轨索引 track = (border_width >> 16) - 1；
//   3. lane_index 存储缓冲把文档音轨映射到侧栏泳道序号（与排序无关）；
//   4. 横向/纵向可视裁剪完全交给 GPU：NDC 裁剪掉屏外，scissor 限定泳道范围；
//      与钢琴卷帘「只画可见的」在视觉上等价，但不再做 CPU 重建 / 第二份缓冲。

struct Uniforms {
    scroll: vec2<f32>,          // x=时间滚动(tick), y=纵向滚动(px)
    zoom: vec2<f32>,            // x=px/tick, y=未用(保留对齐)
    viewport_size: vec2<f32>,   // 画布像素尺寸
    canvas_offset: vec2<f32>,   // 画布内偏移(px)
    lane_height: f32,           // 单个泳道高度(px) = track_height * zoom_y
    note_height: f32,           // 音符条高度(px，固定细条)
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
// lane_index[track] = 该文档音轨在走带侧栏中的泳道序号（浮点，便于着色器直接索引）
@group(0) @binding(1) var<storage, read> lane_index: array<f32>;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) screen_size: vec2<f32>,
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
    @builtin(vertex_index) vi: u32,
    @location(0) start_length: vec2<f32>,  // [start_tick, length_tick]
    @location(1) key_color: u32,           // 低8位=key, 高24位=RGB
    @location(2) border_width: u32,        // 高16位=track+1（洋葱皮编码）
) -> VSOut {
    let tick = start_length.x;
    let length = start_length.y;
    let key = f32(key_color & 0xFFu);
    let track = (border_width >> 16u) - 1u;

    // 泳道顶部：lane 序号 * 泳道高 - 纵向滚动 + 画布偏移
    let lane_top = lane_index[track] * u.lane_height - u.scroll.y + u.canvas_offset.y;
    // 单键在泳道内的高度（128 键铺满泳道）
    let key_h = u.lane_height / 128.0;
    // 音符条纵向中心：按 pitch 在泳道内定位
    let note_y = lane_top + (127.0 - key) * key_h + key_h * 0.5;

    var corners = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let c = corners[vi];

    let sx = u.canvas_offset.x + tick * u.zoom.x - u.scroll.x;
    let sw = length * u.zoom.x;
    // 固定细条：以 pitch 中心向上下各展 note_height/2，下限 1px 保证可见
    let half_h = max(u.note_height * 0.5, 0.5);
    let sy = note_y - half_h;
    let sh = half_h * 2.0;

    let px = sx + c.x * sw;
    let py = sy + c.y * sh;

    let ndc_x = (px / u.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / u.viewport_size.y) * 2.0;

    var out: VSOut;
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = unpack_key_color(key_color);
    out.uv = c;
    out.screen_size = vec2<f32>(sw, sh);
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    var color = in.color.rgb;
    // 2px 同色描边，与钢琴卷帘视觉一致
    let half_w = in.screen_size.x * 0.5;
    let half_h = in.screen_size.y * 0.5;
    var is_border = false;
    if (half_w > 0.0 && half_h > 0.0) {
        let border_px = 2.0;
        let hm = 1.0 / half_w * border_px;
        let vm = 1.0 / half_h * border_px;
        is_border = in.uv.x < hm || in.uv.x > 1.0 - hm || in.uv.y < vm || in.uv.y > 1.0 - vm;
    }
    if (is_border) {
        color = color * 0.4;
    }
    return vec4<f32>(color, in.color.a);
}
