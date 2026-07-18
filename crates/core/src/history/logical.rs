//! 逻辑撤销/重做（跨 group chain 一次性回退/重做）
//!
//! 拆分原因：`history.rs` 超 400 行限制，按职责拆分。
//!
//! 逻辑撤销链：通过 `parent_group_id` 串联超限分割的子分组，
//! `undo_logical` / `redo_logical` 跨 chain 一次性回退/重做整个逻辑操作。

use super::{EditorSnapshot, History, OpKind};
use std::collections::HashSet;

impl History {
    /// 逻辑撤销：跨同 group_id / parent_group_id 链一次性撤销
    ///
    /// 流程：
    /// 1. 把 current 标记为 ChainMarker 推入 redo_stack（chain 的 after 状态）
    /// 2. 持续 pop undo_stack 顶部的同 chain 快照，推入 redo_stack
    /// 3. 返回 chain 中最早被 pop 的快照（chain 操作之前的状态）
    ///
    /// 如果栈顶是单条 group（无 parent chain），退化为普通 undo。
    pub fn undo_logical(&mut self, current_state: EditorSnapshot) -> Option<EditorSnapshot> {
        if self.undo_stack.is_empty() {
            return None;
        }

        let top = self.undo_stack.back()?;
        let chain_groups = Self::collect_chain_groups(top);

        // 把 current 标记为 ChainMarker 推入 redo（chain 的 after 状态）
        let mut marker = current_state;
        marker.op_kind = OpKind::ChainMarker;
        self.redo_stack.push_back(marker);

        // pop 同 chain 的快照，记录 chain 中最早（最后被 pop）的 snapshot
        let mut earliest_in_chain: Option<EditorSnapshot> = None;
        while let Some(top) = self.undo_stack.back() {
            if Self::snapshot_in_chain(top, &chain_groups) {
                let snap = self.undo_stack.pop_back()?;
                earliest_in_chain = Some(snap.clone());
                self.redo_stack.push_back(snap);
            } else {
                break;
            }
        }

        // 返回 chain 中最早的 snapshot（chain 操作之前的状态）
        earliest_in_chain
    }

    /// 逻辑重做：跨同 group_id / parent_group_id 链一次性重做
    ///
    /// 流程：
    /// 1. 把 current 推入 undo_stack
    /// 2. 持续 pop redo_stack 顶部的同 chain 快照，推入 undo_stack
    /// 3. 遇到 ChainMarker 时停止，pop 并返回（chain 的 after 状态）
    pub fn redo_logical(&mut self, current_state: EditorSnapshot) -> Option<EditorSnapshot> {
        if self.redo_stack.is_empty() {
            return None;
        }

        // 把 current 推入 undo
        self.undo_stack.push_back(current_state);

        // 找到当前 chain 的 group 集合（从 redo_stack 栈顶）
        let top = self.redo_stack.back()?;
        let chain_groups = Self::collect_chain_groups(top);

        // pop 同 chain 的快照（不包括 ChainMarker）
        while let Some(top) = self.redo_stack.back() {
            if top.op_kind.is_chain_marker() {
                break;
            }
            if Self::snapshot_in_chain(top, &chain_groups) {
                let snap = self.redo_stack.pop_back()?;
                self.undo_stack.push_back(snap);
            } else {
                break;
            }
        }

        // pop ChainMarker，返回给用户
        let marker = self.redo_stack.pop_back()?;
        Some(marker)
    }

    /// 收集快照所属 chain 的所有 group_id（自身 + parent）
    fn collect_chain_groups(snap: &EditorSnapshot) -> HashSet<Option<u64>> {
        let mut set = HashSet::new();
        set.insert(snap.group_id);
        if let Some(p) = snap.parent_group_id {
            set.insert(Some(p));
        }
        set
    }

    /// 判断快照是否属于指定 chain
    fn snapshot_in_chain(snap: &EditorSnapshot, chain_groups: &HashSet<Option<u64>>) -> bool {
        chain_groups.contains(&snap.group_id)
            || snap
                .parent_group_id
                .map_or(false, |p| chain_groups.contains(&Some(p)))
    }
}
