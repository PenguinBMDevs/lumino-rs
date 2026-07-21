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
#[cfg(test)]
use crate::midi_types::PITCH_BEND_CENTER;
use crate::midi_types::{CcData, TempoPoint};
use crate::note::Note;
use crate::note_store::NoteStore;

use super::constants::DEFAULT_BPM;

pub(crate) mod async_commit;
pub(crate) mod async_commit_streaming;
mod automation;
mod history;
mod note_store_ops;
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
    /// 异步提交的待完成状态（MoveOp 后台应用）
    pub(crate) pending_commit: Option<async_commit::PendingCommit>,
    pub cc_data: CcData,
    /// 自动化 lane 列表。`Arc` 使撤销快照可 O(1) 共享未修改的 lane；
    /// 修改 lane 前必须经 `Arc::make_mut`（见 editor_data/automation.rs）。
    /// lane 数量通常 ≤50，`Vec` 索引写 O(1)。
    pub automation_lanes: Vec<Arc<AutomationLane>>,
    pub tempo_points: Vec<TempoPoint>,
    /// 高性能 SoA 音符存储（与 `notes` 并存，用于批量操作热路径）
    ///
    /// 当音符数超过 `NOTE_STORE_THRESHOLD` 时自动启用：
    /// - 批量移动走 `batch_move_parallel`（8 线程并行，16M 50% 18ms）
    /// - 批量删除走 `delete_selected`（O(N) 单次遍历）
    /// - 批量插入走 `insert_bulk`（无 realloc，1ms/1000 音符）
    ///
    /// 启用后 `notes` 仍作为权威源，`note_store` 通过 `sync_note_store()` 同步。
    /// 后续迁移完成后 `notes` 将退化为 `note_store` 的视图。
    pub note_store: NoteStore,
    /// note_store 启用阈值（音符数低于此值时不启用，避免小数据量开销）
    pub note_store_enabled: bool,
}

/// NoteStore 启用阈值：音符数超过此值时自动启用 SoA 批量操作
pub const NOTE_STORE_THRESHOLD: usize = 10_000;

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
            pending_commit: None,
            cc_data: CcData::default(),
            automation_lanes: Vec::new(),
            tempo_points: vec![TempoPoint {
                tick: 0.0,
                bpm: DEFAULT_BPM,
            }],
            note_store: NoteStore::new(),
            note_store_enabled: false,
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
        self.pending_commit = None;
        self.document = None;
        self.cc_data = CcData::default();
        self.automation_lanes.clear();
        self.tempo_points = vec![TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        }];
        self.note_store.clear();
        self.note_store_enabled = false;
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
        assert!(
            data.find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
                .is_none()
        );
    }

    #[test]
    fn test_find_or_create_automation_lane_creates_new() {
        let mut data = EditorData::new();
        let idx = data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
        assert_eq!(idx, 0, "first lane gets index 0");
        assert_eq!(data.automation_lanes.len(), 1);
        assert_eq!(
            data.automation_lanes[0].target,
            AutomationTarget::CC { controller: 7 }
        );
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
            channel: 0,
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
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });
        let replaced = data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 127,
            shape: SegmentShape::Curve { tension: 0 },
        });
        assert!(replaced);
        assert_eq!(
            data.automation_lanes[0].events.len(),
            1,
            "same tick replaces"
        );
        assert_eq!(data.automation_lanes[0].events[0].value, 127);
    }

    #[test]
    fn test_apply_automation_edit_move() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });
        let moved = data.apply_automation_edit(AutomationEdit::Move {
            track_idx: 0,
            lane_idx: 0,
            old_tick: 100,
            new_tick: 200,
            new_value: 32,
        });
        assert!(moved);
        assert_eq!(data.automation_lanes[0].events[0].tick, 200);
        assert_eq!(data.automation_lanes[0].events[0].value, 32);
    }

    #[test]
    fn test_apply_automation_edit_cycle_shape() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });
        let cycled = data.apply_automation_edit(AutomationEdit::CycleShape {
            track_idx: 0,
            lane_idx: 0,
            tick: 100,
        });
        assert!(cycled);
        assert_eq!(
            data.automation_lanes[0].events[0].shape,
            SegmentShape::Curve { tension: 0 }
        );

        let cycled2 = data.apply_automation_edit(AutomationEdit::CycleShape {
            track_idx: 0,
            lane_idx: 0,
            tick: 100,
        });
        assert!(cycled2);
        assert_eq!(data.automation_lanes[0].events[0].shape, SegmentShape::Step);
    }

    #[test]
    fn test_apply_automation_edit_delete() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });
        let deleted = data.apply_automation_edit(AutomationEdit::Delete {
            track_idx: 0,
            lane_idx: 0,
            tick: 100,
        });
        assert!(deleted);
        assert!(data.automation_lanes[0].events.is_empty());

        let deleted2 = data.apply_automation_edit(AutomationEdit::Delete {
            track_idx: 0,
            lane_idx: 0,
            tick: 999,
        });
        assert!(!deleted2);
    }

    #[test]
    fn test_apply_automation_edit_move_wrong_track_returns_false() {
        let mut data = EditorData::new();
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });
        let moved = data.apply_automation_edit(AutomationEdit::Move {
            track_idx: 1,
            lane_idx: 0,
            old_tick: 100,
            new_tick: 200,
            new_value: 32,
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

    // ── tie_selected_notes 测试 ──

    #[test]
    fn test_tie_selected_notes_same_key_adjacent() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        data.notes.push_back(Note::new(3.0, 60, 3.0));
        let tied = data.tie_selected_notes(&HashSet::from([0, 1]));
        assert_eq!(tied, 1, "should tie one pair");
        assert_eq!(data.notes.len(), 2, "notes count unchanged");
        assert_eq!(
            data.notes[0].length, 3.0,
            "first note extends to second's start"
        );
        assert_eq!(data.notes[1].length, 3.0, "last note unchanged");
    }

    #[test]
    fn test_tie_selected_notes_three_notes() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        data.notes.push_back(Note::new(4.0, 60, 2.0));
        data.notes.push_back(Note::new(8.0, 60, 3.0));
        let tied = data.tie_selected_notes(&HashSet::from([0, 1, 2]));
        assert_eq!(tied, 2, "should tie two pairs");
        assert_eq!(data.notes[0].length, 4.0, "first note extends to second");
        assert_eq!(data.notes[1].length, 4.0, "second note extends to third");
        assert_eq!(data.notes[2].length, 3.0, "last note unchanged");
    }

    #[test]
    fn test_tie_selected_notes_different_key_still_ties() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        data.notes.push_back(Note::new(3.0, 61, 3.0));
        let tied = data.tie_selected_notes(&HashSet::from([0, 1]));
        assert_eq!(tied, 1, "different keys should still tie by tick order");
        assert_eq!(
            data.notes[0].length, 3.0,
            "first note extends to second's start"
        );
        assert_eq!(data.notes[1].length, 3.0, "last note unchanged");
    }

    #[test]
    fn test_tie_selected_notes_overlapping_notes_not_shortened() {
        // Note 0 starts at 0, ends at 10. Note 1 starts at 3 (overlap).
        // Tie 不应缩短 Note 0，因为重叠不算间隙。
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 10.0));
        data.notes.push_back(Note::new(3.0, 61, 10.0));
        let tied = data.tie_selected_notes(&HashSet::from([0, 1]));
        assert_eq!(tied, 0, "overlapping notes should not be tied");
        assert_eq!(data.notes[0].length, 10.0, "first note not shortened");
        assert_eq!(data.notes[1].length, 10.0, "second note unchanged");
    }

    #[test]
    fn test_tie_selected_notes_single_note() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        let tied = data.tie_selected_notes(&HashSet::from([0]));
        assert_eq!(tied, 0, "single note cannot tie");
    }

    #[test]
    fn test_tie_selected_notes_empty_selection() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        assert_eq!(data.tie_selected_notes(&HashSet::new()), 0);
    }

    #[test]
    fn test_tie_selected_notes_mixed_keys() {
        let mut data = EditorData::new();
        data.notes.push_back(Note::new(0.0, 60, 2.0));
        data.notes.push_back(Note::new(3.0, 60, 2.0));
        data.notes.push_back(Note::new(6.0, 61, 2.0));
        data.notes.push_back(Note::new(9.0, 61, 3.0));
        let tied = data.tie_selected_notes(&HashSet::from([0, 1, 2, 3]));
        // All 4 notes sorted by tick: note0→note1→note2→note3
        // 3 ties: note0→note1, note1→note2, note2→note3
        assert_eq!(tied, 3, "should tie all consecutive pairs by tick order");
        assert_eq!(data.notes[0].length, 3.0, "note0 extends to note1");
        assert_eq!(data.notes[1].length, 3.0, "note1 extends to note2");
        assert_eq!(data.notes[2].length, 3.0, "note2 extends to note3");
        assert_eq!(data.notes[3].length, 3.0, "last note unchanged");
    }

    #[test]
    fn test_tie_selected_notes_same_tick_group_extends_to_next_tick() {
        // 模拟用户场景：第一小节放置多个不同 Key 的音符，
        // 空一小节，第三小节放置另一组不同 Key 的音符。
        // 选中所有音符后，第一小节的**全部**音符都应延长到第三小节开头。
        let mut data = EditorData::new();
        // 第一小节：tick 0，三个不同 Key 的音符，长度 4.0（完整小节）
        data.notes.push_back(Note::new(0.0, 60, 4.0));
        data.notes.push_back(Note::new(0.0, 61, 4.0));
        data.notes.push_back(Note::new(0.0, 62, 4.0));
        // 第三小节：tick 8.0，另一组不同 Key 的音符
        data.notes.push_back(Note::new(8.0, 70, 4.0));
        data.notes.push_back(Note::new(8.0, 71, 4.0));
        data.notes.push_back(Note::new(8.0, 72, 4.0));

        let tied = data.tie_selected_notes(&HashSet::from([0, 1, 2, 3, 4, 5]));
        assert_eq!(
            tied, 3,
            "all measure-1 notes should extend to measure-3 start"
        );
        assert_eq!(data.notes[0].length, 8.0, "note0 extends to tick 8");
        assert_eq!(data.notes[1].length, 8.0, "note1 extends to tick 8");
        assert_eq!(data.notes[2].length, 8.0, "note2 extends to tick 8");
        assert_eq!(data.notes[3].length, 4.0, "measure-3 note0 unchanged");
        assert_eq!(data.notes[4].length, 4.0, "measure-3 note1 unchanged");
        assert_eq!(data.notes[5].length, 4.0, "measure-3 note2 unchanged");
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

    // ── COW / Arc 共享测试 ──

    #[test]
    fn test_automation_lane_cow_shares_unmodified_lanes() {
        let mut data = EditorData::new();
        data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
        data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 1 });

        // 快照——所有 lane 的 Arc refcount +1
        data.push_history();

        // 记录 lane 0 的 Arc 地址
        let lane0_ptr = Arc::as_ptr(&data.automation_lanes[0]);

        // 修改 lane 1——只有 lane 1 触发 COW（Arc::make_mut 复制 lane 1）
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 1 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });

        // lane 0 未被修改→地址不变（物理共享）
        assert_eq!(
            lane0_ptr,
            Arc::as_ptr(&data.automation_lanes[0]),
            "未修改的 lane 必须在快照前后共享同一 Arc 分配"
        );
        // lane 0 的数据也不变
        assert_eq!(
            data.automation_lanes[0].target,
            AutomationTarget::CC { controller: 7 }
        );
    }

    #[test]
    fn test_automation_lane_undo_restores_data() {
        let mut data = EditorData::new();
        data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
        });

        // 快照（1 lane, 1 event）
        data.push_history();

        // 添加第二个事件
        data.apply_automation_edit(AutomationEdit::Add {
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 200,
            value: 127,
            shape: SegmentShape::Step,
        });
        assert_eq!(data.automation_lanes[0].events.len(), 2);

        // 撤销——回到 1 event
        assert!(data.undo());
        assert_eq!(data.automation_lanes[0].events.len(), 1);
        assert_eq!(data.automation_lanes[0].events[0].tick, 100);

        // 重做——回到 2 events
        assert!(data.redo());
        assert_eq!(data.automation_lanes[0].events.len(), 2);
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
            track_idx: 0,
            target: AutomationTarget::CC { controller: 7 },
            channel: 0,
            tick: 100,
            value: 64,
            shape: SegmentShape::Step,
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
            track_idx: 0,
            target: AutomationTarget::PitchBend,
            channel: 0,
            tick: 100,
            value: PITCH_BEND_CENTER as u16,
            shape: SegmentShape::Curve { tension: 0 },
        });
        let points = data.build_bend_points();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].tick, 100.0);
        assert_eq!(points[0].value, 0, "PITCH_BEND_CENTER → center = 0");
    }
}
