//! 编辑器状态快照
//!
//! 用于撤销/重做时保存/恢复编辑器状态。
//!
//! 2026-08 单一权威源改造：音符快照从 `im::Vector<Note>` 改为 `Arc<Vec<NoteEvent>>`。
//! 2026-08 内存修复：从 `Arc<Vec<NoteEvent>>` 改为 `Arc<ChunkedList<NoteEvent>>`。
//! - `Arc` 共享：未修改的轨道数据在所有快照间物理共址，快照克隆为 O(块数) 指针拷贝
//! - `ChunkedList` 块级 COW：整轨不再持有超大单一 Vec，快照/恢复均为 O(块数)
//! - `NoteEvent`（midi-model）：与 MidiDocument 轨道存储同构，恢复时零转换写回
//! - 与 `automation_lanes` 的 `Vec<Arc<AutomationLane>>` 模式一致

use std::sync::Arc;
use std::time::Instant;

use lumino_midi_model::{ChunkedList, NoteEvent};

use crate::automation::AutomationLane;
use crate::midi_types::TempoPoint;

use super::OpKind;

/// 快照对应的按值音符变更（供派生缓存增量回放）。
///
/// `old`/`new` 二者其一为 `None` 表达插入/删除；`old == new` 不记录。
/// 由编辑层在推入历史时附带，undo/redo 可直接按方向反向/正向回放，
/// 无需对整轨做全量差分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotNoteChange {
    /// 文档音轨索引
    pub track: usize,
    /// 变更前音符（插入时为 `None`）
    pub old: Option<NoteEvent>,
    /// 变更后音符（删除时为 `None`）
    pub new: Option<NoteEvent>,
}

/// 编辑器状态快照
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    /// 音符快照（按 start_tick 升序，与 MidiDocument 轨道同构）。
    /// `Arc<ChunkedList>` 共享：未修改的块数据在所有快照间物理共享，
    /// 快照克隆为 O(块数) 指针拷贝。编辑路径经 `ChunkedList` 块级 COW 写时复制
    /// （只复制被修改的目标块）。
    pub notes: Arc<ChunkedList<NoteEvent>>,
    /// 当前激活的音轨索引。
    pub current_track: usize,
    /// 自动化 lane 快照。`Arc` 共享：未修改的 lane 在所有快照间物理共址，
    /// 快照克隆为 O(lane 数) 指针拷贝。编辑路径用 `Arc::make_mut` 写时复制。
    pub automation_lanes: Vec<Arc<AutomationLane>>,
    /// 拍号变化列表（可选，兼容旧快照）。
    pub time_signatures: Option<Vec<(u32, u8, u8)>>,
    /// 速度点列表。
    pub tempo_points: Option<Vec<TempoPoint>>,
    /// 本次快照与相邻状态之间的按值音符变更（供派生缓存增量回放）。
    pub note_changes: Vec<SnapshotNoteChange>,
    /// 变更无法用逐音符原子变更表达（批量结构变化）时置位。
    pub note_changes_full: bool,
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
    /// 创建快照（向后兼容，事件字段用 None 表示不恢复）
    pub fn new(
        notes: Arc<ChunkedList<NoteEvent>>,
        current_track: usize,
        automation_lanes: Vec<Arc<AutomationLane>>,
    ) -> Self {
        Self {
            notes,
            current_track,
            automation_lanes,
            time_signatures: None,
            tempo_points: None,
            note_changes: Vec::new(),
            note_changes_full: false,
            group_id: None,
            parent_group_id: None,
            timestamp: Instant::now(),
            op_kind: OpKind::Other,
            entry_count: 1,
        }
    }

    /// 创建带元数据的快照（事件字段用 None）
    pub fn with_metadata(
        notes: Arc<ChunkedList<NoteEvent>>,
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
            time_signatures: None,
            tempo_points: None,
            note_changes: Vec::new(),
            note_changes_full: false,
            group_id,
            parent_group_id,
            timestamp: Instant::now(),
            op_kind,
            entry_count,
        }
    }
}
