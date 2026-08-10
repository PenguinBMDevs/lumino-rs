// 音符渲染着色器 — 16 bytes NoteInstance（严格对齐 wasabi NoteVertex）
// 字段布局：start_length[2] + key_color + border_width
//
// 渲染风格（用户要求）：
//   - 纯色填充（无 cos 渐变、无 SRGB gamma 平方）
//   - 2 像素同色加深描边（border_width 字段传 2，加深系数 0.4）
//   - 预览音符：border_width 哨兵值检测，70% alpha
//
// 深度语义（修复重叠音符随机闪烁，2026-08）：
//   cull.wgsl 并行重打包可见实例，重叠实例的绘制顺序每帧随机；
//   因此用 border_width 高 16 位编码轨道索引，VS 输出稳定深度：
//   - 主音轨（track==0）与预览音符 → z = 0.0（最近，永远覆盖洋葱皮）
//   - 洋葱皮轨道 i → z = (i+1) / 65536.0（索引越大越靠后）
//   深度测试 LessEqual 与绘制顺序无关，重叠处深度小者稳定胜出，不闪烁。
//
// VS 用 instancing + 4 顶点 quad 复刻 wasabi 的 GS「点扩展为 quad」逻辑

const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;
const PREVIEW_ALPHA: f32 = 0.7;

/// 边框颜色加深因子（同色系深色：color * 0.4，比 wasabi 0.2 略亮，视觉更协调）
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

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,           // [0,1]² UV，用于边距判定
    @location(2) screen_size: vec2<f32>,  // 屏幕像素宽高（用于 UV 边距反算）
    @location(3) border_width: u32,       // 透传到 FS
};

// 实例数据（16 bytes，与 wasabi NoteVertex 字段对齐）
struct NoteInstance {
    @location(0) start_length: vec2<f32>,  // [start_tick, length_tick]
    @location(1) key_color: u32,           // 低8位=key, 高24位=RGB
    @location(2) border_width: u32,        // 边框像素宽（PREVIEW_BORDER_SENTINEL=预览）
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
    // local_offset 同时作为 UV 使用
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

    // 稳定深度：主音轨（高 16 位=0）与预览哨兵 → 0.0（最近）；
    // 洋葱皮轨道 i（高 16 位=i+1）→ (i+1)/65536.0（索引越大越靠后）
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
    // 预览音符：border_width 哨兵值检测，70% alpha，不画边框
    if (input.border_width == PREVIEW_BORDER_SENTINEL) {
        return vec4<f32>(input.color.rgb, input.color.a * PREVIEW_ALPHA);
    }

    // 纯色填充（用户要求：去掉 cos 渐变和 gamma 平方）
    var color = input.color.rgb;

    // 2 像素同色加深描边（UV 边距判定算法）
    let half_width_pixels = input.screen_size.x * 0.5;
    let half_height_pixels = input.screen_size.y * 0.5;

    var is_border = false;
    if (half_width_pixels > 0.0 && half_height_pixels > 0.0) {
        // 边框宽 = 低 16 位（高 16 位被深度编码占用）
        let border_px = f32(input.border_width & 0xFFFFu);
        let horiz_margin = 1.0 / half_width_pixels * border_px;
        let vert_margin = 1.0 / half_height_pixels * border_px;
        is_border = input.uv.x < horiz_margin
                 || input.uv.x > 1.0 - horiz_margin
                 || input.uv.y < vert_margin
                 || input.uv.y > 1.0 - vert_margin;
    }

    if (is_border) {
        // 边框色 = 原色 × 0.4（同色系加深，比 wasabi 0.2 略亮）
        color = color * BORDER_DARKEN_FACTOR;
    }

    return vec4<f32>(color, input.color.a);
}
