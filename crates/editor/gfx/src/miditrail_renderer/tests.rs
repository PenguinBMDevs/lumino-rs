//! Miditrail 渲染器单元测试子模块入口

mod basic;
mod cull_equivalence;
mod driven_equivalence;
mod preview;

use crate::NoteInstance;

/// 与 `bucket_cull.wgsl`（`paint_order=1`）+ `prefix_counts_layered` 同序的窗口参考：
/// 白键块→黑键块；块内 key 升序；键内 [未来（start > tick）按 start 降序、
/// 同 start 保持升序 run] ++ [已开始（start ≤ tick）升序稳定]。
///
/// 该顺序即 driven 音符的绘制序（`depth_write=false`，顺序即最终次序），
/// 与 legacy `build_note_instances` 的 `(is_black, z_start, key)` 稳定排序在所有
/// 可见重叠（同键叠音、黑白键交叠）上逐像素等价。
pub(crate) fn paint_order_window(
    notes: &[NoteInstance],
    tick: u32,
    tick_end: u32,
    key_count: usize,
) -> Vec<NoteInstance> {
    let in_window = |n: &NoteInstance| {
        let start = n.start_length[0].max(0.0) as u32;
        let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
        end > tick && start < tick_end
    };
    let start_of = |n: &NoteInstance| n.start_length[0].max(0.0) as u32;
    let mut out = Vec::new();
    for layer in 0..2u32 {
        for key in 0..key_count.min(128) as u32 {
            let is_black = crate::is_black_key(key as isize);
            if (layer == 0) == is_black {
                continue;
            }
            let mut key_sorted: Vec<NoteInstance> = notes
                .iter()
                .copied()
                .filter(|n| (n.key_color & 0xFF) == key && in_window(n))
                .collect();
            // 常驻桶序：key 内按 start 升序稳定（`bucket_sort.wgsl` (key,start)）。
            key_sorted.sort_by_key(|n| start_of(n));
            let split = key_sorted.partition_point(|n| start_of(n) <= tick);
            // 未来段：start 降序、同 start 保持升序 run（稳定）。
            let future = &key_sorted[split..];
            let mut i = future.len();
            while i > 0 {
                let s = start_of(&future[i - 1]);
                let mut r0 = i - 1;
                while r0 > 0 && start_of(&future[r0 - 1]) == s {
                    r0 -= 1;
                }
                out.extend_from_slice(&future[r0..i]);
                i = r0;
            }
            // 已开始段：升序稳定。
            out.extend_from_slice(&key_sorted[..split]);
        }
    }
    out
}
