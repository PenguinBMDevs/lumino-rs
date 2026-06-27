//! 洋葱皮类型定义

use lumino_core::note::Note;
use lumino_midi_loader::NoteInfo;

/// 洋葱皮音符数据（用于后台生成）
///
/// 可以是 tick 或毫秒单位，取决于 `generate` 是否传了 tempo_table。
#[derive(Debug, Clone, Copy)]
pub struct OnionSkinNote {
    /// 起始时间（tick 或毫秒）
    pub start_tick: u32,
    /// 结束时间（tick 或毫秒）
    pub end_tick: u32,
    /// 起始毫秒（如果已经是毫秒单位）
    pub start_ms: f32,
    /// 结束毫秒
    pub end_ms: f32,
    /// MIDI key (0-127)
    pub key: u8,
    /// RGBA 颜色
    pub color: [u8; 4],
}

impl OnionSkinNote {
    /// 从 NoteInfo 创建（tick 单位）
    pub fn from_note_info(note: &NoteInfo, color: [u8; 4]) -> Self {
        Self {
            start_tick: note.start_tick,
            end_tick: note.end_tick(),
            start_ms: 0.0,
            end_ms: 0.0,
            key: note.key,
            color,
        }
    }

    /// 从 NoteEvent 创建（tick 单位）
    pub fn from_note_event(note: &lumino_midi_loader::NoteEvent, color: [u8; 4]) -> Self {
        Self {
            start_tick: note.start_tick,
            end_tick: note.end_tick(),
            start_ms: 0.0,
            end_ms: 0.0,
            key: note.key,
            color,
        }
    }

    /// 从 Note 创建（tick 单位）
    pub fn from_note(note: &Note, color: [u8; 4]) -> Self {
        Self {
            start_tick: note.tick as u32,
            end_tick: (note.tick + note.length) as u32,
            start_ms: 0.0,
            end_ms: 0.0,
            key: note.key as u8,
            color,
        }
    }

    /// 从毫秒数据创建
    pub fn from_ms(start_ms: f32, end_ms: f32, key: u8, color: [u8; 4]) -> Self {
        Self {
            start_tick: 0,
            end_tick: 0,
            start_ms,
            end_ms,
            key,
            color,
        }
    }
}
