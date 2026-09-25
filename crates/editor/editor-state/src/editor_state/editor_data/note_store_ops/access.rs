//! 音符只读视图访问器（NoteStore 兼容层清理后的残留子集）
//!
//! `NoteStore` 已删除，访问器直接遍历 document 当前轨（NoteEvent → NoteView 转换）。
//! 无生产调用方的 `for_each_note_view` 已在 2026-09 死路径清理中删除。

use super::super::EditorData;

impl EditorData {
    /// 获取音符只读视图（从 document 当前轨转换）
    pub fn get_note_view(&self, idx: usize) -> Option<lumino_note_core::note_store::NoteView> {
        self.current_track_notes()
            .get(idx)
            .map(|note| lumino_note_core::note_store::NoteView {
                tick: note.start_tick as f32,
                key: note.key as u16,
                length: (note.end_tick - note.start_tick) as f32,
                velocity: note.velocity,
                channel: note.channel,
            })
    }
}
