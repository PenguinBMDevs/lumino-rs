//! WGSL 着色器常量
//!
//! 包含 event_proc 和 render 两个 compute shader 的 WGSL 源码。
//! 作为常量嵌入 Rust 二进制，运行时直接传给 wgpu 创建 shader module。

// ── WGSL: event_proc ─────────────────────────────────
pub(crate) const EVENT_PROC_SRC: &str = r#"
const MV: u32 = 2048u;
const WGS: u32 = 256u;
const RELEASE_STEAL_AGE: f32 = 480.0;

struct RE { to: u32, data: u32, }
struct RG { kl: u32, kh: u32, vl_l: u32, vl_h: u32, bo: u32, bl: u32, ls: u32, le: u32, lm: u32, rk: u32, tn: i32, vol: f32, pan: i32, }
struct VP { pos: f32, pitch: f32, vol: f32, pan: f32, ss: u32, se: u32, ls: u32, le: u32, ena: u32, lp: u32, ch: u32, ky: u32, rel: u32, rf: u32, sf: u32, rel_elapsed: f32, }
struct U { ne: u32, nr: u32, ns: u32, sr: u32, mv: u32, ch: u32, }

@group(0) @binding(0) var<storage, read_write> params: array<VP>;
@group(0) @binding(1) var<storage, read> events: array<RE>;
@group(0) @binding(2) var<storage, read> rgns: array<RG>;
@group(0) @binding(3) var<uniform> u: U;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let ei = id.x;
    if ei >= u.ne { return; }
    let ev = events[ei];

    let ev_kind = ev.data & 0xFFu;
    let ev_ch = (ev.data >> 8u) & 0xFFu;
    let ev_key = (ev.data >> 16u) & 0xFFu;
    let ev_vel = (ev.data >> 24u) & 0xFFu;

    if ev_kind == 0u {
        // MIDI 规范：velocity 0 的 NoteOn = NoteOff
        if ev_vel == 0u {
            for (var i = 0u; i < u.mv; i++) {
                let p = params[i];
                if p.ena != 0u && p.ch == ev_ch && p.ky == ev_key && p.rel == 0u {
                    params[i].rel = 1u;
                    params[i].rf = ev.to;
                    break;
                }
            }
            return;
        }
        // velocity 1 是最小有效力度，不应被丢弃
        if ev_vel < 1u { return; }

        var ri = 0u;
        var found = false;
        for (var i = 0u; i < u.nr; i++) {
            let r = rgns[i];
            if ev_key >= r.kl && ev_key <= r.kh && ev_vel >= r.vl_l && ev_vel <= r.vl_h {
                ri = i; found = true; break;
            }
        }
        if !found { return; }
        let r = rgns[ri];

        // 多轮 voice 分配：
        // 1) 完全空闲 slot；
        // 2) release 已持续足够久（release tail 基本结束）；
        // 3) 任意 released voice（取 release 最久的）；
        // 4) 最老的活跃 voice（position 最大）。
        // 这样可最大限度避免新 NoteOn 被静默丢弃，并减少截断感。
        var slot = u.mv;
        for (var i = 0u; i < u.mv; i++) {
            if params[i].ena == 0u { slot = i; break; }
        }
        if slot == u.mv {
            for (var i = 0u; i < u.mv; i++) {
                let p = params[i];
                if p.rel != 0u && p.rel_elapsed > RELEASE_STEAL_AGE {
                    slot = i; break;
                }
            }
        }
        if slot == u.mv {
            var oldest_rel = 0.0f;
            for (var i = 0u; i < u.mv; i++) {
                let p = params[i];
                if p.rel != 0u && p.rel_elapsed > oldest_rel {
                    oldest_rel = p.rel_elapsed; slot = i;
                }
            }
        }
        if slot == u.mv {
            var oldest_pos = 0.0f;
            for (var i = 0u; i < u.mv; i++) {
                let p = params[i];
                if p.ena != 0u && p.rel == 0u && p.pos > oldest_pos {
                    oldest_pos = p.pos; slot = i;
                }
            }
        }
        if slot == u.mv { return; }

        let semis = f32(ev_key) - f32(r.rk) + f32(r.tn) / 100.0;
        let pitch = pow(2.0, semis / 12.0);
        params[slot] = VP(
            0.0,                               // pos = 0 → 音符从 sample 开头开始
            pitch,
            (f32(ev_vel) / 127.0) * r.vol,
            f32(r.pan) / 64.0,
            r.bo, r.bo + r.bl,
            r.bo + r.ls, r.bo + r.le,
            1u,
            select(0u, 1u, r.lm == 1u || r.lm == 2u),
            ev_ch, ev_key,
            0u,                               // released
            0u,                               // release_frame
            ev.to,                            // sf = 本块中音符触发的 sample offset
            0.0,                              // rel_elapsed = release 累计样本数
        );
    } else if ev_kind == 1u {
        for (var i = 0u; i < u.mv; i++) {
            let p = params[i];
            if p.ena != 0u && p.ch == ev_ch && p.ky == ev_key && p.rel == 0u {
                params[i].rel = 1u;
                params[i].rf = ev.to;
                break;
            }
        }
    }
}
"#;

// ── WGSL: render ────────────────────────────────────
// [架构修复] 原设计将 voices 按 vi=li, li+WGS 分布到线程，每个 voice 只贡献
// 4/1024 sample（li=0 只处理 voices 0,256 → 只在 sidx=0,256,512,768 处理）。
// 再加上 workgroup reduction 的 li==0u 守卫只写 sidx=0,256,512,768 → 其余 1020
// sample 永远为 0。
//
// 修复：每个线程遍历所有 voice，直接写入自己的 out[sidx*ch..]。
pub(crate) const RENDER_SRC: &str = r#"
const MV: u32 = 2048u;
const WGS: u32 = 256u;

struct VP { pos: f32, pitch: f32, vol: f32, pan: f32, ss: u32, se: u32, ls: u32, le: u32, ena: u32, lp: u32, ch: u32, ky: u32, rel: u32, rf: u32, sf: u32, rel_elapsed: f32, }
struct U { ne: u32, nr: u32, ns: u32, sr: u32, mv: u32, ch: u32, }

@group(0) @binding(0) var<storage, read> params: array<VP>;
@group(0) @binding(1) var<storage, read> smpls: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;
@group(0) @binding(3) var<uniform> u: U;

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>,
) {
    let sidx = id.x;
    if sidx >= u.ns { return; }

    var L = 0.0f; var R = 0.0f;
    for (var vi = 0u; vi < u.mv; vi++) {
        let p = params[vi];
        if p.ena != 0u {
            // 跳过高音在 start_frame 之前的样本（新音符在本块中间触发）
            if sidx < p.sf { continue; }

            var env = 1.0f;
            if p.rel != 0u && sidx >= p.rf {
                // [Bug Fix] 跨块累计 release_elapsed + 当前块内 sidx - rf
                // 保证 release envelope 跨块连续不重启。用 0.999 替代 0.995 延长
                // release tail 从 ~31ms 到 ~157ms，防止音符被截断。
                let rel_samples = p.rel_elapsed + f32(sidx - p.rf);
                env = pow(0.999, rel_samples);
                if env < 0.001 { env = 0.0; }
            }
            // pos = p.pos + (sidx - sf) * pitch：
            //   新音符 (pos=0, sf=to)：sidx=to 时 pos=0 ← 音符从 sample 0 开始
            //   旧音符 (pos=prev_end, sf=0)：sidx=0 时 pos=prev_end ← 连续
            let pos = p.pos + f32(sidx - p.sf) * p.pitch;
            var pi = u32(pos);
            let fr = pos - f32(pi);
            // [Bug Fix] pi 是 sample 内相对偏移（0-based），但 p.le/p.ls/p.se 是
            // 绝对 flat buffer 索引。用 len/le_rel/ls_rel 做相对比较。
            let len = p.se - p.ss;
            if p.lp != 0u {
                let le_rel = p.le - p.ss;
                let ls_rel = p.ls - p.ss;
                if le_rel > ls_rel {
                    let llen = le_rel - ls_rel;
                    if llen > 0u && pi >= le_rel {
                        pi = ls_rel + (pi - ls_rel) % llen;
                    }
                }
            }
            // [Bug Fix] pi < p.se - 1u 在 bo>0 时允许 pi 超过 bl，读取跨 sample 数据。
            // 正确边界：pi < len - 1u（需要 2 个 sample 做线性插值）。
            if pi < len - 1u {
                let i0 = p.ss + pi; let i1 = i0 + 1u;
                let sv = smpls[i0] + (smpls[i1] - smpls[i0]) * fr;
                let lg = p.vol * env * sqrt(max(1.0 - p.pan, 0.0));
                let rg = p.vol * env * sqrt(max(1.0 + p.pan, 0.0));
                L += sv * lg; R += sv * rg;
            }
        }
    }

    // [Bug Fix] 原为 workgroup reduction + li==0u 守卫，只写 4/1024 sample。
    // 修复后每线程直接写自己的 sidx，无竞态（每个 sidx 唯一）。
    //
    // [Bug Fix] 支持单声道输出：shader 根据 u.ch 写 1 或 2 个通道，避免
    // Mono 导出时 out buffer 越界。
    // Master gain: 防止多 voice 求和超出 [-1,1] 导致削波滋滋声。
    // xsynth CPU 路径有内部 gain staging，GPU 路径需要显式控制。
    // 1/8 = 12.5%，给大约 8 个满音量 voice 的 headroom。
    if u.ch == 1u {
        out[sidx] = (L + R) * 0.125;
    } else {
        let oi = sidx * 2u;
        out[oi] = L * 0.125;
        out[oi + 1u] = R * 0.125;
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::{EVENT_PROC_SRC, RENDER_SRC};
    #[test]
    fn validate_wgsl_shaders() {
        naga::front::wgsl::parse_str(EVENT_PROC_SRC).expect("event_proc WGSL");
        naga::front::wgsl::parse_str(RENDER_SRC).expect("render WGSL");
    }
}
