//! 批量/单个插入音符操作

use super::super::EditorData;
use crate::note::Note;

impl EditorData {
    /// 批量插入音符（NoteStore 无 realloc 热路径）
    ///
    /// 返回插入的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_insert_notes(&mut self, notes: &[Note]) -> usize {
        if notes.is_empty() {
            return 0;
        }

        let inserted = if self.note_store_enabled {
            let inserted = self.note_store.insert_bulk(notes);
            self.sync_notes_from_store();
            inserted
        } else {
            for note in notes {
                self.notes.push_back(note.clone());
            }
            notes.len()
        };

        self.sync_track_notes();
        inserted
    }

    /// 单个音符追加（NoteStore 启用时同步到 note_store，避免后续全量重建）
    ///
    /// 返回插入的音符数（0 或 1）。调用方需在调用前 `push_history()`。
    pub fn push_note(&mut self, note: Note) -> usize {
        if self.note_store_enabled {
            self.note_store.push_back(note.clone());
            self.notes.push_back(note);
            self.sync_track_notes();
            1
        } else {
            self.notes.push_back(note);
            self.sync_track_notes();
            1
        }
    }
}
