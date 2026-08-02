//! 编辑器状态快照
//!
//! 用于撤销/重做时保存/恢复编辑器状态。

use std::sync::Arc;
use std::time::Instant;

use im::Vector;

use crate::automation::AutomationLane;
use crate::event::{ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent};
use crate::midi_types::TempoPoint;
use crate::note::Note;

use super::OpKind;

/// 编辑器状态快照
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    pub notes: Vector<Note>,
    pub current_track: usize,
    /// 自动化 lane 快照。`Arc` 共享：未修改的 lane 在所有快照间物理共址，
    /// 快照克隆为 O(lane 数) 指针拷贝。编辑路径用 `Arc::make_mut` 写时复制。
    pub automation_lanes: Vec<Arc<AutomationLane>>,
    /// 拍号变化列表（可选，兼容旧快照）。
    pub time_signatures: Option<Vec<(u32, u8, u8)>>,
    /// 调号事件列表。
    pub key_signatures: Option<Vec<KeySignatureEvent>>,
    /// 标记事件列表。
    pub markers: Option<Vec<MarkerEvent>>,
    /// 歌词事件列表。
    pub lyrics: Option<Vec<LyricsEvent>>,
    /// 和弦事件列表。
    pub chords: Option<Vec<ChordEvent>>,
    /// 音色变换事件列表。
    pub program_changes: Option<Vec<ProgramChangeEvent>>,
    /// 速度点列表。
    pub tempo_points: Option<Vec<TempoPoint>>,
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
        notes: Vector<Note>,
        current_track: usize,
        automation_lanes: Vec<Arc<AutomationLane>>,
    ) -> Self {
        Self {
            notes,
            current_track,
            automation_lanes,
            time_signatures: None,
            key_signatures: None,
            markers: None,
            lyrics: None,
            chords: None,
            program_changes: None,
            tempo_points: None,
            group_id: None,
            parent_group_id: None,
            timestamp: Instant::now(),
            op_kind: OpKind::Other,
            entry_count: 1,
        }
    }

    /// 创建带元数据的快照（事件字段用 None）
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
            time_signatures: None,
            key_signatures: None,
            markers: None,
            lyrics: None,
            chords: None,
            program_changes: None,
            tempo_points: None,
            group_id,
            parent_group_id,
            timestamp: Instant::now(),
            op_kind,
            entry_count,
        }
    }
}
