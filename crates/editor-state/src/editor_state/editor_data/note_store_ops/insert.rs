//! 批量/单个插入音符操作（降级兼容层）
//!
//! NoteStore insert_bulk 热路径已删除，统一走 document insert_note。
//! 保留签名兼容下游调用。

use super::super::EditorData;
use lumino_note_core::note::Note;

impl EditorData {
    /// 批量插入音符
    ///
    /// 返回插入的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_insert_notes(&mut self, notes: &[Note]) -> usize {
        if notes.is_empty() {
            return 0;
        }

        let mut inserted = 0usize;
        for note in notes {
            if self.insert_note(self.current_track, note.clone()) {
                inserted += 1;
            }
        }
        if inserted > 0 {
            self.mark_current_track_changed();
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
