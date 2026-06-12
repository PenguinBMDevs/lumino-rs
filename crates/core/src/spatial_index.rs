//! 基于 Tick 轴二分分割和 Key 排序的变体二叉树空间索引
//!
//! 用于在二维的钢琴卷帘中快速筛选出可见的音符。

use crate::note::Note;

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
        if notes.is_empty() {
            return Self::new();
        }

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

        let mut nodes = Vec::new();
        let root = Self::build_node(note_refs, &mut nodes);
        Self {
            nodes,
            root: Some(root),
        }
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

        let tick_min = note_refs.first().map(|n| n.tick).unwrap_or(0.0);
        let tick_max = note_refs
            .iter()
            .map(|n| n.tick + n.length)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(tick_min);

        if note_refs.len() <= Self::MAX_LEAF_CAPACITY {
            note_refs.sort_by_key(|n| n.key);
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
                let start_idx = node.key_sorted.partition_point(|n| n.key < key_min);
                let end_idx = node.key_sorted.partition_point(|n| n.key <= key_max);

                for n in &node.key_sorted[start_idx..end_idx] {
                    if n.tick + n.length >= tick_start && n.tick <= tick_end {
                        result.push(n.index);
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
                let start_idx = node.key_sorted.partition_point(|n| n.key < key_min);
                let end_idx = node.key_sorted.partition_point(|n| n.key <= key_max);

                for n in &node.key_sorted[start_idx..end_idx] {
                    if n.tick + n.length >= tick_start && n.tick <= tick_end {
                        result.push((n.tick, n.key, n.length));
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
}
