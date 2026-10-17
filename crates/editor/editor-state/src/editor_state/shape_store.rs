//! 形状对象的文档级存储与撤销分组日志（S0-3）
//!
//! ## 为什么形状不进 `EditorSnapshot`
//!
//! 撤销口径（已拍板）：**不改 `EditorSnapshot` 结构**。撤销时：
//! - **音符**由文档历史（快照 / 操作日志）自行回退；
//! - **形状参数**由本模块的**分组日志**回退；
//! - 两者靠**同一个历史 `group_id`** 对齐 ⇒ 形状与它生成的音符天然同生共死。
//!
//! 这条路径沿用仓库既有范式（`drawn_shape.rs` 的 `on_undo_group` / `on_redo_group`
//! 已由 `editor_impl/history.rs` 在撤销/重做成功处调用），只是把「一次性平移回放」
//! 升级为「参数版本回放」。
//!
//! ## 音符指纹
//!
//! `notes` 记录该形状**生成的音符**（值指纹：track/tick/key/length）。
//! 再生成时先按值清理这些旧音符、再写入新音符 —— 这是 S0-3b 的写入口径，
//! 也是「形状重生成不残留孤儿音符」的唯一保证。
//!
//! ## 日志容量
//!
//! 与 `History::new` 的 100 条同量级：超出上限丢最旧记录（撤销不到那么远，
//! 与文档历史的行为一致）。

use std::collections::VecDeque;

use lumino_core::shape::ShapeModel;

use super::drawn_shape::ShapeNote;

/// 撤销分组日志容量上限
pub const MAX_SHAPE_LOG: usize = 128;

/// 一个形状条目
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeEntry {
    /// 形状参数（文档对象本体）
    pub model: ShapeModel,
    /// 该形状当前生成的音符（值指纹；由 S0-3b 的再生成维护）
    pub notes: Vec<ShapeNote>,
    /// 创建动作已被撤销（形状与它的音符都应视为不存在）
    pub hidden_by_creation: bool,
    /// 删除动作已提交（撤销删除即恢复）
    pub deleted: bool,
}

impl ShapeEntry {
    /// 是否对用户可见（未隐藏、未删除）
    pub fn is_visible(&self) -> bool {
        !self.hidden_by_creation && !self.deleted
    }
}

/// 撤销日志中的一条分组记录（保留前后像以支持重做）
#[derive(Debug, Clone)]
enum ShapeGroupRecord {
    /// 创建
    Created { group: u64, id: u64 },
    /// 参数变更
    Edited {
        group: u64,
        id: u64,
        before: Box<ShapeModel>,
        after: Box<ShapeModel>,
    },
    /// 删除
    Deleted { group: u64, id: u64 },
}

impl ShapeGroupRecord {
    /// 记录所属的历史分组 ID
    fn group(&self) -> u64 {
        match self {
            Self::Created { group, .. }
            | Self::Edited { group, .. }
            | Self::Deleted { group, .. } => *group,
        }
    }
}

/// 形状存储：文档级形状对象 + 撤销分组日志
#[derive(Debug, Default)]
pub struct ShapeStore {
    entries: Vec<ShapeEntry>,
    next_id: u64,
    log: VecDeque<ShapeGroupRecord>,
    /// 已撤销的形状记录（LIFO）：Snapshot 历史在 **redo 侧会丢 group_id**
    /// （见 `editor_impl/history.rs` 的既有说明），故重做优先按分组回放，
    /// 分组缺失时按此栈兜底回放。
    redo_stack: Vec<ShapeGroupRecord>,
}

impl ShapeStore {
    /// 创建空存储
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            log: VecDeque::new(),
            redo_stack: Vec::new(),
        }
    }

    /// 清空（文档重建 / `EditorState::reset`）
    pub fn clear(&mut self) {
        self.entries.clear();
        self.log.clear();
        self.redo_stack.clear();
        self.next_id = 1;
    }

    /// 形状数量（含隐藏与已删除）
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 分组日志长度（测试与诊断用）
    pub fn log_len(&self) -> usize {
        self.log.len()
    }

    /// 待重做栈长度（测试与诊断用）
    pub fn redo_stack_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// 按 id 取形状参数
    pub fn model(&self, id: u64) -> Option<&ShapeModel> {
        self.index_of(id).map(|i| &self.entries[i].model)
    }

    /// 按 id 取条目
    pub fn entry(&self, id: u64) -> Option<&ShapeEntry> {
        self.index_of(id).map(|i| &self.entries[i])
    }

    /// 遍历全部条目
    pub fn iter(&self) -> impl Iterator<Item = &ShapeEntry> {
        self.entries.iter()
    }

    /// 全部 id（顺序即创建顺序）
    pub fn ids(&self) -> Vec<u64> {
        self.entries.iter().map(|e| e.model.id).collect()
    }

    /// 插入形状；`model.id == 0` 时自动分配 id，返回最终 id
    ///
    /// `group` 为该次创建对应的历史分组 ID（`0` 表示不记日志，仅加载用）。
    pub fn insert(&mut self, mut model: ShapeModel, group: u64) -> u64 {
        if model.id == 0 {
            model.id = self.next_id;
        }
        self.next_id = self.next_id.max(model.id.saturating_add(1));
        let id = model.id;
        self.entries.push(ShapeEntry {
            model,
            notes: Vec::new(),
            hidden_by_creation: false,
            deleted: false,
        });
        if group != 0 {
            self.push_record(ShapeGroupRecord::Created { group, id });
        }
        id
    }

    /// 修改形状参数（记录前后像）；形状不存在返回 `false`
    pub fn edit(&mut self, id: u64, new_model: ShapeModel, group: u64) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        let before = self.entries[index].model.clone();
        let mut new_model = new_model;
        new_model.id = id;
        self.entries[index].model = new_model.clone();
        if group != 0 {
            self.push_record(ShapeGroupRecord::Edited {
                group,
                id,
                before: Box::new(before),
                after: Box::new(new_model),
            });
        }
        true
    }

    /// 标记删除（记录分组，撤销删除即可恢复）；形状不存在返回 `false`
    pub fn mark_deleted(&mut self, id: u64, group: u64) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        self.entries[index].deleted = true;
        if group != 0 {
            self.push_record(ShapeGroupRecord::Deleted { group, id });
        }
        true
    }

    /// 写入该形状当前生成的音符指纹
    pub fn assign_notes(&mut self, id: u64, notes: Vec<ShapeNote>) -> bool {
        match self.index_of(id) {
            Some(index) => {
                self.entries[index].notes = notes;
                true
            }
            None => false,
        }
    }

    /// 该形状当前生成的音符指纹
    pub fn notes_of(&self, id: u64) -> &[ShapeNote] {
        match self.index_of(id) {
            Some(index) => &self.entries[index].notes,
            None => &[],
        }
    }

    /// 需要**持久化**的形状（隐藏或已删除的不写盘）
    pub fn persisted_models(&self) -> Vec<ShapeModel> {
        self.entries
            .iter()
            .filter(|e| e.is_visible())
            .map(|e| e.model.clone())
            .collect()
    }

    /// 从工程加载（**不记日志**：加载不是用户操作）
    pub fn load_all(&mut self, models: Vec<ShapeModel>) {
        self.clear();
        for model in models {
            self.insert(model, 0);
        }
    }

    /// 撤销一个历史分组对形状的影响
    ///
    /// 同时把该记录压入**重做栈**：Snapshot 历史的 group_id 在 redo 侧会丢，
    /// 重做需要这条栈兜底（与 `drawn_shape.rs` 的 redo 侧对账同因）。
    pub fn on_undo_group(&mut self, group: u64) {
        let Some(record) = self.find_group(group) else {
            return;
        };
        self.apply_undo(&record);
        if self.redo_stack.len() >= MAX_SHAPE_LOG {
            self.redo_stack.remove(0);
        }
        self.redo_stack.push(record);
    }

    /// 重做一个历史分组对形状的影响
    ///
    /// 分组可用时按记录回放并清栈；分组缺失（redo 侧丢 group_id）时退化为
    /// [`Self::on_redo_without_group`] 的 LIFO 兜底回放。
    pub fn on_redo_group(&mut self, group: u64) {
        match self.find_group(group) {
            Some(record) => {
                self.apply_redo(&record);
                self.redo_stack.retain(|r| r.group() != group);
            }
            None => self.on_redo_without_group(),
        }
    }

    /// 重做但没有分组信息：按撤销栈 LIFO 回放最近一次被撤销的形状变更
    pub fn on_redo_without_group(&mut self) {
        let Some(record) = self.redo_stack.pop() else {
            return;
        };
        self.apply_redo(&record);
    }

    fn apply_undo(&mut self, record: &ShapeGroupRecord) {
        match record {
            ShapeGroupRecord::Created { id, .. } => {
                self.set_flag(*id, |e| e.hidden_by_creation = true);
            }
            ShapeGroupRecord::Edited { id, before, .. } => {
                if let Some(index) = self.index_of(*id) {
                    self.entries[index].model = (**before).clone();
                }
            }
            ShapeGroupRecord::Deleted { id, .. } => {
                self.set_flag(*id, |e| e.deleted = false);
            }
        }
    }

    fn apply_redo(&mut self, record: &ShapeGroupRecord) {
        match record {
            ShapeGroupRecord::Created { id, .. } => {
                self.set_flag(*id, |e| e.hidden_by_creation = false);
            }
            ShapeGroupRecord::Edited { id, after, .. } => {
                if let Some(index) = self.index_of(*id) {
                    self.entries[index].model = (**after).clone();
                }
            }
            ShapeGroupRecord::Deleted { id, .. } => {
                self.set_flag(*id, |e| e.deleted = true);
            }
        }
    }

    /// 找该分组的最后一条记录（同一分组只应有一条；从后往前取最新）
    fn find_group(&self, group: u64) -> Option<ShapeGroupRecord> {
        self.log.iter().rev().find(|r| r.group() == group).cloned()
    }

    fn set_flag(&mut self, id: u64, apply: impl FnOnce(&mut ShapeEntry)) {
        if let Some(index) = self.index_of(id) {
            apply(&mut self.entries[index]);
        }
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.model.id == id)
    }

    fn push_record(&mut self, record: ShapeGroupRecord) {
        if self.log.len() >= MAX_SHAPE_LOG {
            self.log.pop_front();
        }
        self.log.push_back(record);
    }
}

#[cfg(test)]
mod tests;
