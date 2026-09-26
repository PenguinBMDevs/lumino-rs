//! 历史记录条目类型
//!
//! 将 `MoveOp`、`OperationEntry`、`HistoryEntry` 从 `history.rs` 拆分出来，
//! 避免单文件超过 400 行。

use std::time::Instant;

use super::OpKind;

/// 移动操作日志（NoteMove 用轻量 op 替代完整快照，删加语义）
///
/// 音符按**值**引用（`originals` 存移动前完整 `NoteEvent` 快照，
/// 含 tick/end/key/velocity/channel），不存索引区间——索引会因协作远端
/// 插入/删除而漂移；按值经 `position_of` 窗口定位（O(log N + 同 tick 数)，
/// 无全扫兜底），同值多份按份数删（删一即删任一，集合语义等价）。
/// 移动抽象为“删旧 + 加新”：前向删 `originals`、加 `moved`（original + delta，
/// clamp + 长度不变）；undo 反向删 `moved`、加回 `originals`。
#[derive(Debug, Clone, PartialEq)]
pub struct MoveOp {
    /// 音轨 ID
    pub track_id: u32,
    /// 被移动音符的原始快照（按值引用，删加语义的删集合）
    pub originals: Vec<lumino_midi_model::NoteEvent>,
    /// 移动后快照（original + delta，创建时按当时 `max_key` clamp + 长度不变，
    /// 为删加语义的加集合；回放时直接使用，不再按回放时 max_key 重算，
    /// 避免 clamp 标准漂移导致 undo 失配）。
    pub moved: Vec<lumino_midi_model::NoteEvent>,
    /// tick 偏移量（协作广播用，对端按 ref+off 执行）
    pub delta_tick: i32,
    /// key 偏移量（协作广播用）
    pub delta_key: i16,
    /// 同一逻辑操作内的序号
    pub seq: u16,
}

impl MoveOp {
    /// 返回反向操作（值语义下方向由 `inverse` 标志决定，op 本体保持前向不变）。
    /// 为兼容旧调用链保留本方法，当前实现为原样克隆（不再对 delta 取反），
    /// 避免“取反 + 标志”双重反转导致参照落空。
    pub fn inverse(&self) -> Self {
        Self {
            track_id: self.track_id,
            originals: self.originals.clone(),
            moved: self.moved.clone(),
            delta_tick: self.delta_tick,
            delta_key: self.delta_key,
            seq: self.seq,
        }
    }

    /// 取移动后快照（存储值，忽略 `max_key` 参数以保持创建时 clamp 一致）。
    /// 保留参数仅为兼容旧调用签名。
    pub fn moved_notes(&self, _max_key: u16) -> Vec<lumino_midi_model::NoteEvent> {
        self.moved.clone()
    }
}

/// 操作日志条目（替代完整快照）
#[derive(Debug, Clone)]
pub struct OperationEntry {
    /// 移动操作列表
    pub ops: Vec<MoveOp>,
    /// 操作类型
    pub op_kind: OpKind,
    /// 分组 ID
    pub group_id: Option<u64>,
    /// 父分组 ID
    pub parent_group_id: Option<u64>,
    /// 操作时间戳
    pub timestamp: Instant,
    /// 该分组内已合并的条目数
    pub entry_count: u32,
}

impl OperationEntry {
    /// 返回反向操作条目
    pub fn inverse(&self) -> Self {
        Self {
            ops: self.ops.iter().map(MoveOp::inverse).collect(),
            op_kind: self.op_kind,
            group_id: self.group_id,
            parent_group_id: self.parent_group_id,
            timestamp: self.timestamp,
            entry_count: self.entry_count,
        }
    }
}

/// 音符创建操作日志（NoteCreate 用轻量 op 替代完整快照）
///
/// 每个 op 仅记录一次铅笔绘制的音符（16 字节值 + track_id），
/// undo 时按值精确定位删除，redo 时按值有序重新插入——
/// 与音符总量解耦，1600W 音符工程不再因合并窗口克隆整轨快照。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreateOp {
    /// 音轨 ID
    pub track_id: u32,
    /// 创建的音符（按值）
    pub note: lumino_midi_model::NoteEvent,
}

/// 音符创建日志条目（合并窗口内连续绘制的音符）
#[derive(Debug, Clone)]
pub struct CreateEntry {
    /// 创建操作列表（按时间正序追加）
    pub ops: Vec<CreateOp>,
    /// 分组 ID
    pub group_id: Option<u64>,
    /// 父分组 ID（分割链）
    pub parent_group_id: Option<u64>,
    /// 操作时间戳
    pub timestamp: Instant,
    /// 该分组内已合并的条目数
    pub entry_count: u32,
}

/// 历史记录条目：完整快照或轻量操作日志
///
/// `Snapshot` 使用 `Box` 包装：`EditorSnapshot` 含大量事件字段（>300B），
/// 装箱避免枚举体积膨胀（clippy::large_enum_variant）。
#[derive(Debug, Clone)]
pub enum HistoryEntry {
    /// 完整状态快照（用于无需增量还原的任意操作回退）。
    Snapshot(Box<super::EditorSnapshot>),
    /// 轻量操作日志（可增量应用/回退）。
    Operation(OperationEntry),
    /// 音符创建日志（增量、极简化，替代 NoteCreate 快照）
    Create(CreateEntry),
}
