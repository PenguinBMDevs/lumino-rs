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
//! 单调游标（累计扫描的根治）：桶按 (key,start) 排，下界钉死桶底时每帧重扫
//! 从 tick=0 起的全部死音符（19M 文档后期每帧 ~1200 万次无效谓词）。
//! 死亡判定 `end <= tick_start` 关于 tick 单调，且导出 tick 严格递增，故每键
//! 记游标 = 已确认死亡的分界：本帧从 `max(cursor,b0)` 起扫，扫完把新死的
//! 推进去——每音符整个导出只被死亡检查一次，累计项摊销为 O(N/帧数)/帧。
//! 正确性：游标前全死（推进条件即死亡谓词的否定），FILL 复用 COUNT 推进后
//! 的游标，两阶段差集全是死音符、计数贡献为 0，输出与从桶底扫逐位一致。
//! 安全护栏（CPU 侧）：桶重建（排序变化）清零游标；tick 倒退清零重扫；
//! 同 tick 重复天然安全（单调非严格成立）。
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
    // 1 = 顺带聚合活跃键（miditrail 导出用；COUNT 扫描已覆盖全部 active 音符，
    // 零额外内存流量；waterfall 传 0 跳过）。tick 统一取 tick_start。
    compute_active: u32,
    ticks_per_second: f32,
    fps: f32,
    // 1 = FILL 按"画家序"写（miditrail：键内 [未来 start 降序、同 start 保持原序]
    // ++ [已开始 start 升序]，配合 CPU `prefix_counts_layered` 的白键块→黑键块
    // 基址，复刻 legacy `(is_black, z_start, key)` 稳定排序；waterfall 传 0 = 原升序）。
    paint_order: u32,
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
@group(0) @binding(7) var<storage, read_write> cursor: array<u32>; // 256 项单调游标
// 256 项：`[0,128)` 活跃键色（0 = 未按下，格式 `(key_color & 0xFFFFFF00) | 0xFF`），
// `[128,256)` 每键光晕系数（f32 bitcast；`compute_active=0` 时不写）。
@group(0) @binding(8) var<storage, read_write> active_out: array<u32>;

fn note_start(n: CullNote) -> u32 {
    return u32(max(n.start_length.x, 0.0));
}

// 打包语义 end（与 waterfall.wgsl `note_end` 同式；零长边界见头注）。
fn note_end(n: CullNote) -> u32 {
    return note_start(n) + u32(max(n.start_length.y, 1.0));
}

// 光晕系数：与 CPU `instances.rs::aura_factor_raw` 逐 op 对齐
//（flash 用 x*x 代替 powi(2)；tail 的 powf(0.3) 由 WGSL pow 近似，ULP 级差异）。
fn aura_factor(tick: u32, start: u32, end: u32, ticks_per_second: f32, fps: f32) -> f32 {
    let tps = max(ticks_per_second, 0.1);
    let frame_ticks = max(tps / max(fps, 1.0), 0.001);
    let frames_since_start = f32(tick - start) / frame_ticks;
    let flash_edge = max(10.0 - frames_since_start, 0.0);
    let flash = flash_edge * flash_edge / 600.0;

    let aura_len = tps * 1.0;
    let length = f32(max(end - start, 1u));
    let remaining = f32(end - tick);
    let offset = min(remaining, aura_len);
    let len = min(length, aura_len);
    var tail = 0.0;
    if len > 0.0 {
        tail = pow(offset / len, 0.3) * 0.5;
    }
    return tail + flash;
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
        // COUNT：从游标（已确认死亡分界）起扫；扫完推进游标。
        // 推进循环只经过"新死"音符——每位置整个桶生命周期恰推进一次。
        // 活跃键/光晕在扫描内顺带聚合：active 音符必落扫描区间
        //（end > tick_start 且 start < tick_end），无额外内存流量。
        var c = max(cursor[key], b0);
        var count = 0u;
        var color = 0u;
        var aura = 0.0;
        let compute_active = params.compute_active == 1u;
        for (var i = c; i < hi; i++) {
            let n = notes[sort_index[i]];
            let s = note_start(n);
            let e = note_end(n);
            if e > tick_start {
                count += 1u;
            }
            if compute_active && s <= tick_start && tick_start < e {
                color = (n.key_color & 0xFFFFFF00u) | 0xFFu;
                aura = max(aura, aura_factor(
                    tick_start,
                    s,
                    e,
                    params.ticks_per_second,
                    params.fps,
                ));
            }
        }
        counts[key] = count;
        while c < hi && note_end(notes[sort_index[c]]) <= tick_start {
            c += 1u;
        }
        cursor[key] = c;
        if compute_active {
            active_out[key] = color;
            active_out[128u + key] = bitcast<u32>(aura);
        }
    } else {
        // FILL：复用 COUNT 已推进的游标（同 tick，差集全死，输出一致）。
        let start = max(cursor[key], b0);
        let dst = base[key];
        var j = 0u;
        if params.paint_order == 1u {
            // 画家序（miditrail）：键内 [未来 start 降序、同 start 保持原序]
            // ++ [已开始 start 升序]。切分点 = 首个 start > tick_start（桶内
            // start 非递减，二分）。未来段全部 end > tick_start（end ≥ start+1），
            // 无需再过滤；已开始段按同一谓词过滤（与 COUNT 计数吻合）。
            var split = start;
            {
                var lo = start;
                var upper = hi;
                while lo < upper {
                    let mid = (lo + upper) / 2u;
                    if note_start(notes[sort_index[mid]]) <= tick_start {
                        lo = mid + 1u;
                    } else {
                        upper = mid;
                    }
                }
                split = lo;
            }
            // 未来段：按 start 降序输出；同 start 连续 run 内保持升序（稳定序）。
            var i = hi;
            while (i > split) {
                let s = note_start(notes[sort_index[i - 1u]]);
                var r0 = i - 1u;
                while (r0 > split && note_start(notes[sort_index[r0 - 1u]]) == s) {
                    r0 -= 1u;
                }
                for (var k = r0; k < i; k += 1u) {
                    compact[dst + j] = notes[sort_index[k]];
                    j += 1u;
                }
                i = r0;
            }
            // 已开始段：升序（死音符跳过，计数与 COUNT 一致）。
            for (var k = start; k < split; k += 1u) {
                let src = sort_index[k];
                if note_end(notes[src]) > tick_start {
                    compact[dst + j] = notes[src];
                    j += 1u;
                }
            }
        } else {
            for (var i = start; i < hi; i++) {
                let src = sort_index[i];
                if note_end(notes[src]) > tick_start {
                    compact[dst + j] = notes[src];
                    j += 1u;
                }
            }
        }
    }
}
