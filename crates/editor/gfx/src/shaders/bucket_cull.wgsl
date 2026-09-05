// bucket_cull.wgsl — 全局桶窗口提取（两阶段 key 分区 cull）
//!
//! 背景：`waterfall_indexed.wgsl` 的逐像素桶内回溯（SEARCH_BUFFER=128）是按
//! 窗口过滤后桶密度标定的；全量历史入桶后，已结束的死音符同样消耗回溯预算，
//! 密集段长音会被漏检（legacy 窗口把死音符排除在外）。因此导出改走
//! “cull 提取窗口 → legacy 精确渲染”：本 shader 只做窗口提取，渲染仍用标定
//! 过的 legacy shader，回溯预算语义与 UI 窗口完全一致。
//!
//! 两阶段（输出 key 主序、start 次序，与 `sort_visible_notes` 同序）：
//! - COUNT：每 key 一线程，桶内二分上界 + 线性过滤，写 `counts[key]`；
//! - FILL：CPU 前缀和得每 key 基址后，同构重扫，写 `compact[base+j]`。
//! 两阶段划分保证输出 key 连续（legacy 桶内二分前提），无原子竞争。
//!
//! 窗口谓词与 UI `collect_window_notes` 逐 op 一致：
//! `end_tick > tick_start && start_tick < tick_end && key < key_count`。
//! 注意 `end` 按打包语义 `start + max(len, 1.0)`（与 legacy shader 的
//! `note_end` 同式）；UI 窗口按原始 `end_tick`（零长音符在边界差 1px，见
//! cull.rs 文档；harness 用非零长数据，生产与现状逐位一致）。
//!
//! dispatch: (ceil(key_count/64), 1, 1), workgroup (64, 1, 1)。

struct CullParams {
    tick_start: u32,
    tick_end: u32,
    key_count: u32,
    phase: u32, // 0 = COUNT，1 = FILL
    total_count: u32, // 常驻总数（越界保护）
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

struct CullNote {
    start_length: vec2<f32>, // [start_tick, length_tick]（与 NoteInstance 同布局）
    key_color: u32, // 低 8 位 = key
    border_width: u32,
}

@group(0) @binding(0) var<uniform> params: CullParams;
@group(0) @binding(1) var<storage, read> notes: array<CullNote>;
@group(0) @binding(2) var<storage, read> key_offsets: array<u32>; // 全局 257 项
@group(0) @binding(3) var<storage, read> sort_index: array<u32>;
@group(0) @binding(4) var<storage, read_write> compact: array<CullNote>;
@group(0) @binding(5) var<storage, read_write> counts: array<u32>; // 256 项
@group(0) @binding(6) var<storage, read> base: array<u32>; // 256 项（FILL 用）

fn note_start(n: CullNote) -> u32 {
    return u32(max(n.start_length.x, 0.0));
}

// 打包语义 end（与 waterfall.wgsl `note_end` 同式；零长边界见头注）。
fn note_end(n: CullNote) -> u32 {
    return note_start(n) + u32(max(n.start_length.y, 1.0));
}

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let key = global_id.x;
    if key >= params.key_count || key >= 256u {
        return;
    }
    let tick_start = params.tick_start;
    let tick_end = params.tick_end;
    var b0 = key_offsets[key];
    var b1 = key_offsets[key + 1u];
    // 越界保护：桶构建计数与常驻一致时恒成立，防御句柄复用错位。
    b0 = min(b0, params.total_count);
    b1 = min(b1, params.total_count);
    // 上界二分：首个 start >= tick_end 的位置（start < tick_end 方为候选）。
    // COUNT 与 FILL 同构（谓词一致是两阶段计数吻合的前提）。
    var lo = b0;
    var hi = b1;
    while lo < hi {
        let mid = (lo + hi) / 2u;
        if note_start(notes[sort_index[mid]]) < tick_end {
            lo = mid + 1u;
        } else {
            hi = mid;
        }
    }
    if params.phase == 0u {
        var c = 0u;
        for (var i = b0; i < hi; i++) {
            if note_end(notes[sort_index[i]]) > tick_start {
                c += 1u;
            }
        }
        counts[key] = c;
    } else {
        let dst = base[key];
        var j = 0u;
        for (var i = b0; i < hi; i++) {
            let src = sort_index[i];
            if note_end(notes[src]) > tick_start {
                compact[dst + j] = notes[src];
                j += 1u;
            }
        }
    }
}
