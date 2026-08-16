//! 逻辑撤销/重做（跨 group chain 一次性回退/重做）
//!
//! 拆分原因：`history.rs` 超 400 行限制，按职责拆分。
//!
//! 逻辑撤销链：通过 `parent_group_id` 串联超限分割的子分组，
//! `undo_logical` / `redo_logical` 跨 chain 一次性回退/重做整个逻辑操作。
//!
//! NoteMove 使用轻量 `OperationEntry`，不参与 chain，逻辑撤销时单条退化。

use super::{CreateEntry, EditorSnapshot, History, HistoryEntry, OpKind};
use std::collections::HashSet;
use std::time::Instant;

impl History {
    /// 逻辑撤销：跨同 group_id / parent_group_id 链一次性撤销
    ///
    /// 流程：
    /// 1. 把 current 标记为 ChainMarker 推入 redo_stack（chain 的 after 状态）
    /// 2. 持续 pop undo_stack 顶部的同 chain 快照，推入 redo_stack
    /// 3. 返回 chain 中最早被 pop 的快照（chain 操作之前的状态）
    ///
    /// 如果栈顶是 `OperationEntry`（如 NoteMove），退化为普通单步 undo。
    /// 如果栈顶是 `CreateEntry`（NoteCreate 增量日志），跨链合并所有 op 一次性返回。
    pub fn undo_logical(&mut self, current_state: EditorSnapshot) -> Option<HistoryEntry> {
        if self.undo_stack.is_empty() {
            return None;
        }

        // Operation 不参与 chain，直接退化
        if matches!(self.undo_stack.back()?, HistoryEntry::Operation(_)) {
            return self.undo(current_state);
        }

        // Create 链：跨 group 合并所有 op 一次性撤销（编辑侧逆序按值删除）
        if let HistoryEntry::Create(_) = self.undo_stack.back()? {
            return self.undo_logical_create();
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
    /// 如果 redo 栈顶是 `CreateEntry`，跨链合并所有 op 一次性返回。
    pub fn redo_logical(&mut self, current_state: EditorSnapshot) -> Option<HistoryEntry> {
        if self.redo_stack.is_empty() {
            return None;
        }

        // Operation 不参与 chain，直接退化
        if matches!(self.redo_stack.back()?, HistoryEntry::Operation(_)) {
            return self.redo(current_state);
        }

        // Create 链：跨 group 合并所有 op 一次性重做（编辑侧正序插入）
        if let HistoryEntry::Create(_) = self.redo_stack.back()? {
            return self.redo_logical_create();
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

    /// Create 链逻辑撤销：pop 同 chain 的 Create 条目，
    /// 合并全部 op 为一条返回（编辑侧逆序按值删除），原条目搬入 redo 栈。
    fn undo_logical_create(&mut self) -> Option<HistoryEntry> {
        let HistoryEntry::Create(top) = self.undo_stack.back()? else {
            return None;
        };
        let chain_groups = Self::collect_create_chain(top);

        let mut all_ops: Vec<super::CreateOp> = Vec::new();
        let mut oldest: Option<(Option<u64>, Option<u64>, Instant, u32)> = None;
        while let Some(top) = self.undo_stack.back() {
            if let HistoryEntry::Create(e) = top
                && Self::create_in_chain(e, &chain_groups)
            {
                let entry = self.undo_stack.pop_back()?;
                let HistoryEntry::Create(e) = &entry else {
                    continue;
                };
                if oldest.is_none() {
                    oldest = Some((e.group_id, e.parent_group_id, e.timestamp, e.entry_count));
                }
                // 时间正序：先 pop 的是最新组，前插保持创建顺序
                let mut new_ops = e.ops.clone();
                new_ops.extend(all_ops);
                all_ops = new_ops;
                self.redo_stack.push_back(entry);
                continue;
            }
            break;
        }
        let (group_id, parent_group_id, timestamp, entry_count) =
            oldest.unwrap_or((None, None, Instant::now(), 1));
        Some(HistoryEntry::Create(CreateEntry {
            ops: all_ops,
            group_id,
            parent_group_id,
            timestamp,
            entry_count,
        }))
    }

    /// Create 链逻辑重做：pop 同 chain 的 Create 条目（搬回 undo 栈），
    /// 合并全部 op 为一条返回（编辑侧正序插入）。
    fn redo_logical_create(&mut self) -> Option<HistoryEntry> {
        let HistoryEntry::Create(top) = self.redo_stack.back()? else {
            return None;
        };
        let chain_groups = Self::collect_create_chain(top);

        let mut all_ops: Vec<super::CreateOp> = Vec::new();
        let mut oldest: Option<(Option<u64>, Option<u64>, Instant, u32)> = None;
        while let Some(top) = self.redo_stack.back() {
            if let HistoryEntry::Create(e) = top
                && Self::create_in_chain(e, &chain_groups)
            {
                let entry = self.redo_stack.pop_back()?;
                let HistoryEntry::Create(e) = &entry else {
                    continue;
                };
                if oldest.is_none() {
                    oldest = Some((e.group_id, e.parent_group_id, e.timestamp, e.entry_count));
                }
                all_ops.extend(e.ops.clone());
                self.undo_stack.push_back(entry);
                continue;
            }
            break;
        }
        let (group_id, parent_group_id, timestamp, entry_count) =
            oldest.unwrap_or((None, None, Instant::now(), 1));
        Some(HistoryEntry::Create(CreateEntry {
            ops: all_ops,
            group_id,
            parent_group_id,
            timestamp,
            entry_count,
        }))
    }

    /// 收集 Create 条目所属 chain 的所有 group_id（自身 + parent）
    fn collect_create_chain(entry: &CreateEntry) -> HashSet<Option<u64>> {
        let mut set = HashSet::new();
        set.insert(entry.group_id);
        if let Some(p) = entry.parent_group_id {
            set.insert(Some(p));
        }
        set
    }

    /// 判断 Create 条目是否属于指定 chain
    fn create_in_chain(entry: &CreateEntry, chain_groups: &HashSet<Option<u64>>) -> bool {
        chain_groups.contains(&entry.group_id)
            || entry
                .parent_group_id
                .is_some_and(|p| chain_groups.contains(&Some(p)))
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
