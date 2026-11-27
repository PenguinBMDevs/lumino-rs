//! 快照式编辑历史：线性 undo / redo，带重做分支截断。
//!
//! 直线工具与画刷工具此前各维护一份 `Vec<Snapshot> + index`，逻辑逐字重复；
//! 这里收成一份，两个工具保存自己的快照类型。

/// 快照历史。`entries` 至少包含一个基准快照。
#[derive(Debug, Clone, PartialEq)]
pub struct History<S> {
    entries: Vec<S>,
    index: usize,
}

impl<S> History<S> {
    /// 以基准快照创建历史。
    pub fn new(initial: S) -> Self {
        Self {
            entries: vec![initial],
            index: 0,
        }
    }

    /// 清空并用当前快照重建（Bake / 外部状态恢复后调用）。
    pub fn clear_to(&mut self, snapshot: S) {
        self.entries.clear();
        self.entries.push(snapshot);
        self.index = 0;
    }

    /// 追加一个快照（先截断 redo 分支）。
    pub fn push(&mut self, snapshot: S) {
        self.truncate_redo();
        self.entries.push(snapshot);
        self.index = self.entries.len() - 1;
    }

    /// 用当前状态覆盖栈顶（合并连续操作）。
    pub fn update_top(&mut self, snapshot: S) {
        if let Some(top) = self.entries.get_mut(self.index) {
            *top = snapshot;
        }
    }

    /// 截断 redo 分支后覆盖栈顶。
    pub fn merge_top(&mut self, snapshot: S) {
        self.truncate_redo();
        self.update_top(snapshot);
    }

    /// 截断当前索引之后的重做分支。
    pub fn truncate_redo(&mut self) {
        self.entries.truncate(self.index + 1);
    }

    /// 撤销一步；返回目标快照。
    pub fn undo(&mut self) -> Option<&S> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        self.entries.get(self.index)
    }

    /// 重做一步；返回目标快照。
    pub fn redo(&mut self) -> Option<&S> {
        if self.index + 1 >= self.entries.len() {
            return None;
        }
        self.index += 1;
        self.entries.get(self.index)
    }

    pub fn can_undo(&self) -> bool {
        self.index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    /// 当前快照索引（诊断 / 测试）。
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 当前快照。
    pub fn current(&self) -> &S {
        &self.entries[self.index]
    }
}
