//! 音符创建操作日志推送（NoteCreate 增量、极简化）
//!
//! 拆分原因：`history.rs` 超 400 行限制，将 `push_note_create` 拆出到独立文件。
//! 本模块是 `history` 的子模块，可直接访问 `History` 的私有字段与方法。

use std::time::Instant;

use super::{CreateEntry, CreateOp, History, HistoryEntry};

impl History {
    /// 推入音符创建操作日志（NoteCreate 用，增量替代快照）
    ///
    /// 合并规则（与快照版 `push_mergeable` 一致）：
    /// 1. 栈顶是 Create + 在合并窗口内 + 未超 entry 上限 → 追加 ops，entry_count + 1
    /// 2. 栈顶是 Create + 在合并窗口内 + 超过上限 → 分割为新分组，parent_group_id 指向旧
    /// 3. 否则 → 新增分组
    ///
    /// 返回 `true` 表示合并到上一条，`false` 表示新增/分割。
    /// 每 op 仅 20 字节，与音符总量解耦——1600W 音符工程的铅笔绘制
    /// 不再克隆整轨快照（对比：快照合并时 `..top.clone()` 复制全快照）。
    pub fn push_note_create(&mut self, ops: Vec<CreateOp>) -> bool {
        let now = Instant::now();

        if let Some(HistoryEntry::Create(top)) = self.undo_stack.back() {
            let within_window =
                (now.duration_since(top.timestamp).as_millis() as u64) < self.merge_window_ms;
            let under_limit = top.entry_count < self.max_entries_per_group;

            if within_window && under_limit {
                // 合并：追加 ops（时间正序），保留 group 链
                let mut merged_ops = top.ops.clone();
                merged_ops.extend(ops);
                let parent_group_id = top.parent_group_id;
                let group_id = top.group_id;
                let merged = CreateEntry {
                    ops: merged_ops,
                    group_id,
                    parent_group_id,
                    timestamp: now,
                    entry_count: top.entry_count + 1,
                };
                self.undo_stack.pop_back();
                self.push_internal(HistoryEntry::Create(merged));
                return true;
            }

            if within_window && !under_limit {
                // 超限分割：新分组，parent 指向旧组（逻辑撤销链）
                let parent_id = top.group_id;
                let split = CreateEntry {
                    ops,
                    group_id: Some(self.alloc_group_id()),
                    parent_group_id: parent_id,
                    timestamp: now,
                    entry_count: 1,
                };
                self.push_internal(HistoryEntry::Create(split));
                return false;
            }
        }

        // 无可合并项，新增分组
        let new_entry = CreateEntry {
            ops,
            group_id: Some(self.alloc_group_id()),
            parent_group_id: None,
            timestamp: now,
            entry_count: 1,
        };
        self.push_internal(HistoryEntry::Create(new_entry));
        false
    }
}
