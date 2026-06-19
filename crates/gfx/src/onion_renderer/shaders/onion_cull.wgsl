// 洋葱皮可见性剔除 Compute Shader
// 工作组大小: 256
//
// 两种工作模式（由 viewport.use_key_ranges 控制）：
// 1. 兼容模式（use_key_ranges == 0）：
//    音符池为扁平数组，GPU 在 [visible_start, visible_end) 区间内全量裁剪。
// 2. Bucket 模式（use_key_ranges != 0）：
//    音符池按 key 分桶（key_offsets 给出累积偏移），每个 workgroup 处理一个 key，
//    只扫描 key_ranges[key] 指定的 [start, end) tick 范围。

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
    indices_capacity: u32,
    current_track: u32,
    use_key_ranges: u32,
    visible_start: u32,
    visible_end: u32,
};

struct OnionKeyRange {
    start: u32,
    end: u32,
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
@group(0) @binding(4) var<storage, read> key_offsets: array<u32>;
@group(0) @binding(5) var<storage, read> key_ranges: array<OnionKeyRange>;

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

// 处理一个音符，若可见则通过 workgroup-local atomic 写入 wg_indices
fn process_note(index: u32) {
    let note = note_pool[index];
    let pitch = unpack_pitch(note.packed);
    let track_idx = unpack_track_idx(note.packed);

    // 排除当前编辑音轨
    if (track_idx == viewport.current_track) {
        return;
    }

    let tick_start_f = f32(note.start_tick);
    let tick_end_f = f32(note.end_tick);
    let in_tick_range = tick_end_f > viewport.tick_start && tick_start_f < viewport.tick_end;

    let pitch_f = f32(pitch);
    let in_pitch_range = pitch_f >= viewport.pitch_min && pitch_f <= viewport.pitch_max;

    if (in_tick_range && in_pitch_range) {
        let slot = atomicAdd(&wg_count, 1u);
        wg_indices[slot] = index;
    }
}

@compute @workgroup_size(256)
fn main(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(global_invocation_id) global_id: vec3<u32>,
) {
    // 线程 0 负责清零 indirect args
    if (global_id.x == 0u) {
        atomicStore(&indirect_args.instance_count, 0u);
        indirect_args.vertex_count = 4u;
    }

    if (viewport.use_key_ranges != 0u) {
        // ── Bucket 模式：一个 workgroup 处理一个 key ──
        let key = workgroup_id.x;
        // key 必须在 [0, 256) 内；dispatch 为 256，所以不会越界
        let pitch_f = f32(key);
        let key_active = pitch_f >= viewport.pitch_min && pitch_f <= viewport.pitch_max;

        if (key_active) {
            let range = key_ranges[key];
            let base = key_offsets[key];
            let count = range.end - range.start;

            // workgroup 内线程循环扫描该 key 的可见 tick 范围
            var i = local_id.x;
            while (i < count) {
                process_note(base + range.start + i);
                i += 256u;
            }
        }
    } else {
        // ── 兼容模式：扁平扫描 [visible_start, visible_end) ──
        let index = viewport.visible_start + global_id.x;
        let in_range = index < viewport.visible_end;
        if (in_range) {
            process_note(index);
        }
    }

    workgroupBarrier();

    // 线程 0 做 1 次全局 atomicAdd，代表整个 workgroup
    if (local_id.x == 0u) {
        let n = atomicLoad(&wg_count);
        wg_total = n;
        wg_global_base = atomicAdd(&indirect_args.instance_count, n);
    }
    workgroupBarrier();

    // 写入可见索引
    if (local_id.x < wg_total) {
        let src_idx = wg_indices[local_id.x];
        let dst = wg_global_base + local_id.x;
        if (dst < viewport.indices_capacity) {
            instance_indices[dst] = src_idx;
        }
    }
}
