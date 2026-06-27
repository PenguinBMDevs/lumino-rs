//! 单条音符表示 — 每个音符一条记录，替代 NoteOn + NoteOff 两个事件。
//!
//! 这是 lumino MIDI 加载的第二刀：把 `CompactEvent` 中拆成两条的音符
//! 合并成 `(start_tick, end_tick, key, velocity, channel)`，内存减半。

use lumino_midi_io::compact::{CompactEvent, EventKind};

use crate::note_info::NoteInfo;

/// 单个音符的自包含表示。
///
/// 与 `CompactEvent` 的 note 事件对相比：
/// - `CompactEvent`: 2 × 12 bytes = 24 bytes / note
/// - `NoteEvent`: 13 bytes（对齐后 16 bytes）/ note
///
/// 后续将逐步用 `NoteEvent` 替代 note 事件对存储。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct NoteEvent {
    /// 音符开始 tick
    pub start_tick: u32,
    /// 音符结束 tick
    pub end_tick: u32,
    /// MIDI key (0-127)
    pub key: u8,
    /// 力度 (0-127)
    pub velocity: u8,
    /// MIDI 通道 (0-15)
    pub channel: u8,
}

impl NoteEvent {
    /// 创建新音符。
    #[inline]
    pub fn new(start_tick: u32, end_tick: u32, key: u8, velocity: u8, channel: u8) -> Self {
        Self {
            start_tick,
            end_tick,
            key,
            velocity,
            channel,
        }
    }

    /// 音符时长（tick 数）。
    #[inline]
    pub fn length(&self) -> u32 {
        self.end_tick.saturating_sub(self.start_tick)
    }

    /// 音符结束 tick（与 `end_tick` 字段等价，方便与 `NoteInfo` 接口兼容）。
    #[inline]
    pub fn end_tick(&self) -> u32 {
        self.end_tick
    }

    /// 转换为 NoteOn + NoteOff 两个 `CompactEvent`。
    ///
    /// 用于尚未迁移到 `NoteEvent` 的下游路径（如音频导出）。
    #[inline]
    pub fn to_compact_events(&self, track_id: u16) -> [CompactEvent; 2] {
        [
            CompactEvent::new(
                self.start_tick,
                track_id,
                EventKind::NoteOn,
                self.channel,
                self.key as u16,
                self.velocity as u16,
            ),
            CompactEvent::new(
                self.end_tick,
                track_id,
                EventKind::NoteOff,
                self.channel,
                self.key as u16,
                self.velocity as u16,
            ),
        ]
    }

    /// 转换为 `NoteInfo`（UI 缓存格式）。
    #[inline]
    pub fn to_note_info(&self) -> NoteInfo {
        NoteInfo::new(
            self.start_tick,
            self.length(),
            self.key,
            self.velocity,
            self.channel,
        )
    }
}

impl From<NoteInfo> for NoteEvent {
    #[inline]
    fn from(info: NoteInfo) -> Self {
        Self {
            start_tick: info.start_tick,
            end_tick: info.end_tick(),
            key: info.key,
            velocity: info.velocity,
            channel: info.channel,
        }
    }
}

impl From<midly::loader::PackedNote> for NoteEvent {
    #[inline]
    fn from(note: midly::loader::PackedNote) -> Self {
        Self {
            start_tick: note.start_tick,
            end_tick: note.end_tick,
            key: note.key,
            velocity: note.velocity,
            // midly::loader::PackedNote 不保存 channel，默认 0。
            // 后续如果 per-track channel 信息可用，应在外部覆写。
            channel: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_event_length() {
        let note = NoteEvent::new(100, 200, 60, 100, 5);
        assert_eq!(note.length(), 100);
    }

    #[test]
    fn test_note_event_to_compact_events() {
        let note = NoteEvent::new(100, 200, 60, 100, 5);
        let [on, off] = note.to_compact_events(3);
        assert_eq!(on.delta_tick(), 100);
        assert_eq!(on.kind(), EventKind::NoteOn);
        assert_eq!(on.param1(), 60);
        assert_eq!(on.param2(), 100);
        assert_eq!(on.channel(), 5);
        assert_eq!(on.track_id(), 3);

        assert_eq!(off.delta_tick(), 200);
        assert_eq!(off.kind(), EventKind::NoteOff);
    }

    #[test]
    fn test_note_event_from_note_info() {
        let info = NoteInfo::new(100, 50, 60, 100, 5);
        let note = NoteEvent::from(info);
        assert_eq!(note.start_tick, 100);
        assert_eq!(note.end_tick, 150);
        assert_eq!(note.key, 60);
        assert_eq!(note.velocity, 100);
        assert_eq!(note.channel, 5);
    }

    #[test]
    fn test_note_event_end_tick_method() {
        let note = NoteEvent::new(100, 200, 60, 100, 5);
        assert_eq!(note.end_tick(), 200);
        assert_eq!(note.length(), 100);
    }
}
