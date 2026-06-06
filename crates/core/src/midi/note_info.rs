//! 轻量级核心音符类型 — 预解析的 Note，避免每帧重复事件→音符转换。
//!
//! 传统 MIDI 存储使用 NoteOn/NoteOff 事件对，查询时必须扫描事件并匹配 start/end。
//! `NoteInfo` 将音符预解析为自包含结构，查询时无需 active-table 扫描即可直接使用。

/// 预解析的核心音符，包含自解析后完整的音符信息
///
/// 与 `CompactEvent` 的关系：
/// - 1 个 `NoteInfo` = 1 个 `PackedNote` = 2 个 `CompactEvent` (NoteOn + NoteOff)
/// - 大小：1×NoteInfo (12 bytes) vs 2×CompactEvent (24 bytes)，节省 50% 存储
/// - 查询：直接读取 start_tick + length，无需 active-table 配对
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteInfo {
    /// 音符开始 tick
    pub start_tick: u32,
    /// 音符时长（tick 数）
    pub length: u32,
    /// MIDI key (0-127)
    pub key: u8,
    /// 力度 (0-127)
    pub velocity: u8,
    /// MIDI 通道 (0-15)
    pub channel: u8,
}

impl NoteInfo {
    /// 创建一个新的 NoteInfo
    #[inline]
    pub fn new(start_tick: u32, length: u32, key: u8, velocity: u8, channel: u8) -> Self {
        Self {
            start_tick,
            length,
            key,
            velocity,
            channel,
        }
    }

    /// 音符结束 tick（start_tick + length）
    #[inline]
    pub fn end_tick(&self) -> u32 {
        self.start_tick.saturating_add(self.length)
    }
}
