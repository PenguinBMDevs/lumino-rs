//! 批量/单个插入音符操作（降级兼容层）
//!
//! NoteStore insert_bulk 热路径已删除，统一走 document insert_note。
//! 保留签名兼容下游调用。

use super::super::EditorData;
use lumino_note_core::note::Note;

impl EditorData {
    /// 批量插入音符（O(N+M) 归并，单次重建，内存可控）
    ///
    /// 旧实现逐条 `insert_note` → N 次 COW 深拷 + N 条 `InsertAt` → GPU N 次搬运，
    /// 数百音符即 8GB 拷贝/卡顿。本实现一次性 `Vec<NoteEvent>` 归并，
    /// 峰值仅单块 8MB，`note_delta_dirty=true` 单次全量上传，避免 N 次 GPU 移位。
    /// 返回插入数。调用方需在调用前 `push_history()`（快照 O(块数) 浅拷）。
    pub fn batch_insert_notes(&mut self, notes: &[Note]) -> usize {
        if notes.is_empty() {
            return 0;
        }
        let Some(doc) = self.document.as_mut() else {
            return 0;
        };
        // 零分配转换：Note(f32) → NoteEvent(u32) 批量
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .iter()
            .map(|n| super::super::accessors::note_to_event(n.clone()))
            .collect();
        let inserted = doc.batch_insert_notes(self.current_track, events);
        if inserted > 0 {
            // 批量插入索引散布，增量 InsertAt 需 N 次 GPU 搬运 → 直接全量兜底
            self.note_delta_events.clear();
            self.note_delta_dirty = true;
            self.mark_current_track_changed();
            // mark_current_track_changed 未置 dirty（Some 时豁免洋葱皮），补置主轨 dirty
            self.note_delta_dirty = true;
        }
        inserted
    }

    /// 批量插入已排序音符（免排序，O(N+M)）
    ///
    /// 前置：`notes` 已按 tick 升序。少一次排序，适合 I2M 放置等已排序路径。
    pub fn batch_insert_notes_sorted(&mut self, notes: Vec<lumino_note_core::note::Note>) -> usize {
        if notes.is_empty() {
            return 0;
        }
        let Some(doc) = self.document.as_mut() else {
            return 0;
        };
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .into_iter()
            .map(super::super::accessors::note_to_event)
            .collect();
        let inserted = doc.batch_insert_notes_sorted(self.current_track, events);
        if inserted > 0 {
            self.note_delta_events.clear();
            self.note_delta_dirty = true;
            self.mark_current_track_changed();
            self.note_delta_dirty = true;
        }
        inserted
    }

    /// 批量插入到指定音轨（O(N+M) 归并，内存可控）
    ///
    /// 用于 I2M 放置等多轨批量场景。`notes` 按 tick 归并到 `track_id`，
    /// 单次重建，单次脏标记。返回插入数。调用方需在调用前 `push_history()`。
    pub fn batch_insert_notes_to_track(&mut self, track_id: usize, notes: &[Note]) -> usize {
        if notes.is_empty() {
            return 0;
        }
        let Some(doc) = self.document.as_mut() else {
            return 0;
        };
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .iter()
            .map(|n| super::super::accessors::note_to_event(n.clone()))
            .collect();
        let inserted = doc.batch_insert_notes(track_id, events);
        if inserted > 0 {
            // 多轨批量：若命中当前轨则主轨需全量，其余轨走洋葱皮增量豁免
            if track_id == self.current_track {
                self.note_delta_events.clear();
                self.note_delta_dirty = true;
            }
        }
        inserted
    }

    /// 单个音符追加
    ///
    /// 返回插入的音符数（0 或 1）。调用方需在调用前 `push_history()`。
    pub fn push_note(&mut self, note: Note) -> usize {
        if self.insert_note(self.current_track, note) {
            self.mark_current_track_changed();
            1
        } else {
            0
        }
    }
}
