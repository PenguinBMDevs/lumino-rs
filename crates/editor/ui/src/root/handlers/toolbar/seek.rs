//! 走带定位：按小节步进与全曲边界换算。
//!
//! 小节线口径与标尺一致：首个拍号段从 tick 0 起（缺省 4/4），同 tick 的重复
//! 拍号只保留最后一条，拍号中途变化时下一小节从变化 tick 起算。拍号段短于
//! 一小节时，也把该段起点当成一条小节线（标尺同样会画出来）。
//!
//! 快退不是单纯「严格上一条线」：本小节前 1/2 退到上一个小节，后 1/2（含正中）
//! 回本小节开头。这样在小节内任意位置单击都能获得符合走带直觉的「退一小节」。
//!
//! 本模块只做纯换算，不持有 `Root`，便于单测覆盖变拍号 / 重复拍号等边界。

use lumino_midi_model::MidiDocument;

/// 全局 EOT（全曲末尾）tick。
///
/// 取「文档声明的结束 tick」与「音符实际末尾」的较大者：
/// - 载入的 MIDI 以各轨 EOT 的最大值作为 `total_ticks`（可能晚于最后一个音符）；
/// - 编辑后新增的音符可能越过声明结尾（`tracks_max_end_tick` 随编辑增量维护）。
///
/// 只取其一都会让"跳到末尾"落在错误位置，故两者取大。
pub(super) fn global_eot_tick(document: Option<&MidiDocument>) -> f32 {
    document
        .map(|doc| doc.total_ticks.max(doc.tracks_max_end_tick()))
        .unwrap_or(0) as f32
}

/// 下一个小节线（严格大于 `tick`）。
///
/// `tick` 正好落在小节线上时返回再下一条；落在小节内时返回本条之后的
/// 小节线。拍号变化 tick 本身也是一条小节线。
pub(super) fn next_measure_tick(tick: f32, ppq: u16, signatures: &[(u32, u8, u8)]) -> f32 {
    let tick = tick.max(0.0);
    for segment in segments(ppq, signatures) {
        if tick >= segment.end {
            continue;
        }
        if tick < segment.start {
            return segment.start;
        }
        let local = (tick - segment.start) / segment.len;
        let boundary = segment.start + (local.floor() + 1.0) * segment.len;
        return boundary.min(segment.end);
    }
    // 只有 `tick` 非有限值才会走到这里；保持"不前进"比给出错误位置安全。
    tick
}

/// 上一个小节起点（走带「快退」按钮的实际语义）。
///
/// 以**当前小节**为基准分两半：
/// - 指示线在本小节前 1/2：跳到**上一个小节**的起点；
/// - 指示线在本小节后 1/2（含正中）：回到**本小节**开头。
///
/// 正好落在小节线上时视为「本小节开头」（前 1/2），因此会退到再上一个小节，
/// 与「正落在小节线上应后退一整小节」的走带直觉一致。已在开头时返回 0。
pub(super) fn previous_measure_tick(tick: f32, ppq: u16, signatures: &[(u32, u8, u8)]) -> f32 {
    let tick = tick.max(0.0);
    let segments = segments(ppq, signatures);
    let Some(segment) = segments.iter().find(|segment| tick < segment.end) else {
        // 只有 `tick` 非有限值才会走到这里；保持"不后退"比给出错误位置安全。
        return tick;
    };

    // 当前小节在拍号段内的起点与"实际"终点（短段会被下一拍号截断）。
    let local = ((tick - segment.start).max(0.0) / segment.len).floor();
    let measure_start = segment.start + local * segment.len;
    let measure_end = (measure_start + segment.len).min(segment.end);

    if (tick - measure_start) * 2.0 >= measure_end - measure_start {
        measure_start
    } else {
        previous_barline(&segments, measure_start)
    }
}

/// 严格小于 `tick` 的最近一条小节线（不含 `tick` 自身）。
///
/// 供 [`previous_measure_tick`] 在前 1/2 时取「本小节开头之前的那条线」；
/// 段短于一小节时只允许段起点这一条线，不能把边界算到下一段去。
fn previous_barline(segments: &[Segment], tick: f32) -> f32 {
    let mut previous = 0.0_f32;
    for segment in segments {
        if segment.start >= tick {
            break;
        }
        let upper = segment.end.min(tick);
        if upper <= segment.start {
            continue;
        }
        let raw_k = ((upper - segment.start) / segment.len).ceil() as i64 - 1;
        if raw_k < 0 {
            continue;
        }
        let max_k = if segment.end.is_finite() {
            (((segment.end - segment.start) / segment.len).ceil() as i64 - 1).max(0)
        } else {
            raw_k
        };
        let boundary = segment.start + raw_k.min(max_k) as f32 * segment.len;
        if boundary < tick && boundary > previous {
            previous = boundary;
        }
    }
    previous
}

/// 一个拍号段：起点、终点（下一段起点 / 无穷）、每小节 tick 数。
struct Segment {
    start: f32,
    end: f32,
    len: f32,
}

/// 拍号表 → 段列表（排序、同 tick 保留最后一条、补默认 4/4）。
fn segments(ppq: u16, signatures: &[(u32, u8, u8)]) -> Vec<Segment> {
    let mut sorted = signatures.to_vec();
    sorted.sort_by_key(|&(tick, _, _)| tick);

    // 同 tick 只保留最后一条（多轨 MIDI 常见；与标尺「后者覆盖」一致）。
    let mut unique: Vec<(u32, u8, u8)> = Vec::with_capacity(sorted.len());
    for signature in sorted {
        match unique.last_mut() {
            Some(last) if last.0 == signature.0 => *last = signature,
            _ => unique.push(signature),
        }
    }
    if unique.first().is_none_or(|&(tick, _, _)| tick != 0) {
        unique.insert(0, (0, 4, 4));
    }

    let ppq = f32::from(ppq.max(1));
    let mut segments = Vec::with_capacity(unique.len());
    for (index, &(start, numerator, denominator)) in unique.iter().enumerate() {
        let end = unique
            .get(index + 1)
            .map_or(f32::INFINITY, |&(tick, _, _)| tick as f32);
        let len =
            (ppq * 4.0 * f32::from(numerator.max(1)) / f32::from(denominator.max(1))).max(1.0);
        segments.push(Segment {
            start: start as f32,
            end,
            len,
        });
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::NoteEvent;

    const PPQ: u16 = 960;
    /// 4/4 一小节 = 960 × 4 = 3840 tick。
    const BAR: f32 = 3840.0;

    #[test]
    fn next_measure_strictly_advances_to_the_next_barline() {
        let sigs = [(0, 4, 4)];
        assert_eq!(next_measure_tick(0.0, PPQ, &sigs), BAR);
        assert_eq!(next_measure_tick(1.0, PPQ, &sigs), BAR);
        assert_eq!(next_measure_tick(BAR - 1.0, PPQ, &sigs), BAR);
        assert_eq!(
            next_measure_tick(BAR, PPQ, &sigs),
            BAR * 2.0,
            "正落在小节线上时应前进到再下一条"
        );
    }

    #[test]
    fn previous_measure_uses_half_measure_threshold() {
        let sigs = [(0, 4, 4)];
        assert_eq!(previous_measure_tick(0.0, PPQ, &sigs), 0.0);
        assert_eq!(previous_measure_tick(1.0, PPQ, &sigs), 0.0);
        assert_eq!(
            previous_measure_tick(BAR + 1.0, PPQ, &sigs),
            0.0,
            "本小节前 1/2 应退到上一个小节"
        );
        assert_eq!(
            previous_measure_tick(BAR + BAR / 2.0 - 1.0, PPQ, &sigs),
            0.0
        );
        assert_eq!(
            previous_measure_tick(BAR + BAR / 2.0, PPQ, &sigs),
            BAR,
            "正好 1/2 视为后 1/2，回到本小节开头"
        );
        assert_eq!(previous_measure_tick(BAR + BAR - 1.0, PPQ, &sigs), BAR);
        assert_eq!(
            previous_measure_tick(BAR * 2.0, PPQ, &sigs),
            BAR,
            "正落在小节线上应后退一整小节"
        );
        assert_eq!(previous_measure_tick(BAR * 2.0 + 100.0, PPQ, &sigs), BAR);
        assert_eq!(
            previous_measure_tick(BAR * 2.0 + BAR / 2.0, PPQ, &sigs),
            BAR * 2.0
        );
    }

    #[test]
    fn three_four_changes_measure_length() {
        let sigs = [(0, 3, 4)];
        let bar = 960.0 * 3.0;
        assert_eq!(next_measure_tick(0.0, PPQ, &sigs), bar);
        assert_eq!(next_measure_tick(bar, PPQ, &sigs), bar * 2.0);
        assert_eq!(
            previous_measure_tick(bar + 10.0, PPQ, &sigs),
            0.0,
            "3/4 本小节前 1/2 退上一小节"
        );
        assert_eq!(
            previous_measure_tick(bar + bar / 2.0, PPQ, &sigs),
            bar,
            "3/4 本小节后 1/2 回本小节开头"
        );
    }

    #[test]
    fn mid_project_signature_change_starts_a_new_measure() {
        // 4/4 两小节后（7680）改 3/4：7680 本身是小节线，下一小节 = 7680 + 2880。
        let sigs = [(0, 4, 4), (7680, 3, 4)];
        assert_eq!(next_measure_tick(7000.0, PPQ, &sigs), 7680.0);
        assert_eq!(next_measure_tick(7680.0, PPQ, &sigs), 10560.0);
        assert_eq!(
            previous_measure_tick(8000.0, PPQ, &sigs),
            3840.0,
            "变拍后本小节前 1/2 仍应退到上一小节（3840）"
        );
        assert_eq!(
            previous_measure_tick(7680.0 + 1440.0, PPQ, &sigs),
            7680.0,
            "变拍后本小节后 1/2 回到 7680"
        );
        assert_eq!(previous_measure_tick(7680.0, PPQ, &sigs), 3840.0);
    }

    #[test]
    fn short_segment_counts_its_start_as_a_barline() {
        // 拍号在 1000 变化，首段不足一小节：1000 是边界，不能跳到 3840。
        let sigs = [(0, 4, 4), (1000, 3, 4)];
        assert_eq!(next_measure_tick(500.0, PPQ, &sigs), 1000.0);
        assert_eq!(previous_measure_tick(500.0, PPQ, &sigs), 0.0);
        assert_eq!(
            previous_measure_tick(1500.0, PPQ, &sigs),
            0.0,
            "3/4 段前 1/2 应退到上一段起点 0"
        );
        assert_eq!(
            previous_measure_tick(1000.0 + 1440.0, PPQ, &sigs),
            1000.0,
            "3/4 段后 1/2 回本段起点"
        );
    }

    #[test]
    fn duplicate_signatures_keep_last_and_empty_falls_back_to_four_four() {
        // 同 tick 重复：后者覆盖（3/4 生效）。
        let duplicate = [(0, 4, 4), (0, 3, 4)];
        assert_eq!(next_measure_tick(0.0, PPQ, &duplicate), 2880.0);

        // 空表 = 默认 4/4。
        assert_eq!(next_measure_tick(0.0, PPQ, &[]), BAR);
    }

    #[test]
    fn global_eot_prefers_later_of_declared_end_and_note_end() {
        assert_eq!(global_eot_tick(None), 0.0, "无文档时不能给出虚假末尾");

        let mut doc = crate::test_helpers::make_test_document();
        doc.total_ticks = 1000;
        doc.notes[0].push_back(NoteEvent::new(0, 2000, 60, 100, 0));
        assert_eq!(
            global_eot_tick(Some(&doc)),
            2000.0,
            "编辑产生的越界音符必须把末尾向后推"
        );
    }
}
