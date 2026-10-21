//! Gate-aware flip（门控感知翻转，DRAW-009）
//!
//! 语义对应上游 `scripts/notes/gateflip.py`：只把每个 key 行内**每个 run 的门控排布**
//! 左右镜像，画的位置与 run 边界不动；力度随音符一起镜像，track/颜色保持原映射。
//!
//! run 划分与上游 `sustain.row_order` 同口径：按 key 行按起点排序，gap 是「与同 key
//! 前一条已发声结束之间的静音」；全形状最常见 gap + 其一半为阈值，超过即断开。
//!
//! 位置：Notes 阶段、Glue 之后、Sustain 之前（DRAW-009 卡口径）。上游 `gateflip.py`
//! 注释写的是 before glue；本移植沿用 DRAW-003 的 Glue-first 链约束，故在此注明差异。

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::ShapeNote;

/// 门控翻转不可用的原因（供 UI 置灰）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateFlipReason {
    /// 每行只有一条音符（或所有音符 key 都不同）：镜像不会改变任何东西
    OneGate,
    /// 形状不是 spam 类填充：没有门控排布可翻
    NoGates,
}

/// 应用门控翻转；输出保持输入数组顺序，只改 `tick`（`length`/`velocity`/`track` 随音符）
pub fn apply(notes: &[ShapeNote]) -> Vec<ShapeNote> {
    if notes.len() < 2 {
        return notes.to_vec();
    }
    let (order, gaps) = row_order(notes);
    let threshold = gap_of(&gaps);

    // 按行序给每个音符分配 run；行首（gap < 0）或越过阈值处断开
    let mut run_ids = Vec::with_capacity(order.len());
    let mut current = 0usize;
    for (position, gap) in gaps.iter().enumerate() {
        if position > 0 && (*gap < 0.0 || *gap > threshold) {
            current += 1;
        }
        run_ids.push(current);
    }
    let run_count = current + 1;
    let mut lo = vec![f32::INFINITY; run_count];
    let mut hi = vec![f32::NEG_INFINITY; run_count];
    for (position, &index) in order.iter().enumerate() {
        let note = notes[index];
        let end = note.tick + note.length.max(0.0);
        let run = run_ids[position];
        lo[run] = lo[run].min(note.tick);
        hi[run] = hi[run].max(end);
    }

    // 区间镜像：new_start = lo + hi - end；长度不变 ⇒ new_end = lo + hi - start
    let mut out = notes.to_vec();
    for (position, &index) in order.iter().enumerate() {
        let note = notes[index];
        let end = note.tick + note.length.max(0.0);
        let run = run_ids[position];
        out[index] = ShapeNote {
            tick: lo[run] + hi[run] - end,
            length: note.length.max(0.0),
            ..note
        };
    }
    out
}

/// 与 DRAW-008 Sustain 共用的 Auto gap（tick）：最常见静音 + 其一半
///
/// 返回的是**断开阈值**（已含 `+ max(1, most/2)`），不是原始众数。
pub fn auto_gap(notes: &[ShapeNote]) -> f32 {
    if notes.len() < 2 {
        return 1.0;
    }
    let (_, gaps) = row_order(notes);
    gap_of(&gaps)
}

/// 可翻转性判定：`None` = 可翻；否则给出置灰原因
///
/// `is_spam_fill` 由调用方按 `ShapeStyle.fill` 判定（本函数不依赖形状类型）。
pub fn why_not(notes: &[ShapeNote], is_spam_fill: bool) -> Option<GateFlipReason> {
    if !is_spam_fill {
        return Some(GateFlipReason::NoGates);
    }
    if notes.len() < 2 {
        return Some(GateFlipReason::OneGate);
    }
    // 每行只有一条音符 ⇔ key 的去重数 == 音符数
    let mut keys: Vec<u16> = notes.iter().map(|note| note.key).collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.len() == notes.len() {
        Some(GateFlipReason::OneGate)
    } else {
        None
    }
}

/// 按 (key, 起点) 排序，并给出每条音符与同 key 前一条「已发声结束」之间的静音
///
/// 行首记 `-1.0`；同 key 的 track/颜色视为同一行（上游 `row_order` 口径）。
fn row_order(notes: &[ShapeNote]) -> (Vec<usize>, Vec<f32>) {
    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by(|&a, &b| {
        notes[a]
            .key
            .cmp(&notes[b].key)
            .then(
                notes[a]
                    .tick
                    .partial_cmp(&notes[b].tick)
                    .unwrap_or(Ordering::Equal),
            )
            .then(a.cmp(&b))
    });

    let mut gaps = vec![-1.0_f32; order.len()];
    let mut key: Option<u16> = None;
    let mut running_end = 0.0_f32;
    for (position, &index) in order.iter().enumerate() {
        let note = notes[index];
        let end = note.tick + note.length.max(0.0);
        if key == Some(note.key) {
            gaps[position] = (note.tick - running_end).max(0.0);
            running_end = running_end.max(end);
        } else {
            key = Some(note.key);
            running_end = end;
        }
    }
    (order, gaps)
}

/// 上游 `gap_of`：非负 gap 的众数 + `max(1, 众数/2)`；无 gap 时 1 tick
fn gap_of(gaps: &[f32]) -> f32 {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for &gap in gaps {
        if gap < 0.0 || !gap.is_finite() {
            continue;
        }
        let tick = gap.round();
        if tick < 0.0 || tick > i64::MAX as f32 {
            continue;
        }
        *counts.entry(tick as i64).or_default() += 1;
    }
    // 众数并列时取最小的 tick（上游 np.unique + argmax 的首个最大值）
    let mut best: Option<(i64, usize)> = None;
    for (&tick, &count) in &counts {
        if best.is_none_or(|(_, best_count)| count > best_count) {
            best = Some((tick, count));
        }
    }
    let Some((most, _)) = best else {
        return 1.0;
    };
    let most = most.max(0);
    (most + (most / 2).max(1)) as f32
}

#[cfg(test)]
mod tests;
