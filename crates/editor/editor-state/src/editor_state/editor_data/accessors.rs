//! 访问器 —— 音符读写访问器 + 增量事件记录
//!
//! 2026-08 单一权威源改造：音符数据唯一权威是 `document`（MidiDocument），
//! 所有读取/写入经本模块访问器。tick 精度：UI 编辑用 f32，写回时
//! 无损转换（`fract() == 0.0` 直接 as u32），异常亚 tick 才 round + warn。

use std::collections::HashSet;

use super::{EditorData, NoteDeltaEvent};
use lumino_midi_model::NoteEvent;
use lumino_note_core::note::Note;

mod marks;
mod read;
mod write;

/// Note（f32 tick）→ NoteEvent（u32 tick）无损转换
///
/// UI 编辑的 tick 全部来自 `snap_tick`（整数网格吸附），正常路径 `fract() == 0.0`。
/// 异常亚 tick（防御性）使用 round 并记录 warn——不引入架构性精度损失。
/// 按值转换，无 ID。
#[inline]
pub fn note_to_event(note: Note) -> NoteEvent {
    let start_tick = f32_to_tick(note.tick);
    let end_tick = f32_to_tick(note.tick + note.length);
    NoteEvent::new(
        start_tick,
        end_tick,
        note.key as u8,
        note.velocity,
        note.channel,
    )
}

/// NoteEvent（u32 tick）→ Note（f32 tick）无损转换（按值，无 ID）
#[inline]
pub fn event_to_note(event: &NoteEvent) -> Note {
    Note::new(
        event.start_tick as f32,
        event.key as u16,
        (event.end_tick - event.start_tick) as f32,
    )
    .with_velocity(event.velocity)
    .with_channel(event.channel)
}

/// f32 tick → u32 tick：无损优先（fract==0），异常亚 tick round + trace
///
/// 注意：非整数 tick 是**设计内预期行为**（图片转 MIDI 区域等比映射会生成
/// 亚 tick 数值），round 是防御性兜底而非异常，因此仅 trace 不 warn——
/// 否则 i2m 批量写入时每音符一条 WARN，高频日志格式化 + 终端 I/O 阻塞主线程。
#[inline]
pub fn f32_to_tick(tick: f32) -> u32 {
    if tick.fract() == 0.0 {
        tick as u32
    } else {
        tracing::trace!("非整数 tick 写回 MIDI: {tick}，已四舍五入");
        tick.round() as u32
    }
}
