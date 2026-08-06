//! 历史记录条目类型
//!
//! 将 `MoveOp`、`OperationEntry`、`HistoryEntry` 从 `history.rs` 拆分出来，
//! 避免单文件超过 400 行。

use std::time::Instant;

use super::OpKind;

/// 移动操作日志（NoteMove 用轻量 op 替代完整快照）
#[derive(Debug, Clone, PartialEq)]
pub struct MoveOp {
    /// 音轨 ID
    pub track_id: u32,
    /// 全局索引起点（含）
    pub range_start: u32,
    /// 全局索引终点（不含）
    pub range_end: u32,
    /// tick 偏移量
    pub delta_tick: i32,
    /// key 偏移量
    pub delta_key: i16,
    /// 同一逻辑操作内的序号
    pub seq: u16,
    /// 范围内音符的原始 tick（用于 undo 精确恢复，尤其是 key/tick 被 clamp 的场景）
    pub original_ticks: Vec<f32>,
    /// 范围内音符的原始 key（用于 undo 精确恢复）
    pub original_keys: Vec<u16>,
}

impl MoveOp {
    /// 返回反向操作（delta 取反，原始位置保持不变）
    pub fn inverse(&self) -> Self {
        Self {
            track_id: self.track_id,
            range_start: self.range_start,
            range_end: self.range_end,
            delta_tick: self.delta_tick.wrapping_neg(),
            delta_key: self.delta_key.wrapping_neg(),
            seq: self.seq,
            original_ticks: self.original_ticks.clone(),
            original_keys: self.original_keys.clone(),
        }
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
/// 每个 op 仅记录一次铅笔绘制的音符（16 字节 + track_id），
/// undo 时按值精确定位删除，redo 时按 tick 有序重新插入——
/// 与音符总量解耦，1600W 音符工程不再因合并窗口克隆整轨快照。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreateOp {
    /// 音轨 ID
    pub track_id: u32,
    /// 创建的音符（tick/key/velocity/channel 全字段，undo 精确匹配）
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
    Snapshot(Box<super::EditorSnapshot>),
    Operation(OperationEntry),
    /// 音符创建日志（增量、极简化，替代 NoteCreate 快照）
    Create(CreateEntry),
}
