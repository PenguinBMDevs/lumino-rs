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
        self.query_into::<false, _>(
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
            result,
        );
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
        self.query_into::<true, _>(tick_start, tick_end, key_min, key_max, result);
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
        self.query_into::<false, _>(
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
            result,
        );
    }

    /// 查询入口：清空结果并把根节点交给遍历骨架。
    fn query_into<const HALF_OPEN: bool, S: HitSink>(
        &self,
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
        result: &mut S,
    ) {
        puffin::profile_function!();
        result.clear();
        if let Some(root_idx) = self.root {
            self.query_node_iter::<HALF_OPEN, S>(
                root_idx, tick_start, tick_end, key_min, key_max, result,
            );
        }
    }

    /// 遍历骨架：索引与原始三元组共用同一份区间原语。
    ///
    /// `HALF_OPEN` 为编译期常量，两种口径各自单态化，**热路径无额外分支**：
    /// - `false`：tick 轴闭区间（视口/播放）
    /// - `true`：tick 轴半开区间（框选）
    ///
    /// 输出形态由 `HitSink` 静态分派（`Vec<usize>` / `Vec<(f32, u16, f32)>`），
    /// 每命中一次仍是内联 push，无虚调用。
    fn query_node_iter<const HALF_OPEN: bool, S: HitSink>(
        &self,
        root_idx: usize,
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
        result: &mut S,
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
                        result.push_hit(note_ref);
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

/// 查询命中输出：索引（渲染/播放）或原始三元组（空间直取）。
trait HitSink {
    fn clear(&mut self);
    fn push_hit(&mut self, note_ref: &NoteRef);
}

impl HitSink for Vec<usize> {
    #[inline]
    fn clear(&mut self) {
        Vec::clear(self);
    }

    #[inline]
    fn push_hit(&mut self, note_ref: &NoteRef) {
        self.push(note_ref.index);
    }
}

impl HitSink for Vec<(f32, u16, f32)> {
    #[inline]
    fn clear(&mut self) {
        Vec::clear(self);
    }

    #[inline]
    fn push_hit(&mut self, note_ref: &NoteRef) {
        self.push((note_ref.tick, note_ref.key, note_ref.length));
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
mod tests;
