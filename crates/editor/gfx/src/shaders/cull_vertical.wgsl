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
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    _padding: f32,
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

        if (length > 0.0) {
            // 纵向：X = key*zoom_y - scroll_y, 宽度 = zoom_y
            let screen_min_x = key * camera.zoom.y - camera.scroll.y + camera.canvas_offset.x;
            if (screen_min_x <= camera.viewport_size.x) {
                let screen_max_x = screen_min_x + camera.zoom.y;
                if (screen_max_x >= 0.0) {
                    // 纵向：Y = tick*zoom_x - scroll_x + ruler, 高度 = len*zoom_x
                    let screen_min_y = tick * camera.zoom.x - camera.scroll.x + camera.ruler_height + camera.canvas_offset.y;
                    let screen_max_y = screen_min_y + length * camera.zoom.x;
                    if (screen_max_y >= 0.0 && screen_min_y <= camera.viewport_size.y) {
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
