// 统一全量音符渲染着色器（洋葱皮 + 主音轨一体，2026-08-06）
//
// 数据模型：GPU buffer 持有**所有轨全部音符**（统一 `border_width` 高 16 位
// 编码 track_idx+1），「哪个轨是主音轨」由 `ViewState.current_track` uniform
// 低频更新（切轨零重传）。shader 内：
//   - 主音轨（track == current_track）：染主音轨蓝、深度 0（最前，覆盖一切）
//   - 静音轨且非主音轨：NDC z=2.0 裁剪（不渲染）
//   - 其余（洋葱皮）：实例固化的调色板色、深度 (track_enc+1)/65536
//
// 深度语义（修复重叠音符随机闪烁，2026-08）：cull.wgsl 并行重打包可见实例，
// 重叠实例绘制顺序每帧随机；深度测试 LessEqual 与绘制顺序无关，深度小者
// 稳定胜出。主音轨 z=0.0 永远覆盖洋葱皮。
//
// 与旧 note.wgsl 的差异：无预览哨兵分支（预览音符走独立渲染器 note.wgsl）。

/// 主音轨固定蓝色（与 UI 层 `MAIN_TRACK_NOTE_COLOR` 一致）
const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);

/// 边框颜色加深因子（同色系深色：color * 0.4，与主音轨 note.wgsl 保持一致）
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

/// 视图状态：当前音轨（track_idx+1）+ 静音位图（512 × vec4 = 65536 轨）
struct ViewState {
    current_track: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    muted_bits: array<vec4<u32>, 512>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<uniform> view_state: ViewState;

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
    @location(2) border_width: u32,        // 低16位=边框像素宽, 高16位=track_idx+1
};

/// 解包 key_color → vec4 RGBA（alpha 恒为 1.0）
fn unpack_key_color(packed: u32) -> vec4<f32> {
    let rgb = packed >> 8u;
    let r = f32((rgb >> 16u) & 0xFFu) / 255.0;
    let g = f32((rgb >> 8u) & 0xFFu) / 255.0;
    let b = f32(rgb & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}

/// 查询静音位图（track_enc = track_idx+1；0 恒为未静音）
/// 布局与 CPU `ViewState` 一致：128 个连续 u32，bit i = 音轨 i 静音。
fn is_muted_track(track_enc: u32) -> bool {
    if (track_enc == 0u) {
        return false;
    }
    let track_idx = track_enc - 1u;
    let word = track_idx / 32u;
    let bit = track_idx % 32u;
    if (word >= 2048u) {
        return false;
    }
    let v = view_state.muted_bits[word / 4u];
    return ((v[word % 4u] >> bit) & 1u) != 0u;
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

    // 主音轨判定：track 编码 == 当前音轨编码（track_idx+1）
    let track_enc = instance.border_width >> 16u;
    let is_main = track_enc == view_state.current_track;
    // 静音且非主音轨 → 不渲染（NDC z=2.0 超出深度范围被裁剪）
    let is_muted = is_muted_track(track_enc);
    let show = is_main || !is_muted;

    // 稳定深度：主音轨 → 0.0（最近）；洋葱皮轨道 → (track_enc+1)/65536（越大越靠后）
    var depth = f32(track_enc + 1u) / 65536.0;
    if (is_main) {
        depth = 0.0;
    }

    // 颜色：主音轨强制主轨蓝（数据无需重传）；其余用实例固化调色板色
    var color = unpack_key_color(instance.key_color);
    if (is_main) {
        color = vec4<f32>(MAIN_TRACK_COLOR, 1.0);
    }

    var output: VertexOutput;
    if (show) {
        output.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    } else {
        output.position = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    output.color = color;
    output.uv = local_offset;
    output.screen_size = screen_size;
    output.border_width = instance.border_width;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // 纯色填充（不透明，洋葱皮 alpha=1.0 与主音轨一致）
    var color = input.color.rgb;

    // 2 像素同色加深描边（与 note.wgsl 共用 UV 边距判定算法）
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
        // 边框色 = 原色 × 0.4（同色系加深，与主音轨一致）
        color = color * BORDER_DARKEN_FACTOR;
    }

    return vec4<f32>(color, 1.0);
}
