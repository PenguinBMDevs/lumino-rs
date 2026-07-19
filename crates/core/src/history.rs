//! History module for undo/redo functionality
//!
//! 方案 X：保留 im::Vector 快照 + 元数据增强
//! - 每个 EditorSnapshot 附带 group_id / parent_group_id / timestamp / op_kind / entry_count
//! - 批量移动延迟提交（拖动期间不 push，松手时 push 一次）
//! - 音符创建走 300ms 合并窗口（OpKind::NoteCreate.is_mergeable() = true）
//! - 超过 max_entries_per_group 时分割为新 group，parent_group_id 指向被分割的旧 group
//! - undo_logical / redo_logical 跨 parent_group_id 一次性回退/重做整个逻辑操作
//! - NoteMove 使用轻量 MoveOp 操作日志替代完整快照，降低内存占用

use crate::automation::AutomationLane;
use crate::note::Note;
use im::Vector;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

mod entry;
pub use entry::{HistoryEntry, MoveOp, OperationEntry};

/// 操作类型（决定是否走合并窗口、是否参与逻辑撤销链）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// 音符创建（Pencil 绘制，走 300ms 合并窗口）
    NoteCreate,
    /// 音符移动（批量拖动，延迟提交，不走合并）
    NoteMove,
    /// 音符删除
    NoteDelete,
    /// 音符变换（翻转/移调/变速）
    NoteTransform,
    /// 力度调整
    VelocityEdit,
    /// 自动化编辑
    AutomationEdit,
    /// MIDI 录制
    Recording,
    /// 其他操作（默认）
    Other,
    /// 逻辑 chain 的 after 标记（undo_logical 时推入 redo_stack，redo_logical 时返回给用户）
    ///
    /// 内部使用，用户不直接构造。
    ChainMarker,
}

impl OpKind {
    /// 是否支持合并窗口（仅 NoteCreate 走合并）
    pub fn is_mergeable(self) -> bool {
        matches!(self, OpKind::NoteCreate)
    }

    /// 是否为内部 chain 标记
    pub fn is_chain_marker(self) -> bool {
        matches!(self, OpKind::ChainMarker)
    }
}

impl Default for OpKind {
    fn default() -> Self {
        OpKind::Other
    }
}

/// A snapshot of the editor state for undo/redo functionality
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    pub notes: Vector<Note>,
    pub current_track: usize,
    /// 自动化 lane 快照。`Arc` 共享：未修改的 lane 在所有快照间物理共址，
    /// 快照克隆为 O(lane 数) 指针拷贝。编辑路径用 `Arc::make_mut` 写时复制。
    pub automation_lanes: Vec<Arc<AutomationLane>>,
    /// 操作元数据：分组 ID（同一逻辑操作的所有快照共享 group_id）
    pub group_id: Option<u64>,
    /// 父分组 ID（超限分割时，新分组的 parent 指向被分割的旧分组）
    pub parent_group_id: Option<u64>,
    /// 操作时间戳（用于合并窗口判断）
    pub timestamp: Instant,
    /// 操作类型
    pub op_kind: OpKind,
    /// 该分组内已合并的条目数（用于超限分割判断）
    pub entry_count: u32,
}

impl EditorSnapshot {
    /// 创建快照（向后兼容，元数据用默认值）
    pub fn new(
        notes: Vector<Note>,
        current_track: usize,
        automation_lanes: Vec<Arc<AutomationLane>>,
    ) -> Self {
        Self {
            notes,
            current_track,
            automation_lanes,
            group_id: None,
            parent_group_id: None,
            timestamp: Instant::now(),
            op_kind: OpKind::Other,
            entry_count: 1,
        }
    }

    /// 创建带元数据的快照
    pub fn with_metadata(
        notes: Vector<Note>,
        current_track: usize,
        automation_lanes: Vec<Arc<AutomationLane>>,
        op_kind: OpKind,
        group_id: Option<u64>,
        parent_group_id: Option<u64>,
        entry_count: u32,
    ) -> Self {
        Self {
            notes,
            current_track,
            automation_lanes,
            group_id,
            parent_group_id,
            timestamp: Instant::now(),
            op_kind,
            entry_count,
        }
    }
}

/// History manager for undo/redo
#[derive(Debug)]
pub struct History {
    /// 使用 `VecDeque` 替代 `Vec` 避免栈满时 `remove(0)` 的 O(n) 平移。
    undo_stack: VecDeque<HistoryEntry>,
    redo_stack: VecDeque<HistoryEntry>,
    max_size: usize,
    /// 合并窗口（毫秒），仅对 `OpKind::NoteCreate` 生效
    merge_window_ms: u64,
    /// 单条分组最大条目数，超过则分割为子分组
    max_entries_per_group: u32,
    /// 下一个 group_id（单调递增）
    next_group_id: u64,
}

impl History {
    pub fn new() -> Self {
        Self::with_config(100, 300, 1000)
    }

    /// 创建带配置的 History
    pub fn with_config(max_size: usize, merge_window_ms: u64, max_entries_per_group: u32) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_size,
            merge_window_ms,
            max_entries_per_group,
            next_group_id: 1,
        }
    }

    /// 设置配置（用于运行时从 UiConfig 注入）
    pub fn set_config(
        &mut self,
        max_size: usize,
        merge_window_ms: u64,
        max_entries_per_group: u32,
    ) {
        self.max_size = max_size;
        self.merge_window_ms = merge_window_ms;
        self.max_entries_per_group = max_entries_per_group;
        // 如果新上限更小，立即裁剪
        while self.undo_stack.len() > self.max_size {
            self.undo_stack.pop_front();
        }
    }

    /// 分配新的 group_id
    fn alloc_group_id(&mut self) -> u64 {
        let id = self.next_group_id;
        self.next_group_id = self.next_group_id.wrapping_add(1);
        id
    }

    /// Push a new snapshot to the undo stack（向后兼容版本，op_kind=Other）
    pub fn push(&mut self, snapshot: EditorSnapshot) {
        let snap = EditorSnapshot {
            group_id: Some(self.alloc_group_id()),
            op_kind: OpKind::Other,
            ..snapshot
        };
        self.push_internal(HistoryEntry::Snapshot(snap));
    }

    /// 推入带 op_kind 的快照（不合并，但记录 group_id）
    pub fn push_with_op_kind(&mut self, snapshot: EditorSnapshot, op_kind: OpKind) {
        let snap = EditorSnapshot {
            group_id: Some(self.alloc_group_id()),
            parent_group_id: None,
            timestamp: Instant::now(),
            op_kind,
            entry_count: 1,
            ..snapshot
        };
        self.push_internal(HistoryEntry::Snapshot(snap));
    }

    /// 推入可合并的快照（仅 `OpKind::NoteCreate` 等可合并类型）
    ///
    /// 合并规则：
    /// 1. 栈顶 op_kind 相同 + 在合并窗口内 + 未超 entry 上限 → 替换栈顶，entry_count + 1
    /// 2. 栈顶 op_kind 相同 + 在合并窗口内 + 超过 entry 上限 → 分割为新分组，parent_group_id 指向旧
    /// 3. 否则 → 新增分组
    ///
    /// 返回 `true` 表示合并到上一条，`false` 表示新增一条。
    pub fn push_mergeable(&mut self, snapshot: EditorSnapshot, op_kind: OpKind) -> bool {
        let now = Instant::now();

        if let Some(HistoryEntry::Snapshot(top)) = self.undo_stack.back() {
            let same_kind = top.op_kind == op_kind;
            // 严格小于：window=0 时任何间隔都不在窗口内（语义：0 窗口 = 不合并）
            let within_window =
                (now.duration_since(top.timestamp).as_millis() as u64) < self.merge_window_ms;
            let under_limit = top.entry_count < self.max_entries_per_group;

            if same_kind && within_window && under_limit {
                let parent_group_id = top.parent_group_id;
                let group_id = top.group_id;
                let merged = EditorSnapshot {
                    notes: top.notes.clone(),
                    current_track: top.current_track,
                    automation_lanes: top.automation_lanes.clone(),
                    group_id,
                    parent_group_id,
                    timestamp: now,
                    op_kind,
                    entry_count: top.entry_count + 1,
                };
                self.undo_stack.pop_back();
                self.push_internal(HistoryEntry::Snapshot(merged));
                return true;
            }

            if same_kind && within_window && !under_limit {
                let parent_id = top.group_id;
                let split = EditorSnapshot {
                    group_id: Some(self.alloc_group_id()),
                    parent_group_id: parent_id,
                    timestamp: now,
                    op_kind,
                    entry_count: 1,
                    ..snapshot
                };
                self.push_internal(HistoryEntry::Snapshot(split));
                return false;
            }
        }

        // 无可合并项，新增分组
        let new_snap = EditorSnapshot {
            group_id: Some(self.alloc_group_id()),
            parent_group_id: None,
            timestamp: now,
            op_kind,
            entry_count: 1,
            ..snapshot
        };
        self.push_internal(HistoryEntry::Snapshot(new_snap));
        false
    }

    /// 推入 MoveOp 操作日志（NoteMove 用），返回分配的 group_id
    pub fn push_move_op(&mut self, ops: Vec<MoveOp>) -> u64 {
        let group_id = self.alloc_group_id();
        let entry = OperationEntry {
            ops,
            op_kind: OpKind::NoteMove,
            group_id: Some(group_id),
            parent_group_id: None,
            timestamp: Instant::now(),
            entry_count: 1,
        };
        self.push_internal(HistoryEntry::Operation(entry));
        group_id
    }

    fn push_internal(&mut self, entry: HistoryEntry) {
        self.undo_stack.push_back(entry);
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_size {
            self.undo_stack.pop_front();
        }
    }

    /// Undo the last action and return the previous state（单步撤销）
    ///
    /// 对 `Snapshot`：把当前状态快照推入 redo_stack，并返回栈顶快照用于恢复。
    /// 对 `Operation`：把反向操作推入 redo_stack，不推快照，避免混合堆叠导致
    /// 多次 undo/redo 时快照覆盖操作结果。
    pub fn undo(&mut self, current_state: EditorSnapshot) -> Option<HistoryEntry> {
        if self.undo_stack.is_empty() {
            return None;
        }
        match self.undo_stack.pop_back()? {
            HistoryEntry::Snapshot(s) => {
                self.redo_stack
                    .push_back(HistoryEntry::Snapshot(current_state));
                Some(HistoryEntry::Snapshot(s))
            }
            HistoryEntry::Operation(op) => {
                let inverse = op.inverse();
                self.redo_stack
                    .push_back(HistoryEntry::Operation(inverse.clone()));
                Some(HistoryEntry::Operation(inverse))
            }
        }
    }

    /// Redo the last undone action（单步重做）
    pub fn redo(&mut self, current_state: EditorSnapshot) -> Option<HistoryEntry> {
        if self.redo_stack.is_empty() {
            return None;
        }
        match self.redo_stack.pop_back()? {
            HistoryEntry::Snapshot(s) => {
                self.undo_stack
                    .push_back(HistoryEntry::Snapshot(current_state));
                Some(HistoryEntry::Snapshot(s))
            }
            HistoryEntry::Operation(op) => {
                let forward = op.inverse();
                self.undo_stack
                    .push_back(HistoryEntry::Operation(forward.clone()));
                Some(HistoryEntry::Operation(forward))
            }
        }
    }

    /// 丢弃最近一次 undo 条目，不触碰 redo 栈。
    ///
    /// 用于 `push_history()` 调用后发现没有实际变更的场景：
    /// 不应调用 `undo()`（会污染 redo 栈），而应直接丢弃刚 push 的空快照。
    /// O(1) 操作——VecDeque pop_back。
    pub fn discard_last(&mut self) {
        self.undo_stack.pop_back();
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// 当前 undo 栈大小（测试用）
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// 当前 redo 栈大小（测试用）
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// 查看 undo 栈顶（测试用）
    pub fn undo_back(&self) -> Option<&HistoryEntry> {
        self.undo_stack.back()
    }

    /// 查看 redo 栈顶（测试用）
    #[cfg(test)]
    pub(crate) fn redo_back(&self) -> Option<&HistoryEntry> {
        self.redo_stack.back()
    }

    /// 查看 redo 栈底（测试用）
    #[cfg(test)]
    pub(crate) fn redo_front(&self) -> Option<&HistoryEntry> {
        self.redo_stack.front()
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

mod logical;

#[cfg(test)]
mod tests;
