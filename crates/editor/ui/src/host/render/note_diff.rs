//! 主音轨可见列表 diff 增量（纯函数，可单测）
//!
//! GPU 布局 = 当前可见音符列表（内容 + 顺序，位置 = 列表下标）。当数据变化
//! 无法用事件队列精确描述（切轨 / 增删音符 / undo/redo / 散改 / 事件含不可见
//! 索引）时，调用方收集新可见列表，本模块与「上次 GPU 布局镜像」做前缀/后缀
//! 对齐 diff：
//!
//! - 全等 → [`DiffResult::Noop`]（**零上传**：切轨到相同内容/空轨不再全量重传）
//! - 中间段替换 → [`DiffResult::Segments`]（UpdateMany + RemoveAt / Insert 组合，
//!   只传输差异段；后缀搬移由 GPU 内部 copy 完成，远便宜于 PCIe 全量上传）
//! - 差异过大 → [`DiffResult::Full`]（一次 Reset 全量写比多段搬移 + 写更便宜）
//!
//! 事件应用顺序（渲染线程 `process_events` 顺序消费）：
//! `updates` → `removes` → `inserts`。updates 位置基于旧布局；removes 后布局
//! 左移；inserts 位置基于 removes 后的布局（即新布局中插入点）。

/// 可见音符三元组 `(tick, key, length)`（与全量收集 buffer 元素类型一致）
pub(crate) type VisibleNote = (f32, u16, f32);

/// 覆盖写/插入段：`(起始位置, 新内容)`
pub(crate) type NoteSegment = (usize, Vec<VisibleNote>);

/// diff 输出：GPU 段级增量（按序应用：updates → removes → inserts）
#[derive(Debug, Default, PartialEq)]
pub(crate) struct VisibleDiff {
    /// 覆盖写段：`(start_index, 新内容)`——与旧布局同位置覆盖
    pub updates: Vec<NoteSegment>,
    /// 保序删除段：`(index, count)`——删除后后续段左移
    pub removes: Vec<(usize, usize)>,
    /// 保序插入段：`(index, 新内容)`——插入后后续段右移
    pub inserts: Vec<NoteSegment>,
}

/// diff 决策结果
#[derive(Debug, PartialEq)]
pub(crate) enum DiffResult {
    /// 新旧完全一致——无需任何上传
    Noop,
    /// 差异过大，一次全量写更优（调用方发 `NoteEvent::Reset`）
    Full,
    /// 段级增量（调用方按序发 UpdateMany / RemoveAt / Insert）
    Segments(VisibleDiff),
}

/// 对「上次 GPU 布局镜像」与「新可见列表」做前缀/后缀对齐 diff
///
/// 两个输入都必须按渲染顺序排列（可见音符收集结果，天然升序）。
/// 相等判定为三元组 `(tick, key, length)` 逐项相等（同一 document 数据
/// 转换而来，无计算误差；ghost 位置由 ghost 增量路径单独处理，不经过 diff）。
pub(crate) fn diff_visible(old: &[VisibleNote], new: &[VisibleNote]) -> DiffResult {
    let old_len = old.len();
    let new_len = new.len();

    // 前缀：从头找最大相等长度
    let prefix = old
        .iter()
        .zip(new.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // 后缀：从尾找最大相等长度（不越过前缀，保证 prefix + suffix <= min 长度）
    let mut suffix = 0usize;
    while suffix < old_len - prefix
        && suffix < new_len - prefix
        && old[old_len - 1 - suffix] == new[new_len - 1 - suffix]
    {
        suffix += 1;
    }

    let mid_old = old_len - prefix - suffix;
    let mid_new = new_len - prefix - suffix;
    if mid_old == 0 && mid_new == 0 {
        return DiffResult::Noop;
    }

    // 成本估算（启发式，单位 = 全量上传 1 个实例的成本）：
    // - segments：写 mid_new 个实例 + 有删/插时后缀搬移（GPU 内部 copy，
    //   双向各 1 次，按 1/4 折算为 PCIe 上传成本）
    // - full：一次写 new_len 个实例
    // 成本相等时选 segments：等长整段覆盖（UpdateMany）与 Reset 传输量相同，
    // 但保持增量语义（GPU 侧只写部分 buffer，不重建 CPU 镜像）。
    let has_shift = mid_old != mid_new;
    let shift_cost = if has_shift { suffix / 2 } else { 0 };
    let seg_cost = mid_new + shift_cost;
    let full_cost = new_len;

    if seg_cost <= full_cost {
        let mut diff = VisibleDiff::default();
        let min_mid = mid_old.min(mid_new);
        if min_mid > 0 {
            diff.updates
                .push((prefix, new[prefix..prefix + min_mid].to_vec()));
        }
        if mid_old > mid_new {
            diff.removes.push((prefix + min_mid, mid_old - mid_new));
        } else if mid_new > mid_old {
            diff.inserts.push((
                prefix + min_mid,
                new[prefix + min_mid..prefix + mid_new].to_vec(),
            ));
        }
        DiffResult::Segments(diff)
    } else {
        DiffResult::Full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(tick: f32, key: u16, len: f32) -> (f32, u16, f32) {
        (tick, key, len)
    }

    fn notes(ticks: &[f32]) -> Vec<(f32, u16, f32)> {
        ticks
            .iter()
            .enumerate()
            .map(|(i, &t)| n(t, i as u16, 1.0))
            .collect()
    }

    #[test]
    fn identical_lists_are_noop() {
        let old = notes(&[0.0, 10.0, 20.0]);
        let new = notes(&[0.0, 10.0, 20.0]);
        assert_eq!(diff_visible(&old, &new), DiffResult::Noop);
    }

    #[test]
    fn both_empty_is_noop() {
        assert_eq!(diff_visible(&[], &[]), DiffResult::Noop);
    }

    #[test]
    fn switch_to_same_content_track_is_noop() {
        // 切轨到内容完全相同的音轨（复制轨/空轨）：零上传
        let old = notes(&[0.0, 10.0, 20.0, 30.0, 40.0]);
        let new = notes(&[0.0, 10.0, 20.0, 30.0, 40.0]);
        assert_eq!(diff_visible(&old, &new), DiffResult::Noop);
    }

    #[test]
    fn switch_to_empty_track_is_remove_all() {
        // 切到空轨：RemoveAt 全删（零传输，不重建 buffer）
        let old = notes(&[0.0, 10.0, 20.0]);
        match diff_visible(&old, &[]) {
            DiffResult::Segments(d) => {
                assert_eq!(d.updates.len(), 0);
                assert_eq!(d.inserts.len(), 0);
                assert_eq!(d.removes, vec![(0, 3)]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn first_build_empty_to_full_is_insert_all() {
        // 首次构建（GPU 空）成本相等时选 segments：Insert 全段 = 一次段写
        let new = notes(&[0.0, 10.0, 20.0, 30.0]);
        match diff_visible(&[], &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.updates.len(), 0);
                assert_eq!(d.removes.len(), 0);
                assert_eq!(d.inserts, vec![(0, new.clone())]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn middle_update_only() {
        // 中间段等长替换：只更新中间 2 个
        let old = notes(&[0.0, 10.0, 20.0, 30.0, 40.0]);
        let new = vec![
            n(0.0, 0, 1.0),
            n(11.0, 1, 1.0),
            n(21.0, 2, 1.0),
            n(30.0, 3, 1.0),
            n(40.0, 4, 1.0),
        ];
        match diff_visible(&old, &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.removes, vec![]);
                assert_eq!(d.inserts, vec![]);
                assert_eq!(d.updates.len(), 1);
                assert_eq!(d.updates[0].0, 1);
                assert_eq!(d.updates[0].1, vec![n(11.0, 1, 1.0), n(21.0, 2, 1.0)]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn delete_middle_with_long_suffix() {
        // 删除中间段、后缀长：段级增量（搬移 GPU 内部 copy 便宜于全量上传）
        let old: Vec<_> = (0..1000).map(|i| n(i as f32, 0, 1.0)).collect();
        let new: Vec<_> = old
            .iter()
            .enumerate()
            .filter(|(i, _)| !(10..20).contains(i))
            .map(|(_, v)| *v)
            .collect();
        match diff_visible(&old, &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.updates.len(), 0);
                assert_eq!(d.inserts.len(), 0);
                assert_eq!(d.removes, vec![(10, 10)]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn insert_middle_with_long_suffix() {
        // 中间插入、后缀长：段级增量（Insert + GPU 右移）
        let old: Vec<_> = (0..1000).map(|i| n(i as f32, 0, 1.0)).collect();
        let mut new = old.clone();
        new.splice(10..10, vec![n(99.0, 0, 1.0), n(98.0, 0, 1.0)]);
        match diff_visible(&old, &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.updates.len(), 0);
                assert_eq!(d.removes.len(), 0);
                assert_eq!(d.inserts.len(), 1);
                assert_eq!(d.inserts[0].0, 10);
                assert_eq!(d.inserts[0].1, vec![n(99.0, 0, 1.0), n(98.0, 0, 1.0)]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn append_at_tail_is_segments() {
        // 末尾追加（尾部插入，无搬移）：增量写
        let old = notes(&[0.0, 10.0, 20.0]);
        let mut new = old.clone();
        new.push(n(30.0, 0, 1.0));
        match diff_visible(&old, &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.updates.len(), 0);
                assert_eq!(d.removes.len(), 0);
                assert_eq!(d.inserts, vec![(3, vec![n(30.0, 0, 1.0)])]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn replace_all_very_different_is_segments() {
        // 内容几乎全变（切轨到完全不同音轨）且无公共前后缀：
        // 成本与 Reset 相同（无搬移），选段级覆盖（等长覆盖 + 尾部插入）
        let old = notes(&[0.0, 10.0, 20.0, 30.0, 40.0]);
        let new = notes(&[1.0, 11.0, 21.0, 31.0, 41.0, 51.0]);
        match diff_visible(&old, &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.updates.len(), 1);
                assert_eq!(d.updates[0].0, 0);
                assert_eq!(d.updates[0].1, notes(&[1.0, 11.0, 21.0, 31.0, 41.0]));
                assert_eq!(d.removes.len(), 0);
                assert_eq!(d.inserts, vec![(5, vec![n(51.0, 5, 1.0)])]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn replace_half_keeps_segments() {
        // 一半相同（前缀 3 相同，尾部不同）：只覆盖中间段
        let old = notes(&[0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
        let mut new = old[..3].to_vec();
        new.extend(notes(&[101.0, 102.0, 103.0, 104.0]));
        match diff_visible(&old, &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.removes.len(), 0);
                assert_eq!(d.inserts.len(), 0);
                assert_eq!(d.updates.len(), 1);
                assert_eq!(d.updates[0].0, 3);
                assert_eq!(d.updates[0].1, notes(&[101.0, 102.0, 103.0, 104.0]));
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }

    #[test]
    fn reorder_all_is_update() {
        // 全部顺序变化（同元素重排）：等长整段覆盖
        let old = notes(&[0.0, 10.0, 20.0]);
        let new = vec![n(20.0, 0, 1.0), n(10.0, 0, 1.0), n(0.0, 0, 1.0)];
        match diff_visible(&old, &new) {
            DiffResult::Segments(d) => {
                assert_eq!(d.removes.len(), 0);
                assert_eq!(d.inserts.len(), 0);
                assert_eq!(d.updates, vec![(0, new.clone())]);
            }
            other => panic!("期望 Segments，实际 {other:?}"),
        }
    }
}
