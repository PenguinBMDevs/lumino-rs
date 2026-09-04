//! Top 视图音符精度降级：时间量化 + 同键合并
//!
//! Top 为俯视（参考 Comet MIDITrail `Top Down Above` 预设），允许降低音符
//! 显示精度换取 GPU 开销下降：
//! - **时间量化**：音符起止 tick 对齐到 [`TOP_QUANT_TICKS`] 网格
//!   （边缘 artifact 即来源于此，验收时需目视确认可接受）；
//! - **同键合并**：量化后重叠/相连的同键音符合并为一个实例，
//!   密集段落实例数显著下降（顶点 + 片元双省）；
//! - Normal 路径不经过本模块（零改动，防两套设置互相污染）。

use super::types::{MiditrailNoteGpu, MiditrailUniformGpu};

/// Top 视图时间量化步长（tick；ppq=480 时相当于 32 分音符）。
///
/// 步长越大合并收益越高、边缘 artifact 越明显；96 为经验折中
/// （120BPM 下约 25ms，远小于人眼可察觉的时值偏差）。
pub const TOP_QUANT_TICKS: u32 = 96;

/// 对可见音符做 Top 视图降精度：按 `tick` 过滤可见音符 → 同键分组 →
/// 起止量化到网格 → 重叠/相连合并（颜色/力度/通道沿用首个区间的）。
///
/// 返回的新切片可直接送入 `build_note_instances`（几何映射与 Comet
/// 绘制顺序排序逻辑复用，不另起一套）。
#[must_use]
pub fn quantize_notes_for_top(
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
) -> Vec<MiditrailNoteGpu> {
    let tick = uniform.tick;
    let quant = TOP_QUANT_TICKS.max(1);

    // 同键分组（key 范围固定 0..127，用数组分桶代替 HashMap）。
    let mut buckets: [Vec<(u32, u32, MiditrailNoteGpu)>; 128] = std::array::from_fn(|_| Vec::new());
    for note in notes {
        if !note.is_visible_at(tick) {
            continue;
        }
        let key = note.key as usize;
        if key >= 128 {
            continue;
        }
        let start_q = note.start_tick - note.start_tick % quant;
        let end_q = note
            .end_tick
            .saturating_add((quant - note.end_tick % quant) % quant);
        buckets[key].push((start_q, end_q.max(start_q + 1), *note));
    }

    let mut out = Vec::with_capacity(notes.len());
    for bucket in buckets.iter_mut() {
        if bucket.is_empty() {
            continue;
        }
        bucket.sort_unstable_by_key(|(start, _, _)| *start);
        let mut iter = bucket.drain(..);
        let (mut cur_start, mut cur_end, mut cur_first) = iter.next().expect("非空桶必有首个区间");
        for (start, end, _first) in iter {
            if start <= cur_end {
                // 量化后重叠/相连 → 合并（颜色沿用首个区间，保证稳定）。
                cur_end = cur_end.max(end);
            } else {
                out.push(with_span(cur_first, cur_start, cur_end));
                (cur_start, cur_end, cur_first) = (start, end, _first);
            }
        }
        out.push(with_span(cur_first, cur_start, cur_end));
    }
    out
}

/// 用量化合并后的起止替换音符的 tick 范围，其余字段沿用首个区间。
fn with_span(first: MiditrailNoteGpu, start: u32, end: u32) -> MiditrailNoteGpu {
    MiditrailNoteGpu {
        start_tick: start,
        end_tick: end,
        ..first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(key: u32, start: u32, end: u32, color: u32) -> MiditrailNoteGpu {
        MiditrailNoteGpu {
            key,
            start_tick: start,
            end_tick: end,
            color_packed: color,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        }
    }

    fn uniform_at(tick: u32) -> MiditrailUniformGpu {
        MiditrailUniformGpu {
            tick,
            ..MiditrailUniformGpu::default()
        }
    }

    #[test]
    fn test_top_quantize_merges_overlapping_same_key() {
        let notes = vec![
            note(60, 0, 200, 0xFF0000FF),
            note(60, 100, 300, 0x00FF00FF),
            note(60, 500, 600, 0x0000FFFF),
        ];
        let out = quantize_notes_for_top(&uniform_at(0), &notes);
        // 量化：(0,288) + (96,384) 重叠合并为 (0,384)；(480,672) 独立。
        assert_eq!(out.len(), 2, "应合并为 2 个实例，实际 {out:?}");
        assert_eq!((out[0].start_tick, out[0].end_tick), (0, 384));
        assert_eq!((out[1].start_tick, out[1].end_tick), (480, 672));
        // 合并后颜色沿用首个区间（稳定不闪烁）。
        assert_eq!(out[0].color_packed, 0xFF0000FF);
    }

    #[test]
    fn test_top_quantize_snaps_edges_to_grid() {
        let out = quantize_notes_for_top(&uniform_at(0), &[note(60, 10, 200, 1)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_tick, 0, "起始应向下取整到网格");
        assert_eq!(out[0].end_tick, 288, "结束应向上取整到网格");
    }

    #[test]
    fn test_top_quantize_keeps_keys_separate() {
        let notes = vec![note(60, 0, 500, 1), note(61, 0, 500, 2)];
        let out = quantize_notes_for_top(&uniform_at(0), &notes);
        assert_eq!(out.len(), 2, "不同键永不合并");
    }

    #[test]
    fn test_top_quantize_filters_invisible_and_clamps_key() {
        let notes = vec![
            note(60, 0, 100, 1),   // tick=1000 时已结束
            note(200, 0, 2000, 2), // 非法 key
            note(61, 900, 2000, 3),
        ];
        let out = quantize_notes_for_top(&uniform_at(1000), &notes);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, 61);
    }

    #[test]
    fn test_top_quantized_never_exceeds_input() {
        // 确定性伪随机密集输入：合并只减不增（GPU 收益单调）。
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let notes: Vec<MiditrailNoteGpu> = (0..5000)
            .map(|_| {
                let start = (next() % 50_000) as u32;
                note(
                    (next() % 8) as u32 + 60,
                    start,
                    start + (next() % 960) as u32 + 1,
                    1,
                )
            })
            .collect();
        let out = quantize_notes_for_top(&uniform_at(0), &notes);
        assert!(
            out.len() < notes.len(),
            "密集输入应显著合并：{} → {}",
            notes.len(),
            out.len()
        );
    }
}
