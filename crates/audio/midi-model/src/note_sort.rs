//! 轨内音符排序：稳定基数排序（巨轨）+ 标准库稳定排序（小轨）。
//!
//! 背景（164M 素材实测，见 midly-fork `perf/results/p5-sort-proto.tsv`）：
//! 逐轨 handler 中「按 start_tick 排序」占 62%；8.8M 音符巨轨的稳定比较排序
//! 330ms，基数排序 80ms（-76%），端到端 754 → 557ms（-26%）。
//!
//! 语义：LSD 基数排序是**稳定**的，输出与 `sort_by_key(|n| n.start_tick)`
//! 逐元素一致（单测 + 164M digest 对拍）。
//!
//! 分流规则：
//! - 音符数 < [`RADIX_MIN_NOTES`]：标准库稳定排序。小数组在缓存内比较更快，
//!   且标准库对「已排序 / 长 run」自适应（Pi 最大轨已排序时仅 0.17ms）。
//! - 巨轨：先一次线性扫描（已排序则直接跳过 / 取 max tick），再按实际位宽
//!   做 2–3 趟 11 位基数排序。已排序的大轨（真实单乐器轨常见）因此几乎零成本。

use midly::loader::PackedNote;

/// 基数排序的位数（每趟 2^11 个桶）。
const RADIX_BITS: u32 = 11;
const RADIX: usize = 1 << RADIX_BITS;
const RADIX_MASK: u32 = (RADIX as u32) - 1;

/// 低于该音符数时用标准库稳定排序。
///
/// 依据：262k 未排序音符时基数排序仍快 3×，但真实素材的小轨多为「已排序 /
/// 长 run」，标准库自适应后远快于基数排序；1M（14MB）以上才稳定进入
/// 「比较排序被缓存未命中支配」的区间。
pub const RADIX_MIN_NOTES: usize = 1_000_000;

/// 按 `start_tick` 稳定升序排序（与 `notes.sort_by_key(|n| n.start_tick)` 等价）。
pub fn sort_by_start_tick(notes: &mut Vec<PackedNote>) {
    sort_with_threshold(notes, RADIX_MIN_NOTES);
}

/// 一次线性扫描：是否已按 start_tick 升序 + 最大 start_tick。
fn scan_start_ticks(notes: &[PackedNote]) -> (bool, u32) {
    let mut sorted = true;
    let mut max = 0u32;
    let mut prev = 0u32;
    for (i, note) in notes.iter().enumerate() {
        let tick = note.start_tick;
        if i > 0 && tick < prev {
            sorted = false;
        }
        prev = tick;
        max = max.max(tick);
    }
    (sorted, max)
}

/// 稳定 LSD 基数排序；趟数按 `max_tick` 实际位宽自适应。
///
/// `notes` 长度必须 ≥ 1，且 `max_tick` 必须是真实最大值（由调用方扫描保证）。
fn radix_sort(notes: &mut Vec<PackedNote>, max_tick: u32) {
    let n = notes.len();
    let bits = 32 - max_tick.leading_zeros() as usize;
    let passes = bits.div_ceil(RADIX_BITS as usize);
    if passes == 0 {
        // 所有 start_tick 都是 0：已有序。
        return;
    }
    let zero = PackedNote {
        start_tick: 0,
        end_tick: 0,
        key: 0,
        velocity: 0,
        release_velocity: 0,
        channel: 0,
        track: 0,
    };
    let mut scratch = vec![zero; n];
    let mut count = [0usize; RADIX];
    let mut data = std::mem::take(notes);
    for pass in 0..passes {
        let shift = pass as u32 * RADIX_BITS;
        count.fill(0);
        for note in &data {
            count[((note.start_tick >> shift) & RADIX_MASK) as usize] += 1;
        }
        let mut sum = 0usize;
        for slot in count.iter_mut() {
            let n = *slot;
            *slot = sum;
            sum += n;
        }
        for note in &data {
            let digit = ((note.start_tick >> shift) & RADIX_MASK) as usize;
            scratch[count[digit]] = *note;
            count[digit] += 1;
        }
        std::mem::swap(&mut data, &mut scratch);
    }
    *notes = data;
}

/// 可注入阈值的内核（单测用小阈值覆盖基数路径）。
fn sort_with_threshold(notes: &mut Vec<PackedNote>, threshold: usize) {
    if notes.len() < threshold {
        notes.sort_by_key(|n| n.start_tick);
        return;
    }
    let (sorted, max_tick) = scan_start_ticks(notes);
    if sorted {
        return;
    }
    radix_sort(notes, max_tick);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 确定性伪随机音符（LCG），start_tick 有重复以覆盖稳定性。
    fn notes_with_dups(n: usize, tick_span: u32) -> Vec<PackedNote> {
        let mut out = Vec::with_capacity(n);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for i in 0..n {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let start = ((state >> 33) % tick_span as u64) as u32;
            out.push(PackedNote {
                start_tick: start,
                end_tick: start + 1 + (i % 60) as u32,
                key: (i % 128) as u8,
                velocity: 90,
                release_velocity: 0,
                channel: (i % 16) as u8,
                track: 0,
            });
        }
        out
    }

    fn reference(notes: &[PackedNote]) -> Vec<PackedNote> {
        let mut v = notes.to_vec();
        v.sort_by_key(|n| n.start_tick);
        v
    }

    /// 基数路径必须与标准库稳定排序逐元素一致（含重复 tick 的稳定顺序）。
    #[test]
    fn radix_matches_stable_sort_exactly() {
        let pristine = notes_with_dups(5000, 64);
        let expected = reference(&pristine);
        let mut got = pristine;
        sort_with_threshold(&mut got, 1024);
        assert_eq!(got, expected);
    }

    /// 已排序的大输入应被扫描短路（输出不变）。
    #[test]
    fn sorted_input_is_short_circuited() {
        let mut notes = notes_with_dups(4096, 4096);
        notes.sort_by_key(|n| n.start_tick);
        let before = notes.clone();
        sort_with_threshold(&mut notes, 1024);
        assert_eq!(notes, before);
    }

    /// 小输入走标准库路径，结果与参考一致。
    #[test]
    fn small_input_matches_reference() {
        let pristine = notes_with_dups(512, 32);
        let expected = reference(&pristine);
        let mut got = pristine;
        sort_with_threshold(&mut got, RADIX_MIN_NOTES);
        assert_eq!(got, expected);
    }

    /// 阈值边界（n == threshold）必须走基数路径且正确。
    #[test]
    fn threshold_boundary_uses_radix() {
        let pristine = notes_with_dups(2048, 16);
        let expected = reference(&pristine);
        let mut got = pristine;
        sort_with_threshold(&mut got, 2048);
        assert_eq!(got, expected);
    }

    /// 全部 start_tick 相同：不得 panic，顺序保持输入序（稳定）。
    #[test]
    fn all_same_tick_keeps_input_order() {
        let mut notes = notes_with_dups(3000, 1);
        let before = notes.clone();
        sort_with_threshold(&mut notes, 1024);
        assert_eq!(notes, before);
    }
}
