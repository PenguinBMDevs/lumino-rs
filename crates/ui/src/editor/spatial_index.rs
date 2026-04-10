use crate::editor::Note;
use std::sync::Arc;

/// 音符的空间索引引用
#[derive(Debug, Clone)]
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
    root: Option<Arc<Node>>,
}

#[derive(Debug, Clone)]
struct Node {
    tick_min: f32,
    tick_max: f32,
    /// 落在此 Tick 区间内的音符，按 Key 排序
    key_sorted: Vec<NoteRef>,
    left: Option<Arc<Node>>,
    right: Option<Arc<Node>>,
}

impl NoteSpatialIndex {
    /// 每个叶子节点的最大音符数阈值
    const MAX_LEAF_CAPACITY: usize = 128;

    pub fn new() -> Self {
        Self { root: None }
    }

    /// 从音符集合构建空间索引
    pub fn from_notes(notes: &[Note]) -> Self {
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

        // 初始时按照 tick 排序，以便进行中位数分割
        note_refs.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let root = Self::build_node(note_refs);
        Self {
            root: Some(Arc::new(root)),
        }
    }

    fn build_node(mut note_refs: Vec<NoteRef>) -> Node {
        if note_refs.is_empty() {
            return Node {
                tick_min: 0.0,
                tick_max: 0.0,
                key_sorted: Vec::new(),
                left: None,
                right: None,
            };
        }

        let tick_min = note_refs
            .first()
            .map(|n| n.tick)
            .unwrap_or(0.0);
        // 考虑 length，因为查询时需要判断音符尾部是否可见
        let tick_max = note_refs
            .iter()
            .map(|n| n.tick + n.length)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(tick_min);

        // 如果音符数量少于阈值，停止分割，成为叶子节点
        if note_refs.len() <= Self::MAX_LEAF_CAPACITY {
            note_refs.sort_by_key(|n| n.key);
            return Node {
                tick_min,
                tick_max,
                key_sorted: note_refs,
                left: None,
                right: None,
            };
        }

        // 按照中位数分割
        let mid = note_refs.len() / 2;
        let right_half = note_refs.split_off(mid);
        let left_half = note_refs;

        let left_node = Self::build_node(left_half);
        let right_node = Self::build_node(right_half);

        Node {
            tick_min,
            tick_max,
            key_sorted: Vec::new(), // 非叶子节点不存储具体音符，只做路由
            left: Some(Arc::new(left_node)),
            right: Some(Arc::new(right_node)),
        }
    }

    /// 查询在指定视口内的音符索引
    pub fn query(
        &self,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Vec<usize> {
        let mut result = Vec::new();
        if let Some(root) = &self.root {
            Self::query_node(
                root,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                &mut result,
            );
        }
        result
    }

    fn query_node(
        node: &Node,
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
        result: &mut Vec<usize>,
    ) {
        // Tick 边界检测，如果没有交集则直接剪枝返回
        if node.tick_max < tick_start || node.tick_min > tick_end {
            return;
        }

        // 如果是叶子节点，处理其中的音符
        if !node.key_sorted.is_empty() {
            // 在 key_sorted 中通过二分查找快速定位可见的 key 范围
            let start_idx = node.key_sorted.partition_point(|n| n.key < key_min);
            let end_idx = node.key_sorted.partition_point(|n| n.key <= key_max);

            for n in &node.key_sorted[start_idx..end_idx] {
                // 精确的 tick 交叉检测（因为只靠节点级 AABB 可能稍微偏大）
                if n.tick + n.length >= tick_start && n.tick <= tick_end {
                    result.push(n.index);
                }
            }
        }

        // 继续递归左右子树
        if let Some(left) = &node.left {
            Self::query_node(left, tick_start, tick_end, key_min, key_max, result);
        }
        if let Some(right) = &node.right {
            Self::query_node(right, tick_start, tick_end, key_min, key_max, result);
        }
    }
}
