// 纵向卷帘音符 GPU 裁剪着色器 — cull.wgsl 转置版
// 复用 NoteInstance 布局与 workgroup 批量原子，仅裁剪逻辑转置：
//   横向：screen_x = tick*zoom_x + kb, screen_y = (max_key - key)*zoom_y + ruler, size=(len*zoom_x, zoom_y)
//   纵向：screen_x = key*zoom_y               , screen_y = tick*zoom_x + ruler,        size=(zoom_y, len*zoom_x)
// 键盘在底部，故 X 不叠加 keyboard_width；Y 仍叠加 ruler_height。
// 样式/深度/可见性阈值与横向完全一致，复用 GPU 音符数据仅改变绘制方式（瀑布流风格的纵向流动感）

struct NoteInstance {
    start_length: vec2<f32>,
    key_color: u32,
    border_width: u32,
};

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
    _padding: u32,
};

struct DrawIndirectArgs {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
    _padding: vec4<u32>,
};

/// 视图状态：与 `onion_note.wgsl` 逐字一致（当前音轨 + 洋葱皮开关 + 静音位图）
struct ViewState {
    current_track: u32,
    /// 洋葱皮（非主音轨实例）是否可见：0 = 隐藏（工具栏开关默认关闭）
    onion_visible: u32,
    _pad1: u32,
    _pad2: u32,
    muted_bits: array<vec4<u32>, 512>,
};

/// 预览音符哨兵（`border_width`）：预览层（`note.wgsl`）不走轨道语义
const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> cull_info: CullUniform;
@group(0) @binding(2) var<storage, read> all_instances: array<NoteInstance>;
@group(0) @binding(3) var<storage, read_write> visible_instances: array<u32>;
@group(0) @binding(4) var<storage, read_write> indirect_args: DrawIndirectArgs;
@group(0) @binding(5) var<uniform> view_state: ViewState;

/// 查询静音位图（track_enc = track_idx+1；0 恒为未静音）
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

/// 轨道闸门（与横向 `cull.wgsl` 逐字同义，改动必须两处同步）。
fn track_visible(border_width: u32) -> bool {
    if (border_width == PREVIEW_BORDER_SENTINEL) {
        return true;
    }
    if (view_state.current_track == 0u) {
        return true;
    }
    let track_enc = border_width >> 16u;
    if (track_enc == view_state.current_track) {
        return true;
    }
    if (view_state.onion_visible == 0u) {
        return false;
    }
    return !is_muted_track(track_enc);
}

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
    let local_index = global_id.x + global_id.y * MAX_X_THREADS;
    let index = cull_info.chunk_start + local_index;
    let in_range = local_index < cull_info.chunk_count
                && index < cull_info.instance_count;

    var is_visible = false;
    if (in_range) {
        let instance = all_instances[local_index];
        let tick = instance.start_length.x;
        let length = instance.start_length.y;
        let key = f32(instance.key_color & 0xFFu);

        if (length > 0.0 && track_visible(instance.border_width)) {
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
