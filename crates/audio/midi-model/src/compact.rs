//! 紧凑型 MIDI 事件格式 — 内存高效的缓存中间表示
//!
//! 相比 `MidiEvent` (24+ 字节/事件)，`CompactEvent` 固定 12 字节，
//! 专为大规模黑乐谱缓存场景设计。支持快速编码/解码，可直接按字节存储。
//!
//! 内存占用预估：
//! - `CompactEvent`: 12 bytes/event
//! - 1 亿事件 ≈ 1.14 GB（原始紧凑格式）
//! - 8 字节对齐时，条目本身无浪费（12 是 4 的倍数）

use core::fmt;

/// 紧凑型 MIDI 事件种类（4 位编码，最多 16 种）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EventKind {
    NoteOn = 0,
    NoteOff = 1,
    ControlChange = 2,
    ProgramChange = 3,
    Tempo = 4,
    TimeSignature = 5,
    KeySignature = 6,
    PitchBend = 7,
    Aftertouch = 8,
    PolyAftertouch = 9,
    SysEx = 10,
    Other = 11,
}

impl EventKind {
    /// 从 u8 安全转换
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::NoteOn),
            1 => Some(Self::NoteOff),
            2 => Some(Self::ControlChange),
            3 => Some(Self::ProgramChange),
            4 => Some(Self::Tempo),
            5 => Some(Self::TimeSignature),
            6 => Some(Self::KeySignature),
            7 => Some(Self::PitchBend),
            8 => Some(Self::Aftertouch),
            9 => Some(Self::PolyAftertouch),
            10 => Some(Self::SysEx),
            11 => Some(Self::Other),
            _ => None,
        }
    }

    /// 是否为音符事件（NoteOn/NoteOff）
    pub fn is_note(self) -> bool {
        matches!(self, Self::NoteOn | Self::NoteOff)
    }

    /// 是否为元事件（Tempo/TimeSignature/KeySignature）
    pub fn is_meta(self) -> bool {
        matches!(self, Self::Tempo | Self::TimeSignature | Self::KeySignature)
    }
}

/// 紧凑型 MIDI 事件 — 固定 12 字节
///
/// 二进制布局（小端序）：
/// ```text
/// offset  size  field
///  0       4    delta_tick : u32 — 块内相对 tick
///  4       2    track_id   : u16 — 音轨编号
///  6       2    param1     : u16 — NoteOn/Off=key, CC=controller, PC=program, Tempo=tempo低16位
///  8       2    param2     : u16 — NoteOn/Off=velocity, CC=value, Tempo=tempo高16位
/// 10       1    kind       : u8  — EventKind 数值
/// 11       1    channel    : u8  — MIDI通道 (0-15)
/// ─────────────────────────────────
/// total   12
/// ```
///
/// 内存占用：12 bytes/event，紧密排列时无浪费
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct CompactEvent {
    delta_tick: u32,
    track_id: u16,
    param1: u16,
    param2: u16,
    kind: u8,
    channel: u8,
}

// 手动实现 Debug（packed struct 不能 derive）
impl fmt::Debug for CompactEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompactEvent")
            .field("delta_tick", &self.delta_tick())
            .field("track_id", &self.track_id())
            .field("kind", &self.kind())
            .field("channel", &self.channel())
            .field("param1", &self.param1())
            .field("param2", &self.param2())
            .finish()
    }
}

impl PartialEq for CompactEvent {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for CompactEvent {}

/// 从 packed struct 的字段读取值。
/// 避免取引用（packed struct 字段引用是 UB），而是用 raw pointer + read_unaligned。
macro_rules! packed_field_read {
    ($self:expr, $field:ident, $ty:ty, $offset:expr) => {{
        let ptr = $self as *const Self as *const u8;
        unsafe { core::ptr::read_unaligned(ptr.add($offset) as *const $ty) }
    }};
}

macro_rules! packed_field_write {
    ($self:expr, $field:ident, $val:expr, $offset:expr) => {{
        let ptr = $self as *mut Self as *mut u8;
        unsafe { core::ptr::write_unaligned(ptr.add($offset) as *mut _, $val) }
    }};
}

// offset_of! 在 Rust 1.77+ 中已稳定
// 这里使用 `std::mem::offset_of!` for packed struct field offset calculation
// 注意：在 packed struct 上直接使用 offset_of! 时 field 必须是 pub 的
// 这里改为使用已知偏移常量（CompactEvent 的布局是稳定的）
const OFFSET_DELTA_TICK: usize = 0;
const OFFSET_TRACK_ID: usize = 4;
const OFFSET_PARAM1: usize = 6;
const OFFSET_PARAM2: usize = 8;
const OFFSET_KIND: usize = 10;
const OFFSET_CHANNEL: usize = 11;

/// 编译时验证字段偏移常量与实际布局一致。
/// 若 `CompactEvent` 字段顺序/类型变更导致偏移变化，此处 const assert 将触发编译错误。
const _: () = {
    assert!(std::mem::offset_of!(CompactEvent, delta_tick) == OFFSET_DELTA_TICK);
    assert!(std::mem::offset_of!(CompactEvent, track_id) == OFFSET_TRACK_ID);
    assert!(std::mem::offset_of!(CompactEvent, param1) == OFFSET_PARAM1);
    assert!(std::mem::offset_of!(CompactEvent, param2) == OFFSET_PARAM2);
    assert!(std::mem::offset_of!(CompactEvent, kind) == OFFSET_KIND);
    assert!(std::mem::offset_of!(CompactEvent, channel) == OFFSET_CHANNEL);
};

impl CompactEvent {
    /// 构造一个新的紧凑事件
    #[inline]
    pub fn new(
        delta_tick: u32,
        track_id: u16,
        kind: EventKind,
        channel: u8,
        param1: u16,
        param2: u16,
    ) -> Self {
        Self {
            delta_tick,
            track_id,
            param1,
            param2,
            kind: kind as u8,
            channel,
        }
    }

    /// 获取块内相对 tick
    #[inline]
    pub fn delta_tick(&self) -> u32 {
        packed_field_read!(self, delta_tick, u32, OFFSET_DELTA_TICK)
    }

    /// 设置块内相对 tick
    #[inline]
    pub fn set_delta_tick(&mut self, val: u32) {
        packed_field_write!(self, delta_tick, val, OFFSET_DELTA_TICK);
    }

    /// 获取音轨编号
    #[inline]
    pub fn track_id(&self) -> u16 {
        packed_field_read!(self, track_id, u16, OFFSET_TRACK_ID)
    }

    /// 获取事件种类
    #[inline]
    pub fn kind(&self) -> EventKind {
        EventKind::from_u8(packed_field_read!(self, kind, u8, OFFSET_KIND))
            .unwrap_or(EventKind::Other)
    }

    /// 获取 MIDI 通道
    #[inline]
    pub fn channel(&self) -> u8 {
        packed_field_read!(self, channel, u8, OFFSET_CHANNEL)
    }

    /// 获取参数1（取决于 kind）
    #[inline]
    pub fn param1(&self) -> u16 {
        packed_field_read!(self, param1, u16, OFFSET_PARAM1)
    }

    /// 获取参数2（取决于 kind）
    #[inline]
    pub fn param2(&self) -> u16 {
        packed_field_read!(self, param2, u16, OFFSET_PARAM2)
    }

    /// 以原始字节切片访问（12 字节）
    #[inline]
    pub fn as_bytes(&self) -> &[u8; 12] {
        unsafe { &*(self as *const Self as *const [u8; 12]) }
    }

    /// 从字节数组解码（不会验证字段有效性）
    #[inline]
    pub fn from_bytes(bytes: &[u8; 12]) -> Self {
        unsafe { core::ptr::read_unaligned(bytes.as_ptr() as *const Self) }
    }

    /// 检查事件是否属于指定音轨
    #[inline]
    pub fn is_track(&self, track_id: u16) -> bool {
        self.track_id() == track_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_event_roundtrip() {
        let event = CompactEvent::new(12345, 2, EventKind::NoteOn, 5, 60, 100);
        assert_eq!(event.delta_tick(), 12345);
        assert_eq!(event.track_id(), 2);
        assert_eq!(event.kind(), EventKind::NoteOn);
        assert_eq!(event.channel(), 5);
        assert_eq!(event.param1(), 60);
        assert_eq!(event.param2(), 100);

        // 字节往返
        let bytes = *event.as_bytes();
        let decoded = CompactEvent::from_bytes(&bytes);
        assert_eq!(decoded, event);
    }

    #[test]
    fn test_compact_event_note_kind() {
        assert!(EventKind::NoteOn.is_note());
        assert!(EventKind::NoteOff.is_note());
        assert!(!EventKind::ControlChange.is_note());
    }

    #[test]
    fn test_compact_event_meta_kind() {
        assert!(EventKind::Tempo.is_meta());
        assert!(EventKind::TimeSignature.is_meta());
        assert!(EventKind::KeySignature.is_meta());
        assert!(!EventKind::NoteOn.is_meta());
    }

    #[test]
    fn test_event_kind_from_u8() {
        assert_eq!(EventKind::from_u8(0), Some(EventKind::NoteOn));
        assert_eq!(EventKind::from_u8(11), Some(EventKind::Other));
        assert_eq!(EventKind::from_u8(255), None);
        assert_eq!(EventKind::from_u8(12), None);
    }

    #[test]
    fn test_compact_event_size() {
        assert_eq!(core::mem::size_of::<CompactEvent>(), 12);
    }

    #[test]
    fn test_compact_event_track_filter() {
        let event = CompactEvent::new(0, 7, EventKind::NoteOn, 0, 60, 100);
        assert!(event.is_track(7));
        assert!(!event.is_track(3));
    }
}
