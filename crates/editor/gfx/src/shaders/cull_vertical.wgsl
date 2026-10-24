// 纵向卷帘音符 GPU 裁剪着色器 — cull.wgsl 转置版
// 复用 NoteInstance 布局与 workgroup 批量原子，仅裁剪逻辑转置：
//   横向：screen_x = tick*zoom_x + kb, screen_y = (max_key - key)*zoom_y + ruler, size=(len*zoom_x, zoom_y)
//   纵向：screen_x = key*zoom_y               , screen_y = tick*zoom_x + ruler,        size=(zoom_y, len*zoom_x)
// 键盘在底部，故 X 不叠加 keyboard_width；Y 仍叠加 ruler_height。
// 样式/深度/可见性阈值与横向完全一致，复用 GPU 音符数据仅改变绘制方式（瀑布流风格的纵向流动感）

// ── 12B note layout（16B wasabi 布局已删除）────────────────────────────
struct NoteInstance {
    start: f32,
    length: f32,
    attrs: u32, // key(0..7) | track_enc(8..23) | border(24..30) | preview(31)
};

fn note_key(attrs: u32) -> u32 { return attrs & 0xFFu; }
fn note_track(attrs: u32) -> u32 { return (attrs >> 8u) & 0xFFFFu; }
fn note_border(attrs: u32) -> u32 { return (attrs >> 24u) & 0x7Fu; }
fn note_is_preview(attrs: u32) -> bool { return (attrs & 0x80000000u) != 0u; }

// 与 lumino_extras fallback 调色板同色；GPU 调色板缓冲接入前的兜底。
const FALLBACK_TRACK_COLORS: array<vec4<f32>, 12> = array<vec4<f32>, 12>(
    vec4<f32>(0.784, 0.314, 0.314, 1.0),
    vec4<f32>(0.314, 0.784, 0.471, 1.0),
    vec4<f32>(0.314, 0.471, 0.863, 1.0),
    vec4<f32>(0.863, 0.784, 0.314, 1.0),
    vec4<f32>(0.784, 0.392, 0.784, 1.0),
    vec4<f32>(0.314, 0.784, 0.784, 1.0),
    vec4<f32>(0.941, 0.588, 0.314, 1.0),
    vec4<f32>(0.706, 0.706, 0.706, 1.0),
    vec4<f32>(0.902, 0.392, 0.392, 1.0),
    vec4<f32>(0.392, 0.706, 0.392, 1.0),
    vec4<f32>(0.392, 0.392, 0.784, 1.0),
    vec4<f32>(0.784, 0.706, 0.392, 1.0),
);

fn note_color(attrs: u32) -> vec4<f32> {
    let idx = max(note_track(attrs), 1u) - 1u;
    return FALLBACK_TRACK_COLORS[idx % 12u];
}

struct CameraUniform {
    scroll: vec2<f32>,
    zoom: vec2<f32>,
    viewport_size: vec2<f32>,
    canvas_offset: vec2<f32>,
    canvas_size: vec2<f32>,
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: vec2<f32>,
};

struct CullUniform {
    instance_count: u32,
    chunk_start: u32,
    chunk_count: u32,
    // 本次分发在 chunk 局部的起始偏移（整段 cull = 0）
    local_start: u32,
};

struct DrawIndirectArgs {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
    _padding: vec4<u32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> cull_info: CullUniform;
@group(0) @binding(2) var<storage, read> all_instances: array<NoteInstance>;
@group(0) @binding(3) var<storage, read_write> visible_instances: array<u32>;
@group(0) @binding(4) var<storage, read_write> indirect_args: DrawIndirectArgs;

var<workgroup> wg_count: atomic<u32>;
var<workgroup> wg_indices: array<u32, 256>;
var<workgroup> wg_global_base: u32;
var<workgroup> wg_total: u32;

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    let MAX_X_THREADS: u32 = 65535u * 256u;
    let rel_index = global_id.x + global_id.y * MAX_X_THREADS;
    let local_index = cull_info.local_start + rel_index;
    let index = cull_info.chunk_start + local_index;
    let in_range = rel_index < cull_info.chunk_count
                && index < cull_info.instance_count;

    var is_visible = false;
    if (in_range) {
        let instance = all_instances[local_index];
        let tick = instance.start;
        let length = instance.length;
        let key = f32(note_key(instance.attrs));

        if (length > 0.0) {
            // 纵向头部对齐键盘顶部：X = key*zoom_y - scroll_y, Y = grid_bottom - (tick+len)*zoom + scroll
            let grid_bottom = camera.canvas_offset.y + camera.canvas_size.y - camera.keyboard_width;
            let screen_min_x = key * camera.zoom.y - camera.scroll.y + camera.canvas_offset.x;
            if (screen_min_x <= camera.viewport_size.x) {
                let screen_max_x = screen_min_x + camera.zoom.y;
                if (screen_max_x >= 0.0) {
                    let screen_top = grid_bottom - (tick + length) * camera.zoom.x + camera.scroll.x;
                    let screen_bottom = grid_bottom - tick * camera.zoom.x + camera.scroll.x;
                    if (screen_bottom >= 0.0 && screen_top <= camera.viewport_size.y) {
                        is_visible = true;
                    }
                }
            }
        }
    }

    if (is_visible) {
        let slot = atomicAdd(&wg_count, 1u);
        wg_indices[slot] = local_index;
    }
    workgroupBarrier();

    if (local_id.x == 0u) {
        let n = atomicLoad(&wg_count);
        wg_total = n;
        wg_global_base = atomicAdd(&indirect_args.instance_count, n);
    }
    workgroupBarrier();

    if (local_id.x < wg_total) {
        let src_idx = wg_indices[local_id.x];
        let dst = wg_global_base + local_id.x;
        if (dst < arrayLength(&visible_instances)) {
            visible_instances[dst] = src_idx;
        }
    }
}
