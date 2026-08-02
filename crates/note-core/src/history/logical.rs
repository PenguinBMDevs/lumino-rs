//! 逻辑撤销/重做（跨 group chain 一次性回退/重做）
//!
//! 拆分原因：`history.rs` 超 400 行限制，按职责拆分。
//!
//! 逻辑撤销链：通过 `parent_group_id` 串联超限分割的子分组，
//! `undo_logical` / `redo_logical` 跨 chain 一次性回退/重做整个逻辑操作。
//!
//! NoteMove 使用轻量 `OperationEntry`，不参与 chain，逻辑撤销时单条退化。

use super::{EditorSnapshot, History, HistoryEntry, OpKind};
use std::collections::HashSet;

impl History {
    /// 逻辑撤销：跨同 group_id / parent_group_id 链一次性撤销
    ///
    /// 流程：
    /// 1. 把 current 标记为 ChainMarker 推入 redo_stack（chain 的 after 状态）
    /// 2. 持续 pop undo_stack 顶部的同 chain 快照，推入 redo_stack
    /// 3. 返回 chain 中最早被 pop 的快照（chain 操作之前的状态）
    ///
    /// 如果栈顶是 `OperationEntry`（如 NoteMove），退化为普通单步 undo。
    pub fn undo_logical(&mut self, current_state: EditorSnapshot) -> Option<HistoryEntry> {
        if self.undo_stack.is_empty() {
            return None;
        }

        // Operation 不参与 chain，直接退化
        if matches!(self.undo_stack.back()?, HistoryEntry::Operation(_)) {
            return self.undo(current_state);
        }

        let top = self.undo_stack.back()?;
        let HistoryEntry::Snapshot(bx) = top else {
            return self.undo(current_state);
        };
        let chain_groups = Self::collect_chain_groups(bx.as_ref());

        // 把 current 标记为 ChainMarker 推入 redo（chain 的 after 状态）
        let mut marker = current_state;
        marker.op_kind = OpKind::ChainMarker;
        self.redo_stack
            .push_back(HistoryEntry::Snapshot(Box::new(marker)));

        // pop 同 chain 的快照，记录 chain 中最早（最后被 pop）的 snapshot
        let mut earliest_in_chain: Option<HistoryEntry> = None;
        while let Some(top) = self.undo_stack.back() {
            if let HistoryEntry::Snapshot(bx) = top
                && Self::snapshot_in_chain(bx.as_ref(), &chain_groups)
            {
                let snap = self.undo_stack.pop_back()?;
                earliest_in_chain = Some(snap.clone());
                self.redo_stack.push_back(snap);
                continue;
            }
            break;
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
    ///
    /// 如果 redo 栈顶是 `OperationEntry`，退化为普通单步 redo。
    pub fn redo_logical(&mut self, current_state: EditorSnapshot) -> Option<HistoryEntry> {
        if self.redo_stack.is_empty() {
            return None;
        }

        // Operation 不参与 chain，直接退化
        if matches!(self.redo_stack.back()?, HistoryEntry::Operation(_)) {
            return self.redo(current_state);
        }

        // 把 current 推入 undo
        self.undo_stack
            .push_back(HistoryEntry::Snapshot(Box::new(current_state.clone())));

        // 找到当前 chain 的 group 集合（从 redo_stack 栈顶）
        let top = self.redo_stack.back()?;
        let HistoryEntry::Snapshot(bx) = top else {
            // 非 Snapshot 的 chain 理论上不会出现，安全退化
            return self.redo(current_state);
        };
        let chain_groups = Self::collect_chain_groups(bx.as_ref());

        // pop 同 chain 的快照（不包括 ChainMarker）
        while let Some(top) = self.redo_stack.back() {
            if let HistoryEntry::Snapshot(bx) = top {
                let s = bx.as_ref();
                if s.op_kind.is_chain_marker() {
                    break;
                }
                if Self::snapshot_in_chain(s, &chain_groups) {
                    let snap = self.redo_stack.pop_back()?;
                    self.undo_stack.push_back(snap);
                    continue;
                }
            }
            break;
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
                .is_some_and(|p| chain_groups.contains(&Some(p)))
    }
}
