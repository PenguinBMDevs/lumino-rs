//! 音符只读视图与位集工具
//!
//! 2026-08 单一权威源改造：SoA `NoteStore`（分块存储 + 墓碑删除 + 批量并行操作）
//! 已整体删除，音符数据唯一权威是 `MidiDocument`。本模块仅保留仍被下游使用的类型：
//! - `NoteView`：音符只读视图（Copy 语义，避免构造 Note 结构体的开销）
//! - `BitSet`：自实现位集（选中状态 / 标记位）

mod bitset;

pub use bitset::BitSet;

use crate::note::Note;

/// 音符只读视图（避免构造 Note 结构体的开销）
#[derive(Debug, Clone, Copy)]
pub struct NoteView {
    pub tick: f32,
    pub key: u16,
    pub length: f32,
    pub velocity: u8,
    pub channel: u8,
}

impl From<Note> for NoteView {
    fn from(note: Note) -> Self {
        Self {
            tick: note.tick,
            key: note.key,
            length: note.length,
            velocity: note.velocity,
            channel: note.channel,
        }
    }
}

impl From<&Note> for NoteView {
    /// 从 &Note 零 clone 构造 NoteView（字段全部 Copy）
    ///
    /// 用于 im::Vector 路径下 `for_each_note_view` 等场景，避免先 clone Note
    /// 再消耗的冗余开销。
    fn from(note: &Note) -> Self {
        Self {
            tick: note.tick,
            key: note.key,
            length: note.length,
            velocity: note.velocity,
            channel: note.channel,
        }
    }
}

impl From<NoteView> for Note {
    fn from(r: NoteView) -> Self {
        Note::from_raw(r.tick, r.key, r.length, r.velocity, r.channel)
    }
}
