//! 选中索引集合：稠密整数位图
//!
//! 键为**当前轨音符索引**（`0..轨道音符数`，局部稠密整数域）。曾用恒等哈希
//! `HashSet`：顺序插入快，但存在致命退化——**索引 ≥ 表容量时**（部分选中/大轨道），
//! 恒等哈希的桶索引 = `index & mask` 与既有连续满桶前缀别名，开放探测要扫过整段
//! 前缀才能确认「不存在」。实测 19.2M 轨道 / 3M 选中做一次全轨成员判断耗时
//! **150 秒**（复制路径逐音符 `contains` → 整条复制链路冻结 160s）。
//!
//! 位图方案在全部维度严格更优：
//! - `insert/contains/remove` 恒 O(1)，与索引量级无关（无哈希退化）；
//! - 顺序写位缓存友好：2.8M 插入实测 ~7ms（恒等哈希 ~13ms）；
//! - 内存 = 轨道长度/8 字节：19.2M 音符 = 2.4MB（3M 选中哈希表 ~48MB）；
//! - `iter()` 天然**升序（轨道序）**，消除哈希任意序输出（Domino/JSON 剪贴板
//!   载荷顺序随之确定）。

/// 选中音符索引位图集合
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SelectionSet {
    /// 位图字（每字 64 位，位 `i` 表示索引 `i` 被选中）
    words: Vec<u64>,
    /// 已选中的索引个数（与置位数保持一致）
    len: usize,
}

impl SelectionSet {
    /// 空集合
    pub fn new() -> Self {
        Self::default()
    }

    /// 确保位图覆盖 `bits` 位（翻倍增长，摊还 O(1)）
    #[inline]
    fn ensure_bits(&mut self, bits: usize) {
        let words = bits.div_ceil(64);
        if words > self.words.len() {
            let target = words.max(self.words.len() * 2).max(4);
            self.words.resize(target, 0);
        }
    }

    /// 插入索引，返回是否为新增（已存在返回 false）
    #[inline]
    pub fn insert(&mut self, index: usize) -> bool {
        self.ensure_bits(index + 1);
        let mask = 1u64 << (index & 63);
        let word = &mut self.words[index >> 6];
        if *word & mask != 0 {
            return false;
        }
        *word |= mask;
        self.len += 1;
        true
    }

    /// 移除索引，返回是否存在（不存在返回 false）
    #[inline]
    pub fn remove(&mut self, index: &usize) -> bool {
        let index = *index;
        let Some(word) = self.words.get_mut(index >> 6) else {
            return false;
        };
        let mask = 1u64 << (index & 63);
        if *word & mask == 0 {
            return false;
        }
        *word &= !mask;
        self.len -= 1;
        true
    }

    /// 索引是否被选中（越界返回 false）
    #[inline]
    pub fn contains(&self, index: &usize) -> bool {
        let index = *index;
        self.words
            .get(index >> 6)
            .is_some_and(|word| word & (1u64 << (index & 63)) != 0)
    }

    /// 清空集合（保留位图容量，供同轨连续框选复用）
    pub fn clear(&mut self) {
        self.words.fill(0);
        self.len = 0;
    }

    /// 已选中索引个数
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 位图容量（位；等价旧 `HashSet::capacity` 的容量语义，调试/监控用）
    #[inline]
    pub fn capacity(&self) -> usize {
        self.words.len() * 64
    }

    /// 预留至少可覆盖 `len + additional` 位的位图空间（避免插入期反复扩容）
    pub fn reserve(&mut self, additional: usize) {
        self.ensure_bits(self.len.saturating_add(additional));
    }

    /// 升序迭代选中索引（轨道序）
    pub fn iter(&self) -> SelectionSetIter<'_> {
        SelectionSetIter {
            words: &self.words,
            word_idx: 0,
            cur: 0,
            base: 0,
        }
    }

    /// 批量插入
    pub fn extend<I: IntoIterator<Item = usize>>(&mut self, iter: I) {
        for i in iter {
            self.insert(i);
        }
    }

    /// 保留满足条件的索引（用于选中集过滤）
    pub fn retain<F: FnMut(&usize) -> bool>(&mut self, mut f: F) {
        let mut removed = 0usize;
        for word_idx in 0..self.words.len() {
            let mut word = self.words[word_idx];
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                let index = word_idx * 64 + bit;
                if !f(&index) {
                    self.words[word_idx] &= !(1u64 << bit);
                    removed += 1;
                }
            }
        }
        self.len -= removed;
    }
}

/// 升序位图迭代器
pub struct SelectionSetIter<'a> {
    words: &'a [u64],
    word_idx: usize,
    cur: u64,
    base: usize,
}

impl Iterator for SelectionSetIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        loop {
            if self.cur != 0 {
                let bit = self.cur.trailing_zeros() as usize;
                self.cur &= self.cur - 1;
                return Some(self.base + bit);
            }
            if self.word_idx >= self.words.len() {
                return None;
            }
            self.base = self.word_idx * 64;
            self.cur = self.words[self.word_idx];
            self.word_idx += 1;
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // 剩余位数上界（不精确统计置位数，避免 O(N) 预扫描）
        let remaining_bits =
            (self.words.len() - self.word_idx) * 64 + self.cur.count_ones() as usize;
        (0, Some(remaining_bits))
    }
}

impl std::iter::FusedIterator for SelectionSetIter<'_> {}

impl FromIterator<usize> for SelectionSet {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut set = Self::new();
        set.extend(iter);
        set
    }
}

impl<'a> IntoIterator for &'a SelectionSet {
    type Item = usize;
    type IntoIter = SelectionSetIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::fmt::Debug for SelectionSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectionSet")
            .field("len", &self.len)
            .field("capacity_bits", &(self.words.len() * 64))
            .finish()
    }
}

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
    fn test_sparse_large_indices_are_cheap_and_correct() {
        // 稀疏大索引（旧恒等哈希的退化场景）：位图 O(1)，且越界判断安全
        let mut set = SelectionSet::default();
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
    fn test_iter_ascending_and_clear_reuse() {
        let mut set = SelectionSet::default();
        set.extend((0..10_000usize).filter(|i| i % 2 == 0));
        assert_eq!(set.len(), 5000);
        let seen: Vec<usize> = set.iter().collect();
        assert_eq!(
            seen,
            (0..10_000usize).filter(|i| i % 2 == 0).collect::<Vec<_>>(),
            "位图迭代必须升序（轨道序）"
        );
        let capacity = set.capacity();
        set.clear();
        assert!(set.is_empty());
        set.extend((0..10_000usize).filter(|i| i % 2 == 0));
        assert_eq!(set.len(), 5000);
        assert!(set.capacity() >= capacity, "clear 后容量应复用不缩水");
    }

    #[test]
    fn test_retain_and_from_iter() {
        let set: SelectionSet = [1usize, 5, 9, 100].into_iter().collect();
        assert_eq!(set.len(), 4);
        let mut set = set;
        set.retain(|&i| i >= 5);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![5, 9, 100]);
    }
}
