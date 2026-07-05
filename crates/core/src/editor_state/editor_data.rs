//! 编辑器数据层
//!
//! 负责音符集合、音轨缓存、历史记录、MIDI 文档、CC 和速度点等数据的持久化与管理。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::history::{EditorSnapshot, History};
use crate::midi_types::{CcData, TempoPoint};
use crate::note::Note;

use super::constants::DEFAULT_BPM;
use super::constants::GLUE_PROXIMITY_THRESHOLD;
use super::note_grouping::{self, NoteTuple};

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
    pub document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    pub history: History,
    pub cc_data: CcData,
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

    // ── 历史记录 ──

    /// 将当前状态快照推入历史记录
    pub fn push_history(&mut self) {
        self.history
            .push(EditorSnapshot::new(self.notes.clone(), self.current_track));
    }

    /// 撤销上一次操作
    pub fn undo(&mut self) -> bool {
        let current = EditorSnapshot::new(self.notes.clone(), self.current_track);
        if let Some(snapshot) = self.history.undo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            true
        } else {
            false
        }
    }

    /// 重做上一次撤销的操作
    pub fn redo(&mut self) -> bool {
        let current = EditorSnapshot::new(self.notes.clone(), self.current_track);
        if let Some(snapshot) = self.history.redo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            true
        } else {
            false
        }
    }

    /// 是否可以撤销
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// 是否可以重做
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 同步 notes 到 track_notes 缓存
    pub fn sync_track_notes(&mut self) {
        if self.notes.is_empty() {
            self.track_notes.remove(&self.current_track);
        } else {
            self.track_notes
                .insert(self.current_track, self.notes.clone());
        }
        self.mark_track_notes_changed();
    }

    // ── 音符操作 ──

    /// 通过索引删除单个音符
    pub fn delete_note_by_index(&mut self, index: usize) {
        if index < self.notes.len() {
            self.push_history();
            self.notes.remove(index);
            self.sync_track_notes();
        }
    }

    /// 批量删除选中音符
    pub fn delete_selected_notes(&mut self, selected: &HashSet<usize>) {
        if selected.is_empty() {
            return;
        }
        self.push_history();
        let mut indices: Vec<usize> = selected.iter().copied().collect();
        indices.sort_by(|a, b| b.cmp(a));
        for &i in &indices {
            if i < self.notes.len() {
                self.notes.remove(i);
            }
        }
        self.sync_track_notes();
    }

    /// 返回所有音符索引
    pub fn select_all_notes(&self) -> HashSet<usize> {
        (0..self.notes.len()).collect()
    }

    /// 分割音符
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool {
        if index >= self.notes.len() {
            return false;
        }
        let (note_tick, note_length, key, velocity, channel) = {
            let n = &self.notes[index];
            if split_tick <= n.tick || split_tick >= n.tick + n.length {
                return false;
            }
            (n.tick, n.length, n.key, n.velocity, n.channel)
        };
        self.push_history();
        self.notes.remove(index);
        let right = Note::from_raw(
            split_tick,
            key,
            note_tick + note_length - split_tick,
            velocity,
            channel,
        );
        self.notes.insert(index, right);
        let left = Note::from_raw(note_tick, key, split_tick - note_tick, velocity, channel);
        self.notes.insert(index, left);
        self.sync_track_notes();
        true
    }

    /// 合并选中音符
    pub fn glue_selected_notes(&mut self, selected: &HashSet<usize>) -> usize {
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.is_empty() {
            return 0;
        }
        let selected_notes: Vec<NoteTuple> = sel
            .iter()
            .filter_map(|&i| {
                self.notes
                    .get(i)
                    .map(|n| (i, n.tick, n.key, n.length, n.velocity, n.channel))
            })
            .collect();
        if selected_notes.is_empty() {
            return 0;
        }

        let groups = note_grouping::group_adjacent_notes(&selected_notes, GLUE_PROXIMITY_THRESHOLD);
        if groups.is_empty() {
            return 0;
        }

        self.push_history();
        let mut merged = 0usize;
        for group in &groups {
            let first = &group[0];
            let last = &group[group.len() - 1];
            let merged_tick = first.1;
            let merged_length = (last.1 + last.3) - merged_tick;
            let rm: Vec<usize> = group.iter().map(|n| n.0).collect();
            let mut rm_sorted = rm.clone();
            rm_sorted.sort_by(|a, b| b.cmp(a));
            for &idx in &rm_sorted {
                self.notes.remove(idx);
            }
            let adj = rm[0].min(self.notes.len());
            self.notes.insert(
                adj,
                Note::from_raw(merged_tick, first.2, merged_length, first.4, first.5),
            );
            merged += 1;
        }
        self.sync_track_notes();
        merged
    }

    /// 完成绘制新音符（纯业务逻辑），返回创建的 Note
    pub fn finish_drawing(
        &mut self,
        start_tick: f32,
        key: u16,
        current_tick: f32,
        snap_precision: f32,
        default_note_length: f32,
    ) -> Option<Note> {
        if self.current_track == 0 {
            tracing::debug!("编辑器: Conductor 轨道禁止放置音符");
            return None;
        }
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, default_note_length)
        };
        let length = length.max(snap_precision);
        self.push_history();
        let note = Note::new(tick, key, length);
        self.notes.push_back(note.clone());
        self.track_notes
            .insert(self.current_track, self.notes.clone());
        self.mark_track_notes_changed();
        tracing::debug!(
            "编辑器: 已保存 {} 个音符到音轨 {}",
            self.notes.len(),
            self.current_track
        );
        Some(note)
    }

    /// 计算选择框内的音符索引（委托到 get_notes_in_selection_box，消除重复逻辑）
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> HashSet<usize> {
        self.get_notes_in_selection_box(start_tick, start_key, current_tick, current_key)
            .into_iter()
            .collect()
    }

    /// 获取选择框内的音符索引列表
    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        let ts = start_tick.min(current_tick);
        let te = start_tick.max(current_tick);
        let km = start_key.min(current_key);
        let kx = start_key.max(current_key);
        let mut r = Vec::new();
        for (i, n) in self.notes.iter().enumerate() {
            let ne = n.tick + n.length;
            if n.key >= km && n.key <= kx && n.tick < te && ne > ts {
                r.push(i);
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
