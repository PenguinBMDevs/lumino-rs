//! 编辑器数据层（结构定义 + 构造 + 测试）
//!
//! 方法实现已拆分为同级子模块：
//! - `automation`：自动化 lane 管理、编辑与导出
//! - `notes`：音符 CRUD、分割、合并、选择框
//! - `history`：Undo/Redo 历史记录

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::automation::AutomationLane;
use crate::history::History;
use crate::midi_types::{CcData, TempoPoint};
use crate::note::Note;

use super::constants::DEFAULT_BPM;

mod automation;
mod history;
mod notes;

/// 编辑器数据
#[derive(Debug)]
pub struct EditorData {
    pub notes: im::Vector<Note>,
    pub current_track: usize,
    pub track_notes: HashMap<usize, im::Vector<Note>>,
    /// 递增版本号，track_notes 每次变化时 bump。
    /// 用于 NoteWorker 快照的 Arc 缓存失效检测，避免每帧全量克隆 HashMap。
    pub track_notes_gen: u64,
    /// 被编辑过的音轨集合（用于协作同步，记录需要广播变更的所有音轨）
    pub edited_tracks: HashSet<usize>,
    pub document: Option<Arc<lumino_midi_model::MidiDocument>>,
    pub history: History,
    pub cc_data: CcData,
    /// 自动化事件 lane 列表（从 yinhe 移植的曲线/CC/Bend/RPN/NRPN 数据模型）。
    pub automation_lanes: Vec<AutomationLane>,
    pub tempo_points: Vec<TempoPoint>,
}

impl Default for EditorData {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorData {
    /// 创建新的编辑器数据实例
    pub fn new() -> Self {
        Self {
            notes: im::Vector::new(),
            current_track: 0,
            track_notes: HashMap::new(),
            track_notes_gen: 0,
            edited_tracks: HashSet::new(),
            document: None,
            history: History::new(),
            cc_data: CcData::default(),
            automation_lanes: Vec::new(),
            tempo_points: vec![TempoPoint {
                tick: 0.0,
                bpm: DEFAULT_BPM,
            }],
        }
    }

    /// 重置编辑器数据到初始状态（释放所有内存）
    pub fn reset(&mut self) {
        self.notes.clear();
        self.track_notes.clear();
        self.edited_tracks.clear();
        self.mark_track_notes_changed();
        self.current_track = 0;
        self.history.clear();
        self.document = None;
        self.cc_data = CcData::default();
        self.automation_lanes.clear();
        self.tempo_points = vec![TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        }];
    }

    /// 标记 track_notes 已变化（递增版本号）
    ///
    /// 所有直接修改 `self.track_notes` 的地方都必须在操作后调用此方法，
    /// 否则 NoteWorker 快照缓存无法感知数据变化。
    #[inline]
    pub fn mark_track_notes_changed(&mut self) {
        self.track_notes_gen = self.track_notes_gen.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationEdit, AutomationTarget, SegmentShape};

    #[test]
    fn test_editor_data_default() {
        let data = EditorData::default();
        assert!(data.notes.is_empty());
        assert_eq!(data.current_track, 0);
        assert_eq!(data.track_notes_gen, 0);
        assert!(data.document.is_none());
    }

    #[test]
    fn test_editor_data_new() {
        let data = EditorData::new();
        assert_eq!(data.tempo_points.len(), 1);
        assert_eq!(data.tempo_points[0].bpm, DEFAULT_BPM);
    }

    #[test]
    fn test_reset_clears_data() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        data.track_notes.insert(1, data.notes.clone());
        data.reset();
        assert!(data.notes.is_empty());
        assert!(data.track_notes.is_empty());
        assert_eq!(data.track_notes_gen, 1);
    }

    #[test]
    fn test_mark_track_notes_changed() {
        let mut data = EditorData::new();
        data.mark_track_notes_changed();
        assert_eq!(data.track_notes_gen, 1);
    }

    #[test]
    fn test_select_all_notes() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        data.notes.push_back(Note::new(1.0, 62, 1.0));
        let selected = data.select_all_notes();
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_get_notes_in_selection_box() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        data.notes.push_back(Note::new(5.0, 62, 1.0));

        let indices = data.get_notes_in_selection_box(-1.0, 59, 3.0, 61);
        assert_eq!(indices.len(), 1);
        assert_eq!(indices[0], 0);
    }

    #[test]
    fn test_compute_selection() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        let selected = data.compute_selection(-1.0, 59, 3.0, 61);
        assert_eq!(selected.len(), 1);
        assert!(selected.contains(&0));
    }

    // ── automation lane 测试 ──

    #[test]
    fn test_find_automation_lane_returns_none() {
        let data = EditorData::new();
        assert!(data.find_automation_lane(0, &AutomationTarget::CC { controller: 7 }).is_none());
    }

    #[test]
    fn test_find_or_create_automation_lane_creates_new() {
        let mut data = EditorData::new();
        let idx = data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
        assert_eq!(idx, 0, "first lane gets index 0");
        assert_eq!(data.automation_lanes.len(), 1);
        assert_eq!(data.automation_lanes[0].target, AutomationTarget::CC { controller: 7 });
        assert_eq!(data.automation_lanes[0].track, 0);
    }

    #[test]
    fn test_find_or_create_automation_lane_reuses_existing() {
        let mut data = EditorData::new();
        let idx1 = data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
        let idx2 = data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
        assert_eq!(idx1, idx2, "same lane should be reused");
        assert_eq!(data.automation_lanes.len(), 1);
    }

    // ── apply_automation_edit 测试 ──

    #[test]
    fn test_apply_automation_edit_add() {
        let mut data = EditorData::new();
        let added = data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });
        assert!(added);
        assert_eq!(data.automation_lanes.len(), 1);
        assert_eq!(data.automation_lanes[0].events.len(), 1);
        assert_eq!(data.automation_lanes[0].events[0].tick, 100);
        assert_eq!(data.automation_lanes[0].events[0].value, 64);
    }

    #[test]
    fn test_apply_automation_edit_add_duplicate_tick_replaces() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::CC { controller: 7 },
            tick: 100, value: 64, shape: SegmentShape::Step,
        });
        let replaced = data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::CC { controller: 7 },
            tick: 100, value: 127, shape: SegmentShape::Curve { tension: 0 },
        });
        assert!(replaced);
        assert_eq!(data.automation_lanes[0].events.len(), 1, "same tick replaces");
        assert_eq!(data.automation_lanes[0].events[0].value, 127);
    }

    #[test]
    fn test_apply_automation_edit_move() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::CC { controller: 7 },
            tick: 100, value: 64, shape: SegmentShape::Step,
        });
        let moved = data.apply_automation_edit(AutomationEdit::Move {
            track_idx: 0, lane_idx: 0, old_tick: 100, new_tick: 200, new_value: 32,
        });
        assert!(moved);
        assert_eq!(data.automation_lanes[0].events[0].tick, 200);
        assert_eq!(data.automation_lanes[0].events[0].value, 32);
    }

    #[test]
    fn test_apply_automation_edit_cycle_shape() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::CC { controller: 7 },
            tick: 100, value: 64, shape: SegmentShape::Step,
        });
        let cycled = data.apply_automation_edit(AutomationEdit::CycleShape {
            track_idx: 0, lane_idx: 0, tick: 100,
        });
        assert!(cycled);
        assert_eq!(data.automation_lanes[0].events[0].shape, SegmentShape::Curve { tension: 0 });

        let cycled2 = data.apply_automation_edit(AutomationEdit::CycleShape {
            track_idx: 0, lane_idx: 0, tick: 100,
        });
        assert!(cycled2);
        assert_eq!(data.automation_lanes[0].events[0].shape, SegmentShape::Step);
    }

    #[test]
    fn test_apply_automation_edit_delete() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::CC { controller: 7 },
            tick: 100, value: 64, shape: SegmentShape::Step,
        });
        let deleted = data.apply_automation_edit(AutomationEdit::Delete {
            track_idx: 0, lane_idx: 0, tick: 100,
        });
        assert!(deleted);
        assert!(data.automation_lanes[0].events.is_empty());

        let deleted2 = data.apply_automation_edit(AutomationEdit::Delete {
            track_idx: 0, lane_idx: 0, tick: 999,
        });
        assert!(!deleted2);
    }

    #[test]
    fn test_apply_automation_edit_move_wrong_track_returns_false() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::CC { controller: 7 },
            tick: 100, value: 64, shape: SegmentShape::Step,
        });
        let moved = data.apply_automation_edit(AutomationEdit::Move {
            track_idx: 1, lane_idx: 0, old_tick: 100, new_tick: 200, new_value: 32,
        });
        assert!(!moved, "should reject move with mismatched track");
    }

    // ── split_note 测试 ──

    #[test]
    fn test_split_note_success() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 4.0));
        let result = data.split_note(0, 2.0);
        assert!(result, "split at middle should succeed");
        assert_eq!(data.notes.len(), 2, "one note becomes two");
        assert_eq!(data.notes[0].tick, 0.0, "left half start tick");
        assert_eq!(data.notes[0].length, 2.0, "left half length");
        assert_eq!(data.notes[1].tick, 2.0, "right half start tick");
        assert_eq!(data.notes[1].length, 2.0, "right half length");
    }

    #[test]
    fn test_split_note_invalid_index() {
        let mut data = EditorData::new();
        assert!(!data.split_note(0, 1.0), "empty notes → false");
    }

    #[test]
    fn test_split_note_at_boundary() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 4.0));
        assert!(!data.split_note(0, 0.0), "split at start tick = false");
        assert!(!data.split_note(0, 4.0), "split at end tick = false");
    }

    // ── glue_selected_notes 测试 ──

    #[test]
    fn test_glue_selected_notes_adjacent() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        data.notes.push_back(Note::new(2.0, 60, 3.0));
        let merged = data.glue_selected_notes(&HashSet::from([0, 1]));
        assert_eq!(merged, 1, "should merge one pair");
        assert_eq!(data.notes.len(), 1, "two notes become one");
        assert_eq!(data.notes[0].tick, 0.0);
        assert_eq!(data.notes[0].length, 5.0, "merged length = sum");
    }

    #[test]
    fn test_glue_selected_notes_non_adjacent() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        data.notes.push_back(Note::new(5.0, 60, 3.0));
        let merged = data.glue_selected_notes(&HashSet::from([0, 1]));
        assert_eq!(merged, 0, "non-adjacent notes with gap should not merge");
        assert_eq!(data.notes.len(), 2, "notes unchanged");
    }

    #[test]
    fn test_glue_selected_notes_empty_selection() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        assert_eq!(data.glue_selected_notes(&HashSet::new()), 0);
    }

    // ── undo / redo 测试 ──

    #[test]
    fn test_undo_redo_basic() {
        let mut data = EditorData::new();
        data.push_history();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        assert_eq!(data.notes.len(), 1);
        assert!(data.can_undo());

        let undone = data.undo();
        assert!(undone);
        assert!(data.notes.is_empty(), "undo should restore empty notes");
        assert!(data.can_redo());

        let redone = data.redo();
        assert!(redone);
        assert_eq!(data.notes.len(), 1, "redo should restore the note");
    }

    #[test]
    fn test_undo_when_nothing_to_undo() {
        let mut data = EditorData::new();
        assert!(!data.can_undo());
        assert!(!data.undo(), "undo on empty history = false");
    }

    // ── build_cc_points / build_bend_points 测试 ──

    #[test]
    fn test_build_cc_points_empty() {
        let data = EditorData::new();
        let points = data.build_cc_points(7);
        assert!(points.is_empty());
    }

    #[test]
    fn test_build_cc_points_with_data() {
        let mut data = EditorData::new();
        data.current_track = 0;
        data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::CC { controller: 7 },
            tick: 100, value: 64, shape: SegmentShape::Step,
        });
        let points = data.build_cc_points(7);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].tick, 100.0);
        assert_eq!(points[0].value, 64);
    }

    #[test]
    fn test_build_bend_points_with_data() {
        let mut data = EditorData::new();
        data.current_track = 0;
        data.find_or_create_automation_lane(0, AutomationTarget::PitchBend);
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0, target: AutomationTarget::PitchBend,
            tick: 100, value: 8192, shape: SegmentShape::Curve { tension: 0 },
        });
        let points = data.build_bend_points();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].tick, 100.0);
        assert_eq!(points[0].value, 0, "8192 → center = 0");
    }
}
