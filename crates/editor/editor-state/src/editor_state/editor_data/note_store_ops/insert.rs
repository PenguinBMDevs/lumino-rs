//! 批量插入音符并返回已分配 id（NoteStore 兼容层清理后的残留子集）
//!
//! 无生产调用方的旧签名（`batch_insert_notes` / `_sorted` / `_to_track` /
//! `push_note`）已在 2026-09 死路径清理中删除；仅保留返回 id 的变体
//! （粘贴/复制/协作落盘需要真实 id 用于广播）。

use super::super::EditorData;
use lumino_note_core::note::Note;

impl EditorData {
    /// 批量插入并返回已分配 id（按输入顺序），供粘贴广播免去 `note_id_at` 全轨重扫。
    ///
    /// 消除粘贴路径 O(N·M) 悬崖（N 粘贴 / M 轨已有）。仅当前轨触发主轨全量脏标记。
    pub fn batch_insert_notes_with_ids(&mut self, notes: &[Note]) -> Vec<u64> {
        if notes.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.document.as_mut() else {
            return Vec::new();
        };
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .iter()
            .map(|n| super::super::accessors::note_to_event(n.clone()))
            .collect();
        let ids = doc.batch_insert_notes_with_ids(self.current_track, events);
        if !ids.is_empty() {
            self.note_delta_events.clear();
            self.note_delta_dirty = true;
            self.mark_current_track_changed();
            self.note_delta_dirty = true;
        }
        ids
    }

    /// 批量插入到指定音轨并返回已分配 id（按输入顺序），供粘贴广播免去全轨重扫。
    ///
    /// 仅当 `track_id == current_track` 时触发主轨全量脏标记（其余轨走洋葱皮增量豁免）。
    pub fn batch_insert_notes_to_track_with_ids(
        &mut self,
        track_id: usize,
        notes: &[Note],
    ) -> Vec<u64> {
        if notes.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.document.as_mut() else {
            return Vec::new();
        };
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .iter()
            .map(|n| super::super::accessors::note_to_event(n.clone()))
            .collect();
        let ids = doc.batch_insert_notes_with_ids(track_id, events);
        if !ids.is_empty() && track_id == self.current_track {
            self.note_delta_events.clear();
            self.note_delta_dirty = true;
        }
        ids
    }
}
