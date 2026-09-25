//! 单条音符表示 — 每个音符一条记录，替代 NoteOn + NoteOff 两个事件。
//!
//! 这是 lumino MIDI 加载的第二刀：把 `CompactEvent` 中拆成两条的音符
//! 合并成 `(start_tick, end_tick, key, velocity, channel)`，内存减半。

use crate::chunked_list::EventTick;
use crate::compact::{CompactEvent, EventKind};
use crate::note_info::NoteInfo;

use midly;

/// 单个音符的自包含表示。
///
/// 与 `CompactEvent` 的 note 事件对相比：
/// - `CompactEvent`: 2 × 12 bytes = 24 bytes / note
/// - `NoteEvent`: 24 bytes（id u64 + start/end u32 + key/vel/rel/chan u8，末尾 padding 对齐）/ note
///
/// `release_velocity` 复用原 padding 位，`size_of::<NoteEvent>() == 24` 由单测锁死。
///
/// `id` 为文档级全局唯一、单调递增、删除不回收的稳定身份，
/// 用于撤销/重做与协作同步的精确引用（取代易漂移的 index/坐标）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct NoteEvent {
    /// 文档级全局唯一 ID（分配器单调分配，删除不回收；0 = 未分配哨兵）
    pub id: u64,
    /// 音符开始 tick
    pub start_tick: u32,
    /// 音符结束 tick
    pub end_tick: u32,
    /// MIDI key (0-127)
    pub key: u8,
    /// 按压力度 / NoteOn velocity (0-127)
    pub velocity: u8,
    /// 释放力度 / NoteOff velocity (0-127)，缺失时为 0
    pub release_velocity: u8,
    /// MIDI 通道 (0-15)
    pub channel: u8,
}

impl NoteEvent {
    /// 未分配哨兵 id（分配器从 1 开始，永不发出 0）。
    pub const UNASSIGNED_ID: u64 = 0;

    /// 创建新音符（id 默认未分配，存储前须用 `with_id` 附加全局唯一 ID）。
    ///
    /// 释放力度默认 0；需要显式释放力度时用 [`Self::new_with_release`]。
    #[inline]
    pub fn new(start_tick: u32, end_tick: u32, key: u8, velocity: u8, channel: u8) -> Self {
        Self {
            id: Self::UNASSIGNED_ID,
            start_tick,
            end_tick,
            key,
            velocity,
            release_velocity: 0,
            channel,
        }
    }

    /// 创建携带释放力度的新音符（MIDI 加载 / 工程恢复路径用）。
    #[inline]
    pub fn new_with_release(
        start_tick: u32,
        end_tick: u32,
        key: u8,
        velocity: u8,
        release_velocity: u8,
        channel: u8,
    ) -> Self {
        Self {
            id: Self::UNASSIGNED_ID,
            start_tick,
            end_tick,
            key,
            velocity,
            release_velocity,
            channel,
        }
    }

    /// 为音符附加全局唯一 ID（构建器风格）。
    #[inline]
    pub fn with_id(mut self, id: u64) -> Self {
        self.id = id;
        self
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
                self.release_velocity as u16,
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
            id: Self::UNASSIGNED_ID,
            start_tick: info.start_tick,
            end_tick: info.end_tick(),
            key: info.key,
            velocity: info.velocity,
            // NoteInfo（UI 缓存）无释放力度概念，归零
            release_velocity: 0,
            channel: info.channel,
        }
    }
}

impl From<midly::loader::PackedNote> for NoteEvent {
    #[inline]
    fn from(note: midly::loader::PackedNote) -> Self {
        Self {
            id: Self::UNASSIGNED_ID,
            start_tick: note.start_tick,
            end_tick: note.end_tick,
            key: note.key,
            velocity: note.velocity,
            release_velocity: note.release_velocity,
            channel: note.channel,
        }
    }
}

impl EventTick for NoteEvent {
    #[inline]
    fn tick(&self) -> u32 {
        self.start_tick
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
    fn test_note_event_layout_still_24_bytes() {
        // 释放力度复用原 padding 位，加字段后体积不得上涨（千万级内存红线）
        assert_eq!(core::mem::size_of::<NoteEvent>(), 24);
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
        // new() 默认释放力度 0（不再复用按压力度）
        assert_eq!(off.param2(), 0);
    }

    #[test]
    fn test_note_event_release_velocity_roundtrip() {
        let note = NoteEvent::new_with_release(100, 200, 60, 100, 64, 5);
        let [_, off] = note.to_compact_events(3);
        assert_eq!(off.param2(), 64);

        let packed = midly::loader::PackedNote::new_with_release(100, 200, 60, 100, 77, 5, 0);
        let doc_note = NoteEvent::from(packed);
        assert_eq!(doc_note.velocity, 100);
        assert_eq!(doc_note.release_velocity, 77);
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
