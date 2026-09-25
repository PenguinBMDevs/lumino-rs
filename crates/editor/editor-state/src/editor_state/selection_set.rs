//! 选中索引集合：稠密整数索引专用恒等哈希
//!
//! `SelectionSet` 的键是**当前轨音符索引**（`0..轨道音符数`，局部生成的稠密整数）。
//! 标准 SipHash/FxHash 会把顺序索引打散到随机桶——百万级全选插入 = 对 ~30MB
//! 哈希表做随机写，实测 ~80-110ms；本哈希直接以索引值为哈希：
//!
//! - 表容量（2 的幂，≈ 选中数 / 0.875）通常 ≥ 轨道索引 → **索引与桶一一对应（零碰撞）**；
//! - 顺序插入即**顺序写桶**（缓存友好），全选 1.9M 插入实测 ~13ms（约 10x）；
//! - 索引超出表容量时退化为常规开放探测，**正确性不受影响**（仅哈希质量下降）。
//!
//! 安全性：键为本地索引，非不可信输入，无哈希碰撞 DoS 面。
//! 非 `usize` 键路径（防御性 `write`）按字节小端组装，保证哈希合法。

use std::hash::{BuildHasherDefault, Hasher};

/// 恒等哈希：`usize` 键直接返回键值本身
#[derive(Default)]
pub struct DenseIndexHasher(u64);

impl Hasher for DenseIndexHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // 防御路径：SelectionSet 仅存 usize（走 write_usize），此处仅保证哈希合法
        let mut v = 0u64;
        for (i, b) in bytes.iter().take(8).enumerate() {
            v |= (*b as u64) << (i * 8);
        }
        self.0 = v;
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.0 = i as u64;
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

/// 选中音符索引集合（稠密整数专用恒等哈希，见模块文档）
pub type SelectionSet = std::collections::HashSet<usize, BuildHasherDefault<DenseIndexHasher>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_contains_remove_dense() {
        let mut set = SelectionSet::default();
        for i in 0..1000usize {
            assert!(set.insert(i), "首次插入应返回 true");
        }
        assert_eq!(set.len(), 1000);
        for i in 0..1000usize {
            assert!(set.contains(&i), "已插入索引应命中");
        }
        assert!(!set.contains(&1000));
        assert!(set.remove(&500));
        assert!(!set.contains(&500));
        assert_eq!(set.len(), 999);
        assert!(set.insert(500), "删除后可重新插入");
        assert!(set.contains(&500));
    }

    #[test]
    fn test_keys_beyond_table_capacity() {
        // 稀疏大索引（超表容量 → 恒等哈希退化探测）仍正确
        let mut set = SelectionSet::default();
        set.reserve(4);
        for &i in &[0usize, 1, 1 << 22, (1 << 22) + 1, 1 << 30] {
            assert!(set.insert(i));
        }
        for &i in &[0usize, 1, 1 << 22, (1 << 22) + 1, 1 << 30] {
            assert!(set.contains(&i));
        }
        assert_eq!(set.len(), 5);
        assert!(!set.contains(&2));
        assert!(!set.contains(&((1 << 22) + 2)));
    }

    #[test]
    fn test_clear_reuse_and_iteration_completeness() {
        let mut set = SelectionSet::default();
        set.extend(0..10_000usize);
        set.clear();
        set.extend((0..10_000usize).filter(|i| i % 2 == 0));
        assert_eq!(set.len(), 5000);
        let mut seen: Vec<usize> = set.iter().copied().collect();
        seen.sort_unstable();
        assert_eq!(
            seen,
            (0..10_000usize).filter(|i| i % 2 == 0).collect::<Vec<_>>()
        );
    }
}
