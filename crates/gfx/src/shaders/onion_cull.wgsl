// 洋葱皮可见性剔除 Compute Shader
// 扁平剔除：每个线程处理一个音符，直写全局缓冲区
// 移除了旧版的 per-key bucket 模式，只保留全量剔除

struct OnionNote {
    start_tick: u32,
    end_tick: u32,
    packed: u32,
    color_packed: u32,
};

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

struct DrawIndirectArgs {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
};

@group(0) @binding(0) var<uniform> viewport: OnionViewportUniform;
@group(0) @binding(1) var<storage, read> note_pool: array<OnionNote>;
@group(0) @binding(2) var<storage, read_write> instance_indices: array<u32>;
@group(0) @binding(3) var<storage, read_write> indirect_args: DrawIndirectArgs;

fn unpack_pitch(packed: u32) -> u32 {
    return packed & 0xFFu;
}

fn unpack_track_idx(packed: u32) -> u32 {
    return (packed >> 8u) & 0xFFFFu;
}

// dispatch_x = min(note_count / 256, 65535), dispatch_y = ceil(note_count / 256 / 65535)
// 用 2D dispatch 规避 WGSL 单维 ≤ 65535 限制
const DISPATCH_X_MAX: u32 = 65535u;
const WG_SIZE: u32 = 256u;

@compute @workgroup_size(WG_SIZE, 1, 1)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
) {
    let index = global_id.y * DISPATCH_X_MAX * WG_SIZE + global_id.x;
    if (index >= viewport.note_count) {
        return;
    }

    let note = note_pool[index];
    let pitch = unpack_pitch(note.packed);
    let track_idx = unpack_track_idx(note.packed);

    // 排除当前编辑音轨
    if (track_idx == viewport.current_track) {
        return;
    }

    // 视口剔除：音符与视口有重叠 → 保留
    let in_pitch = f32(pitch) >= viewport.pitch_min && f32(pitch) <= viewport.pitch_max;
    let in_tick = f32(note.end_tick) > viewport.tick_start && f32(note.start_tick) < viewport.tick_end;

    if (in_pitch && in_tick) {
        let slot = atomicAdd(&indirect_args.instance_count, 1u);
        if (slot < 33554432u) {
            instance_indices[slot] = index;
        }
    }
}
