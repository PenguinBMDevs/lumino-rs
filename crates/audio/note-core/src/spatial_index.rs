//! 基于 Tick 轴二分分割和 Key 排序的变体二叉树空间索引
//!
//! 用于在二维的钢琴卷帘中快速筛选出可见的音符。

use crate::note::Note;

/// 音符的空间索引引用
#[derive(Debug, Clone, Copy)]
pub struct NoteRef {
    /// 音符起始 tick。
    pub tick: f32,
    /// 音高（MIDI 音高数字）。
    pub key: u16,
    /// 音符时长（tick）。
    pub length: f32,
    /// 在源音符集合中的索引。
    pub index: usize,
}

/// 基于 Tick 轴二分分割和 Key 排序的变体二叉树
/// 用于在二维的钢琴卷帘中快速筛选出可见的音符
#[derive(Debug, Clone)]
pub struct NoteSpatialIndex {
    nodes: Vec<Node>,
    root: Option<usize>,
}

#[derive(Debug, Clone)]
struct Node {
    tick_min: f32,
    tick_max: f32,
    /// 落在此 Tick 区间内的音符，按 Key 排序
    key_sorted: Vec<NoteRef>,
    left: Option<usize>,
    right: Option<usize>,
}

impl Default for NoteSpatialIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl NoteSpatialIndex {
    /// 每个叶子节点的最大音符数阈值
    const MAX_LEAF_CAPACITY: usize = 128;

    /// 创建一个空的音符空间索引。
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
        }
    }

    /// 从音符集合构建空间索引
    pub fn from_notes(notes: &[Note]) -> Self {
        puffin::profile_function!();
        let mut note_refs: Vec<NoteRef> = notes
            .iter()
            .enumerate()
            .map(|(index, note)| NoteRef {
                tick: note.tick,
                key: note.key,
                length: note.length,
                index,
            })
            .collect();

        note_refs.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self::from_sorted_note_refs(note_refs)
    }

    /// 从 `NoteRef` 切片构建空间索引（自动排序）
    pub fn from_note_refs(note_refs: &[NoteRef]) -> Self {
        puffin::profile_function!();
        if note_refs.is_empty() {
            return Self::new();
        }
        let mut sorted = note_refs.to_vec();
        sorted.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self::from_sorted_note_refs(sorted)
    }

    /// 从已排序的 `Vec<NoteRef>` 构建空间索引（内部方法，避免重复排序）
    fn from_sorted_note_refs(note_refs: Vec<NoteRef>) -> Self {
        if note_refs.is_empty() {
            return Self::new();
        }
        let mut nodes = Vec::new();
        let root = Self::build_node(note_refs, &mut nodes);
        Self {
            nodes,
            root: Some(root),
        }
    }

    /// 从原始 (tick, key, length) 数据构建空间索引（不需要 Note 数组）
    pub fn from_raw_notes(raw: &[(f32, u16, f32)]) -> Self {
        puffin::profile_function!();
        if raw.is_empty() {
            return Self::new();
        }

        let mut note_refs: Vec<NoteRef> = raw
            .iter()
            .map(|&(tick, key, length)| NoteRef {
                tick,
                key,
                length,
                index: 0,
            })
            .collect();

        note_refs.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self::from_sorted_note_refs(note_refs)
    }

    fn build_node(mut note_refs: Vec<NoteRef>, nodes: &mut Vec<Node>) -> usize {
        puffin::profile_function!();
        if note_refs.is_empty() {
            let idx = nodes.len();
            nodes.push(Node {
                tick_min: 0.0,
                tick_max: 0.0,
                key_sorted: Vec::new(),
                left: None,
                right: None,
            });
            return idx;
        }

        let tick_min = note_refs
            .first()
            .map(|note_ref| note_ref.tick)
            .unwrap_or(0.0);
        let tick_max = note_refs
            .iter()
            .map(|note_ref| note_ref.tick + note_ref.length)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(tick_min);

        if note_refs.len() <= Self::MAX_LEAF_CAPACITY {
            note_refs.sort_by_key(|note_ref| note_ref.key);
            let idx = nodes.len();
            nodes.push(Node {
                tick_min,
                tick_max,
                key_sorted: note_refs,
                left: None,
                right: None,
            });
            return idx;
        }

        let mid = note_refs.len() / 2;
        let right_half = note_refs.split_off(mid);
        let left_half = note_refs;

        let idx = nodes.len();
        nodes.push(Node {
            tick_min,
            tick_max,
            key_sorted: Vec::new(),
            left: None,
            right: None,
        });

        let left_node = Self::build_node(left_half, nodes);
        let right_node = Self::build_node(right_half, nodes);

        nodes[idx].left = Some(left_node);
        nodes[idx].right = Some(right_node);

        idx
    }

    /// 查询在指定视口内的音符索引（tick 轴**闭**区间 `[start, end]`）
    ///
    /// ⚠️ **闭区间是有意为之，禁止改为半开**：
    /// - `playback.rs` 以 `update_query(tick, tick, ..)`（零宽区间）取播放头**活跃**
    ///   音符，起点恰在播放头的音符必须命中；
    /// - `rendering/visible_notes.rs` 以视口边界做裁剪，闭区间避免边界音符漏渲染。
    ///
    /// 框选命中语义为半开（边界相触不算选中），请使用
    /// [`Self::update_query_marquee`]，不要改动本方法。
    pub fn update_query(
        &self,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
        result: &mut Vec<usize>,
    ) {
        puffin::profile_function!();
        result.clear();
        if let Some(root_idx) = self.root {
            self.query_node_iter::<false>(
                root_idx,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                result,
            );
        }
    }

    /// 框选（marquee）专用查询：tick 轴**半开**区间 `[tick_start, tick_end)`，
    /// key 轴闭区间 `[key_min, key_max]`。
    ///
    /// 半开语义 = 「音符与选框**重叠**才算命中；仅边界相触不算」：
    /// - 起点恰等于框右边界的音符**不**入选（低精度模式下防框缘误选）；
    /// - 终点恰等于框左边界的音符**不**入选；
    /// - 跨越框边界、内部有交叠的长音符**仍然**入选。
    ///
    /// ⚠️ 与 [`Self::update_query`] 语义不同，**禁止合并或互相替换**：
    /// 后者供视口裁剪/播放活跃判定使用，必须保持闭区间。
    /// key 轴保持闭区间：`key` 是离散格，闭区间为「整格覆盖」语义，且
    /// 框选增量差集（`rect_subtract`）的 key 代数依赖闭区间。
    ///
    /// 结果顺序与 [`Self::update_query`] 一致（不保证有序，调用方自行处理）。
    pub fn update_query_marquee(
        &self,
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
        result: &mut Vec<usize>,
    ) {
        puffin::profile_function!();
        result.clear();
        if let Some(root_idx) = self.root {
            self.query_node_iter::<true>(root_idx, tick_start, tick_end, key_min, key_max, result);
        }
    }

    /// 直接从空间索引节点数据中收集视口内音符的 (tick, key, length)
    ///
    /// tick 轴闭区间（视口语义，同 [`Self::update_query`]）。
    pub fn collect_instances_in_range(
        &self,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
        result: &mut Vec<(f32, u16, f32)>,
    ) {
        puffin::profile_function!();
        result.clear();
        if let Some(root_idx) = self.root {
            self.query_node_iter_direct::<false>(
                root_idx,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                result,
            );
        }
    }

    /// 遍历骨架（索引输出）。
    ///
    /// `HALF_OPEN` 为编译期常量，两种口径各自单态化，**热路径无额外分支**：
    /// - `false`：tick 轴闭区间（视口/播放）
    /// - `true`：tick 轴半开区间（框选）
    fn query_node_iter<const HALF_OPEN: bool>(
        &self,
        root_idx: usize,
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
        result: &mut Vec<usize>,
    ) {
        puffin::profile_function!();
        let mut stack = Vec::with_capacity(32);
        stack.push(root_idx);

        while let Some(node_idx) = stack.pop() {
            let node = &self.nodes[node_idx];
            if !node_tick_may_hit::<HALF_OPEN>(node, tick_start, tick_end) {
                continue;
            }

            if !node.key_sorted.is_empty() {
                let start_idx = node
                    .key_sorted
                    .partition_point(|note_ref| note_ref.key < key_min);
                let end_idx = node
                    .key_sorted
                    .partition_point(|note_ref| note_ref.key <= key_max);

                for note_ref in &node.key_sorted[start_idx..end_idx] {
                    if note_tick_hits::<HALF_OPEN>(note_ref, tick_start, tick_end) {
                        result.push(note_ref.index);
                    }
                }
            }

            if let Some(left) = node.left {
                stack.push(left);
            }
            if let Some(right) = node.right {
                stack.push(right);
            }
        }
    }

    /// 遍历骨架（原始 (tick, key, length) 输出）。口径同 [`Self::query_node_iter`]。
    fn query_node_iter_direct<const HALF_OPEN: bool>(
        &self,
        root_idx: usize,
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
        result: &mut Vec<(f32, u16, f32)>,
    ) {
        puffin::profile_function!();
        let mut stack = Vec::with_capacity(32);
        stack.push(root_idx);

        while let Some(node_idx) = stack.pop() {
            let node = &self.nodes[node_idx];
            if !node_tick_may_hit::<HALF_OPEN>(node, tick_start, tick_end) {
                continue;
            }

            if !node.key_sorted.is_empty() {
                let start_idx = node
                    .key_sorted
                    .partition_point(|note_ref| note_ref.key < key_min);
                let end_idx = node
                    .key_sorted
                    .partition_point(|note_ref| note_ref.key <= key_max);

                for note_ref in &node.key_sorted[start_idx..end_idx] {
                    if note_tick_hits::<HALF_OPEN>(note_ref, tick_start, tick_end) {
                        result.push((note_ref.tick, note_ref.key, note_ref.length));
                    }
                }
            }

            if let Some(left) = node.left {
                stack.push(left);
            }
            if let Some(right) = node.right {
                stack.push(right);
            }
        }
    }
}

/// 节点级剪枝：`false` = tick 轴闭区间，`true` = 半开区间。
///
/// 剪枝必须与叶子判定**同口径且保守**（不得剪掉可能命中的节点）：
/// - 闭区间：`tick_max < start || tick_min > end` → 全部失败
/// - 半开区间：`tick_max <= start || tick_min >= end` → 全部失败
#[inline(always)]
fn node_tick_may_hit<const HALF_OPEN: bool>(node: &Node, tick_start: f32, tick_end: f32) -> bool {
    if HALF_OPEN {
        !(node.tick_max <= tick_start || node.tick_min >= tick_end)
    } else {
        !(node.tick_max < tick_start || node.tick_min > tick_end)
    }
}

/// 叶子级判定：音符与 tick 区间是否重叠。
///
/// 闭区间 = 「边界相触也算命中」（视口/播放活跃判定）；
/// 半开区间 = 「仅边界相触不算命中」（框选，防框缘误选）。
#[inline(always)]
fn note_tick_hits<const HALF_OPEN: bool>(note: &NoteRef, tick_start: f32, tick_end: f32) -> bool {
    if HALF_OPEN {
        note.tick + note.length > tick_start && note.tick < tick_end
    } else {
        note.tick + note.length >= tick_start && note.tick <= tick_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use std::time::Instant;

    #[test]
    fn test_spatial_index_performance() {
        puffin::set_scopes_on(true);

        let mut notes = Vec::new();
        let num_notes = 100_000;
        for i in 0..num_notes {
            notes.push(Note {
                tick: (i % 10000) as f32 * 10.0,
                key: (i % 128) as u16,
                length: 20.0,
                velocity: 100,
                channel: 0,
            });
        }

        let start = Instant::now();
        let index = NoteSpatialIndex::from_notes(&notes);
        println!("Build tree took: {:?}", start.elapsed());

        let mut result = Vec::new();
        let start = Instant::now();
        for i in 0..1000 {
            let tick_start = (i % 1000) as f32 * 50.0;
            let tick_end = tick_start + 1000.0;
            index.update_query(tick_start, tick_end, 40, 80, &mut result);
        }
        println!("1000 queries took: {:?}", start.elapsed());
        assert!(!result.is_empty());
    }

    /// 构造三条**首尾相接**的边界音符（用于验证两种 tick 口径的差异）：
    /// - C = `[0, 100)`
    /// - A = `[100, 200)`
    /// - B = `[200, 300)`
    fn boundary_notes() -> Vec<Note> {
        vec![
            Note {
                tick: 0.0,
                key: 60,
                length: 100.0,
                velocity: 100,
                channel: 0,
            },
            Note {
                tick: 100.0,
                key: 60,
                length: 100.0,
                velocity: 100,
                channel: 0,
            },
            Note {
                tick: 200.0,
                key: 60,
                length: 100.0,
                velocity: 100,
                channel: 0,
            },
        ]
    }

    fn sorted(mut v: Vec<usize>) -> Vec<usize> {
        v.sort_unstable();
        v
    }

    /// 闭区间 `[100, 200]`：三条音符的首尾边界全部与之相触 → 全命中。
    ///
    /// 这是视口裁剪/播放头活跃判定依赖的语义（守卫：不得被框选半开化改动污染）。
    #[test]
    fn test_update_query_closed_interval_keeps_touching_notes() {
        let index = NoteSpatialIndex::from_notes(&boundary_notes());
        let mut result = Vec::new();
        index.update_query(100.0, 200.0, 0, 127, &mut result);
        assert_eq!(
            sorted(result),
            vec![0, 1, 2],
            "闭区间下：终点恰等于左边界(C)、起点恰等于右边界(B) 都应命中"
        );
    }

    /// 播放活跃判定守卫：`update_query(tick, tick, ..)` 零宽闭区间必须命中
    /// 「起点恰在播放头」与「终点恰在播放头」的音符。
    ///
    /// 回归背景：`impls/playback.rs` 以 `update_query(tick, tick, 0, 255, ..)`
    /// 取播放头活跃音符，随后自行以 `end_tick > tick` 收口。若把本方法半开化，
    /// 起点恰在播放头的音符会漏取 → 起播瞬间第一个音符不高亮。
    #[test]
    fn test_update_query_zero_width_still_hits_playhead_notes() {
        let index = NoteSpatialIndex::from_notes(&boundary_notes());
        let mut result = Vec::new();
        index.update_query(100.0, 100.0, 0, 127, &mut result);
        assert_eq!(
            sorted(result.clone()),
            vec![0, 1],
            "零宽闭区间：终点=100(C) 与 起点=100(A) 均须命中（播放头活跃判定依赖）"
        );

        // 半开口径下零宽区间必然为空（» 用于对照，说明两者不可互换）
        index.update_query_marquee(100.0, 100.0, 0, 127, &mut result);
        assert!(
            result.is_empty(),
            "半开零宽区间为空——框选语义与播放活跃判定语义不同，禁止合并"
        );
    }

    /// 半开区间 `[100, 200)`：仅边界相触的 C、B 被排除，内部交叠的 A 保留。
    ///
    /// 这是框选（marquee）依赖的语义：低精度模式下框缘贴边音符不被误选。
    #[test]
    fn test_update_query_marquee_half_open_excludes_touching_notes() {
        let index = NoteSpatialIndex::from_notes(&boundary_notes());
        let mut result = Vec::new();
        index.update_query_marquee(100.0, 200.0, 0, 127, &mut result);
        assert_eq!(
            sorted(result),
            vec![1],
            "半开区间下：仅内部有交叠的 A[100,200) 入选；\
             C 终点=100 与 B 起点=200 仅边界相触，必须排除"
        );
    }

    /// 跨越框边界的长音符（内部有交叠）仍须入选——半开不等于「包含」。
    #[test]
    fn test_update_query_marquee_keeps_straddling_long_note() {
        let notes = vec![Note {
            tick: 0.0,
            key: 60,
            length: 1000.0,
            velocity: 100,
            channel: 0,
        }];
        let index = NoteSpatialIndex::from_notes(&notes);
        let mut result = Vec::new();
        index.update_query_marquee(500.0, 600.0, 0, 127, &mut result);
        assert_eq!(
            sorted(result),
            vec![0],
            "长音符 [0,1000) 跨越 [500,600) 且内部交叠，必须入选"
        );
    }

    /// key 轴保持闭区间（整格覆盖语义）：key 上下边界都必须命中。
    #[test]
    fn test_update_query_marquee_key_axis_stays_closed() {
        let notes: Vec<Note> = (60..=62)
            .map(|key| Note {
                tick: 0.0,
                key,
                length: 100.0,
                velocity: 100,
                channel: 0,
            })
            .collect();
        let index = NoteSpatialIndex::from_notes(&notes);
        let mut result = Vec::new();
        index.update_query_marquee(0.0, 100.0, 60, 62, &mut result);
        assert_eq!(
            sorted(result.clone()),
            vec![0, 1, 2],
            "key 轴闭区间：上下边界 key 均须命中"
        );

        index.update_query_marquee(0.0, 100.0, 61, 61, &mut result);
        assert_eq!(sorted(result), vec![1], "单个 key 只命中该 key");
    }

    /// 多节点树（> `MAX_LEAF_CAPACITY`）下，半开查询必须与暴力过滤完全一致。
    ///
    /// 覆盖节点级剪枝（`node_tick_may_hit`）与叶子判定的同口径一致性：
    /// 剪枝不能剪掉可能命中的节点，否则会出现「树大小影响结果」的口径分裂。
    #[test]
    fn test_update_query_marquee_matches_brute_force_on_multi_node_tree() {
        // 400 条音符：长度 1..=8，tick 密集重叠，必然触发多级节点切分
        let notes: Vec<Note> = (0..400u32)
            .map(|i| Note {
                tick: (i % 97) as f32 * 10.0,
                key: (i % 128) as u16,
                length: ((i % 8) + 1) as f32 * 10.0,
                velocity: 100,
                channel: 0,
            })
            .collect();
        let index = NoteSpatialIndex::from_notes(&notes);
        assert!(notes.len() > NoteSpatialIndex::MAX_LEAF_CAPACITY);

        let mut result = Vec::new();
        // 遍历多条查询窗，含与音符边界精确对齐的场景（步长 10 = tick 粒度）
        for t0 in (0..960).step_by(10) {
            for span in [0.0_f32, 5.0, 10.0, 100.0] {
                let t1 = t0 as f32 + span;
                index.update_query_marquee(t0 as f32, t1, 0, 127, &mut result);
                let got = sorted(result.clone());
                let expected: Vec<usize> = notes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.tick + n.length > t0 as f32 && n.tick < t1)
                    .map(|(i, _)| i)
                    .collect();
                assert_eq!(
                    got, expected,
                    "半开查询 [{t0}, {t1}) 与暴力过滤不一致（树剪枝口径错误）"
                );
            }
        }
    }
}
