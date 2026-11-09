//! 走带标尺的小节边界计算。
//!
//! 与 iced 侧 `ui-editor/src/grid/ruler.rs` 的 `MeasureIterator` **同口径**：
//! - 同一 tick 的重复拍号只保留最后一个（多轨 MIDI 常见，否则首个可见小节号会从 2 起跳）；
//! - 第 0 段起点恒为 tick 0，按 `time_signature_at(0.0)` 取拍号；
//! - 后续段从各自 `tick` 起，若跨段时该段不足一小节，也按 1 小节计入编号；
//! - 小节号跨段连续递增，不重置。

use std::borrow::Cow;

/// 拍号表去重：同 tick 只保留最后一个定义（与「最后一个 ≤ tick 生效」语义一致）。
fn dedup(sigs: &[(u32, u8, u8)]) -> Cow<'_, [(u32, u8, u8)]> {
    if !sigs.windows(2).any(|w| w[0].0 == w[1].0) {
        return Cow::Borrowed(sigs);
    }
    let mut out: Vec<(u32, u8, u8)> = Vec::with_capacity(sigs.len());
    for &(tick, num, den) in sigs {
        if let Some(last) = out.last_mut()
            && last.0 == tick
        {
            *last = (tick, num, den);
        } else {
            out.push((tick, num, den));
        }
    }
    Cow::Owned(out)
}

/// 某个 tick 生效的拍号（最后一个起始 ≤ tick 的段）。
fn signature_at(tick: f32, sigs: &[(u32, u8, u8)]) -> (u8, u8) {
    let mut active = (4_u8, 4_u8);
    for &(ts_tick, num, den) in sigs {
        if tick >= ts_tick as f32 {
            active = (num, den);
        } else {
            break;
        }
    }
    active
}

/// 每小节 tick 数。
fn ticks_per_measure(ppq: f32, numerator: u8, denominator: u8) -> f32 {
    let beat = ppq * 4.0 / denominator.max(1) as f32;
    beat * numerator.max(1) as f32
}

/// 段 0 的起点恒为 0（与 iced 侧一致）。
fn segment_start(sigs: &[(u32, u8, u8)], index: usize) -> f32 {
    if index == 0 {
        return 0.0;
    }
    sigs.get(index).map_or(0.0, |&(tick, _, _)| tick as f32)
}

/// 容纳 `target` 的段下标（最后一个起始 ≤ target 的段）。
fn segment_index_at(sigs: &[(u32, u8, u8)], target: f32) -> usize {
    let mut index = 0;
    for (i, &(tick, _, _)) in sigs.iter().enumerate() {
        if (tick as f32) <= target {
            index = i;
        } else {
            break;
        }
    }
    index
}

/// 段 `index` 内的小节数（跨段时不足一小节也算 1）。
fn measures_in_segment(sigs: &[(u32, u8, u8)], ppq: f32, index: usize) -> Option<u32> {
    let start = segment_start(sigs, index);
    let end = sigs.get(index + 1).map(|&(tick, _, _)| tick as f32)?;
    let (num, den) = signature_at(start, sigs);
    let ticks = ticks_per_measure(ppq, num, den).max(f32::EPSILON);
    Some((((end - start) / ticks).ceil() as u32).max(1))
}

/// `[from, to]` 内的小节边界（tick, 小节号），按 tick 升序。
///
/// 复杂度与输出条数同阶：先定位段与小节号前缀，再只遍历可见段。
pub(crate) fn measure_marks(
    time_signatures: &[(u32, u8, u8)],
    ppq: f32,
    from: f32,
    to: f32,
) -> Vec<(f32, u32)> {
    /// 防御上限：病态文件（巨大 tick 跨度 + 极小 zoom）不应拖死一帧。
    const MAX_MARKS: usize = 4096;

    if to < from {
        return Vec::new();
    }
    let sigs = dedup(time_signatures);
    let sigs = sigs.as_ref();
    let ppq = ppq.max(1.0);

    // 每段的小节长度与（可选）小节数。
    let mut segment_ticks: Vec<f32> = Vec::with_capacity(sigs.len().max(1));
    let mut prefix: Vec<u32> = Vec::with_capacity(sigs.len().max(1));
    let mut number = 1_u32;
    for index in 0..sigs.len().max(1) {
        let start = segment_start(sigs, index);
        let (num, den) = signature_at(start, sigs);
        segment_ticks.push(ticks_per_measure(ppq, num, den).max(f32::EPSILON));
        prefix.push(number);
        if let Some(count) = measures_in_segment(sigs, ppq, index) {
            number = number.saturating_add(count);
        }
    }

    let first = segment_index_at(sigs, from);
    let mut out = Vec::new();
    for index in first..sigs.len().max(1) {
        let start = segment_start(sigs, index);
        if start > to {
            break;
        }
        let ticks = segment_ticks[index];
        let seg_end = sigs
            .get(index + 1)
            .map_or(f32::INFINITY, |&(tick, _, _)| tick as f32);

        // 段内起始下标：跳过窗口左侧的小节（编号仍按段前缀累加）。
        let mut local = if from > start {
            ((from - start) / ticks).ceil().max(0.0) as u32
        } else {
            0
        };
        loop {
            let tick = start + local as f32 * ticks;
            // 段末边界属于下一段（`ceil` 已把不足一小节的尾段计入本段编号）。
            if tick >= seg_end || tick > to {
                break;
            }
            out.push((tick, prefix[index] + local));
            if out.len() >= MAX_MARKS {
                return out;
            }
            local += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PPQ: f32 = 960.0;

    #[test]
    fn four_four_measures_every_3840_ticks() {
        let sigs = [(0, 4, 4)];
        let marks = measure_marks(&sigs, PPQ, 0.0, 3840.0 * 3.0);
        let ticks: Vec<f32> = marks.iter().map(|(t, _)| *t).collect();
        assert_eq!(ticks, vec![0.0, 3840.0, 7680.0, 11520.0]);
        let numbers: Vec<u32> = marks.iter().map(|(_, n)| *n).collect();
        assert_eq!(numbers, vec![1, 2, 3, 4], "小节号必须从 1 连续递增");
    }

    #[test]
    fn duplicate_signatures_at_same_tick_do_not_shift_numbering() {
        // 多轨 MIDI 常见：两轨各写一个 tick 0 的 4/4（#146）。
        let sigs = [(0, 4, 4), (0, 4, 4)];
        let marks = measure_marks(&sigs, PPQ, 0.0, 3840.0 * 2.0);
        let numbers: Vec<u32> = marks.iter().map(|(_, n)| *n).collect();
        assert_eq!(numbers, vec![1, 2, 3], "首个可见小节号不得从 2 起跳");
    }

    #[test]
    fn three_four_changes_measure_length() {
        let sigs = [(0, 3, 4)];
        let marks = measure_marks(&sigs, PPQ, 0.0, 2880.0 * 2.0);
        let ticks: Vec<f32> = marks.iter().map(|(t, _)| *t).collect();
        assert_eq!(ticks, vec![0.0, 2880.0, 5760.0]);
    }

    #[test]
    fn numbering_continues_across_signature_change() {
        // 4/4 两小节（0..7680），tick 7680 起改 3/4。
        let sigs = [(0, 4, 4), (7680, 3, 4)];
        let marks = measure_marks(&sigs, PPQ, 0.0, 7680.0 + 2880.0);
        let numbers: Vec<u32> = marks.iter().map(|(_, n)| *n).collect();
        assert_eq!(numbers, vec![1, 2, 3, 4], "跨拍号变化时小节号连续");
        let ticks: Vec<f32> = marks.iter().map(|(t, _)| *t).collect();
        assert_eq!(ticks, vec![0.0, 3840.0, 7680.0, 10560.0]);
    }

    #[test]
    fn scrolling_skips_boundaries_before_window_but_keeps_numbers() {
        let sigs = [(0, 4, 4)];
        let marks = measure_marks(&sigs, PPQ, 3840.0 * 2.0 + 1.0, 3840.0 * 4.0);
        let ticks: Vec<f32> = marks.iter().map(|(t, _)| *t).collect();
        assert_eq!(ticks, vec![11520.0, 15360.0]);
        let numbers: Vec<u32> = marks.iter().map(|(_, n)| *n).collect();
        assert_eq!(numbers, vec![4, 5]);
    }
}
