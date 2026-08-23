//! 共享辅助函数和类型
//!
//! 为 arrangement_ops 子模块提供以下共享资源：
//! - `note_event_to_note`: MIDI NoteEvent → 编辑器 Note
//! - `note_in_rect`: 音符与擦除矩形相交判断
//! - `ClipboardNoteEntry`: 剪贴板音符元组类型别名
//!
//! 2026-08 单一权威源：`track_notes` 缓存已删除，arrangement 操作直接读写
//! document（MidiDocument 唯一权威）。`sync_current_track_after_arrange_op` /
//! `load_missing_tracks_from_document` 的缓存同步语义不复存在，已移除。

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
    .with_id(event.id)
}

/// 判断音符是否与擦除矩形相交（tick 半开区间 [tick_start, tick_end)）。
#[allow(dead_code)]
pub(super) fn note_in_rect(note: &Note, tick_start: f64, tick_end: f64) -> bool {
    let ne = note.tick + note.length;
    note.tick < tick_end as f32 && ne > tick_start as f32
}

/// 判断 NoteEvent 是否与擦除矩形相交（document 版本，u32 tick）
pub(super) fn note_event_in_rect(note: &NoteEvent, tick_start: f64, tick_end: f64) -> bool {
    let s = note.start_tick as f32;
    let e = note.end_tick as f32;
    s < (tick_end as f32) && e > (tick_start as f32)
}

/// 剪贴板音符元组：(track_offset, tick_offset, key_offset, length, velocity, channel)
pub(super) type ClipboardNoteEntry = (u16, f32, u16, f32, u8, u8);
