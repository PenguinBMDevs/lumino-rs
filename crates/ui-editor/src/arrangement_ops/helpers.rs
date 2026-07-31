//! 共享辅助函数和类型
//!
//! 为 arrangement_ops 子模块提供以下共享资源：
//! - `note_event_to_note`: MIDI NoteEvent → 编辑器 Note
//! - `note_in_rect`: 音符与擦除矩形相交判断
//! - `ClipboardNoteEntry`: 剪贴板音符元组类型别名
//! - `sync_current_track_after_arrange_op`: 音轨同步
//! - `load_missing_tracks_from_document`: 延迟加载音轨

use super::Editor;
use crate::note::Note;
use lumino_midi_loader::NoteEvent;

/// 将 MIDI 模型的 NoteEvent 转换为编辑器 Note。
pub(super) fn note_event_to_note(event: &NoteEvent) -> Note {
    Note::from_raw(
        event.start_tick as f32,
        event.key as u16,
        (event.end_tick - event.start_tick) as f32,
        event.velocity,
        event.channel,
    )
}

/// 判断音符是否与擦除矩形相交（tick 半开区间 [tick_start, tick_end)）。
pub(super) fn note_in_rect(note: &Note, tick_start: f64, tick_end: f64) -> bool {
    let ne = note.tick + note.length;
    note.tick < tick_end as f32 && ne > tick_start as f32
}

/// 剪贴板音符元组：(track_offset, tick_offset, key_offset, length, velocity, channel)
pub(super) type ClipboardNoteEntry = (u16, f32, u16, f32, u8, u8);

impl Editor {
    /// 工程走带操作后，若当前音轨受影响则同步 editor_data.notes 与 NoteStore。
    pub(super) fn sync_current_track_after_arrange_op(&mut self, touched: bool) {
        if !touched {
            return;
        }
        let editor_data = &mut self.editor_state.data;
        editor_data.notes = editor_data
            .track_notes
            .get(&editor_data.current_track)
            .cloned()
            .unwrap_or_default();
        if editor_data.is_note_store_enabled() {
            editor_data.sync_note_store();
        }
        self.mark_notes_changed();
    }

    /// 从 MidiDocument 加载尚未被 track_notes 缓存的音轨。
    ///
    /// 加载所有音轨而非仅 selection 覆盖的音轨，因为 ArrangeSelection 存储的是
    /// 视觉音轨位置（侧边栏顺序），而 track_notes 使用文档音轨索引。在默认的
    /// ChannelGrouped 模式下两者不一致，按 selection 筛选会导致错误/缺失音轨加载。
    /// 全量加载后由主循环中的 selection.contains 做 tick/key 层面筛选。
    pub(super) fn load_missing_tracks_from_document(&mut self) {
        let tracks_to_load: Vec<usize> = {
            let editor_data = &self.editor_state.data;
            let Some(doc) = &editor_data.document else {
                return;
            };
            let mut result = Vec::new();
            for track_idx in 0..doc.notes.len() {
                if !editor_data.track_notes.contains_key(&track_idx) {
                    result.push(track_idx);
                }
            }
            result
        };

        if tracks_to_load.is_empty() {
            return;
        }

        let editor_data = &mut self.editor_state.data;
        for track_idx in tracks_to_load {
            let Some(doc) = &editor_data.document else {
                continue;
            };
            let doc_notes = doc.track_notes(track_idx);
            let mut loaded: im::Vector<Note> = im::Vector::new();
            for ne in doc_notes {
                loaded.push_back(note_event_to_note(ne));
            }
            editor_data.track_notes.insert(track_idx, loaded);
        }
        editor_data.mark_track_notes_changed();
    }
}
