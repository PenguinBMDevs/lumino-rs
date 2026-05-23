// 洋葱皮可见性剔除 Compute Shader
// 每个线程处理一个音符，通过原子计数器输出可见实例索引
// 工作组大小: 256
//
// 音符池需按 start_tick 升序排列
// CPU 端二分查找定位 visible_start/visible_end，GPU 仅扫描该区间

struct OnionNote {
    start_tick: u32,
    end_tick: u32,
    packed: u32,
    _padding: u32,
};

struct OnionViewportUniform {
    tick_start: f32,
    tick_end: f32,
    pitch_min: f32,
    pitch_max: f32,
    note_count: u32,
    indices_capacity: u32,
    visible_start: u32,
    visible_end: u32,
};

struct OnionTrackMask {
    mask_lo: u32,
    mask_hi: u32,
};

struct DrawIndirectArgs {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
};

@group(0) @binding(0) var<uniform> viewport: OnionViewportUniform;
@group(0) @binding(1) var<uniform> track_mask: OnionTrackMask;
@group(0) @binding(2) var<storage, read> note_pool: array<OnionNote>;
@group(0) @binding(3) var<storage, read_write> instance_indices: array<u32>;
@group(0) @binding(4) var<storage, read_write> indirect_args: DrawIndirectArgs;

var<workgroup> wg_count: atomic<u32>;
var<workgroup> wg_indices: array<u32, 256>;
var<workgroup> wg_global_base: u32;
var<workgroup> wg_total: u32;

fn unpack_pitch(packed: u32) -> u32 {
    return packed & 0xFFu;
}

fn unpack_track_idx(packed: u32) -> u32 {
    return (packed >> 8u) & 0xFFFFu;
}

fn is_track_visible(track_idx: u32, mask: OnionTrackMask) -> bool {
    var bit: u32;
    var word: u32;
    if (track_idx < 32u) {
        word = mask.mask_lo;
        bit = track_idx;
    } else {
        word = mask.mask_hi;
        bit = track_idx - 32u;
    }
    return ((word >> bit) & 1u) != 0u;
}

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    // visible_start 偏移：CPU 二分查找定位的起始索引
    // 仅扫描 [visible_start, visible_end) 区间，跳过区间外的音符
    let index = viewport.visible_start + global_id.x;

    if (global_id.x == 0u) {
        atomicStore(&indirect_args.instance_count, 0u);
        indirect_args.vertex_count = 4u;
    }

    let in_range = index < viewport.visible_end;

    var is_visible = false;
    if (in_range) {
        let note = note_pool[index];
        let pitch = unpack_pitch(note.packed);
        let track = unpack_track_idx(note.packed);

        let tick_start_f = f32(note.start_tick);
        let tick_end_f = f32(note.end_tick);
        let in_tick_range = tick_end_f > viewport.tick_start && tick_start_f < viewport.tick_end;

        let pitch_f = f32(pitch);
        let in_pitch_range = pitch_f >= viewport.pitch_min && pitch_f <= viewport.pitch_max;

        let track_visible = is_track_visible(track, track_mask);

        if (in_tick_range && in_pitch_range && track_visible) {
            is_visible = true;
        }
    }

    if (is_visible) {
        let slot = atomicAdd(&wg_count, 1u);
        wg_indices[slot] = index;
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
        if (dst < viewport.indices_capacity) {
            instance_indices[dst] = src_idx;
        }
    }
}
