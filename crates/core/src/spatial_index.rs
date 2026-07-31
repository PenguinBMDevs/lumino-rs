//! 基于 Tick 轴二分分割和 Key 排序的变体二叉树空间索引
//!
//! 用于在二维的钢琴卷帘中快速筛选出可见的音符。

use crate::note::Note;
use crate::note_store::NoteStore;

/// 音符的空间索引引用
#[derive(Debug, Clone, Copy)]
pub struct NoteRef {
    pub tick: f32,
    pub key: u16,
    pub length: f32,
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

    /// 从 NoteStore（SoA 布局）直接构建空间索引
    ///
    /// **性能优化**：16M 音符场景下，比 `from_notes(&[Note])` 节省 ~80ms
    /// 的 Note 结构体 clone 开销——直接遍历 SoA 数组构造 NoteRef。
    ///
    /// 调用方需保证 `store` 与 `notes` 一致（NoteStore 启用时）。
    pub fn from_note_store(store: &NoteStore) -> Self {
        puffin::profile_function!();
        if store.is_empty() {
            return Self::new();
        }

        let mut note_refs: Vec<NoteRef> = Vec::with_capacity(store.len());
        store.for_each_ref(|index, view| {
            note_refs.push(NoteRef {
                tick: view.tick,
                key: view.key,
                length: view.length,
                index,
            });
        });

        // 按 tick 排序后递归建树
        note_refs.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self::from_sorted_note_refs(note_refs)
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

    /// 查询在指定视口内的音符索引
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
            self.query_node_iter(
                root_idx,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                result,
            );
        }
    }

    /// 直接从空间索引节点数据中收集视口内音符的 (tick, key, length)
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
            self.query_node_iter_direct(
                root_idx,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                result,
            );
        }
    }

    fn query_node_iter(
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
            if node.tick_max < tick_start || node.tick_min > tick_end {
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
                    if note_ref.tick + note_ref.length >= tick_start && note_ref.tick <= tick_end {
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

    fn query_node_iter_direct(
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
            if node.tick_max < tick_start || node.tick_min > tick_end {
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
                    if note_ref.tick + note_ref.length >= tick_start && note_ref.tick <= tick_end {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use crate::note_store::NoteStore;
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

    /// 验证 from_note_store 与 from_notes 产生等价的查询结果
    #[test]
    fn test_from_note_store_equivalent_to_from_notes() {
        let mut notes = Vec::new();
        for i in 0..500 {
            notes.push(Note {
                tick: (i % 50) as f32 * 10.0,
                key: (i % 128) as u16,
                length: 20.0,
                velocity: 100,
                channel: 0,
            });
        }

        let store = NoteStore::from_im_vector(&im::Vector::from(notes.clone()));

        let idx_notes = NoteSpatialIndex::from_notes(&notes);
        let idx_store = NoteSpatialIndex::from_note_store(&store);

        // 同一查询条件下结果应一致
        let mut r1 = Vec::new();
        let mut r2 = Vec::new();
        idx_notes.update_query(0.0, 500.0, 40, 80, &mut r1);
        idx_store.update_query(0.0, 500.0, 40, 80, &mut r2);

        // 排序后比较（不同实现可能返回顺序不同）
        r1.sort_unstable();
        r2.sort_unstable();
        assert_eq!(r1, r2, "from_note_store 与 from_notes 查询结果应一致");
    }

    /// 验证空 NoteStore 不 panic
    #[test]
    fn test_from_note_store_empty() {
        let store = NoteStore::new();
        let idx = NoteSpatialIndex::from_note_store(&store);
        let mut results = Vec::new();
        idx.update_query(0.0, 100.0, 0, 127, &mut results);
        assert!(results.is_empty());
    }
}
