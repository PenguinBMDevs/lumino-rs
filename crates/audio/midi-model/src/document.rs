//! MidiDocument — 解析后的 MIDI 文档（全内存紧凑存放）
//!
//! 使用 midly 提取音符后以 NoteEvent（16 bytes/note）紧凑存放。
//! 每轨独立一个 Vec，按 start_tick 排序，查询时无需 active-table 配对。
//!
//! 本文件仅保留类型定义（`MidiDocument` / `TrackNoteView` / `TICK_SEARCH_BUFFER`）
//! 与子模块声明，实现按职责拆分（保持各文件 < 400 行）：
//! - `scan`（document_scan.rs）：音轨名称扫描
//! - `build`（document_build.rs）：构造与加载
//! - `query`（document_query.rs）：只读查询
//! - `edit`（document_edit.rs）：音符编辑

use crate::note_event::NoteEvent;
use crate::track::TrackManager;

#[path = "document_scan.rs"]
pub(crate) mod scan;

#[path = "document_build.rs"]
mod build;

#[path = "document_query.rs"]
mod query;

#[path = "document_edit.rs"]
mod edit;

/// Tick 搜索缓冲区大小（用于二分查找的范围扩展）
///
/// 语义：查询视口 `[tick_start, tick_end]` 内音符时，从
/// `tick_start - TICK_SEARCH_BUFFER` 开始二分定位，保证时长不超过该缓冲区的
/// 跨视口长音符（start_tick 早于视口起点）不被遗漏。19200 tick ≈ 10 小节
/// （PPQ=480），覆盖绝大多数 MIDI 音符时长。
///
/// 视频导出（video_export）的可见音符收集与流式帧索引复用此常量，
/// 避免各模块魔法数漂移。
pub const TICK_SEARCH_BUFFER: u32 = 19200;

/// 音轨音符只读投影（替代裸 5 元组 `(start_tick, key, length, velocity, channel)`，
/// 调用端无需记忆字段顺序）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackNoteView {
    /// 起始 tick
    pub start_tick: f32,
    /// 键位（0-127 或 0-255，取决于键盘模式）
    pub key: u8,
    /// 时长（tick）
    pub length: f32,
    /// 力度（0-127）
    pub velocity: u8,
    /// MIDI 通道（0-15）
    pub channel: u8,
}

impl TrackNoteView {
    fn from_event(n: &NoteEvent) -> Self {
        Self {
            start_tick: n.start_tick as f32,
            key: n.key,
            length: n.length() as f32,
            velocity: n.velocity,
            channel: n.channel,
        }
    }
}

/// 解析后的 MIDI 文档（全内存紧凑存放）
///
/// 音符按音轨存放为 `Vec<ChunkedList<NoteEvent>>`，每轨内按 `start_tick` 升序排列，
/// 分块存储（50 万事件/块）保证插入不阻塞（O(块内) 而非 O(整轨)）。
/// 控制事件和速度变化仍保留，用于播放、导出和工程保存。
#[derive(Clone)]
pub struct MidiDocument {
    /// 每轨的音符列表，按 `start_tick` 升序排列，分块存储
    pub notes: Vec<crate::chunked_list::ChunkedList<NoteEvent>>,
    /// 音符全局唯一 ID 分配器（单调分配、删除不回收；0 = 未分配哨兵）。
    ///
    /// 全局单一计数器保证跨轨不重名；从 1 起（0 保留为未分配哨兵）。
    pub next_note_id: u64,
    /// 预提取的 tempo 变化（tick, bpm）
    pub tempo_changes: Vec<(u32, f32)>,
    /// 预提取的拍号变化（tick, 分子, 分母）。
    /// 分母为人类可读值：4 = 四分音符，8 = 八分音符。
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 预提取的调号变化（tick, 升降号数, 是否小调）。
    /// 正数表示升号数量，负数表示降号数量。
    pub key_signatures: Vec<(u32, i8, bool)>,
    /// MIDI 控制事件（CC / PC / PB），以 midly PackedControlEvent 紧凑存储，
    /// 分块（50 万事件/块）保证大量 CC 事件插入不阻塞
    pub control_events: crate::chunked_list::ChunkedList<midly::loader::PackedControlEvent>,
    /// 歌词文本事件（tick, track_id, 原始字节）
    pub lyrics: Vec<(u32, u16, Vec<u8>)>,
    /// 标记文本事件（tick, track_id, 原始字节）
    pub markers: Vec<(u32, u16, Vec<u8>)>,
    /// SysEx 事件（tick, track_id, 原始字节）
    pub sys_ex: Vec<(u32, u16, Vec<u8>)>,
    /// 音轨名称（索引 = track_index）
    pub track_names: Vec<Option<String>>,
    /// MIDI 文件总 tick 数
    pub total_ticks: u32,
    /// 音轨数量
    pub track_count: u16,
    /// 音轨可见性管理
    pub tracks: TrackManager,
    /// MIDI 文件头 division（PPQ）
    pub division: u16,
    /// 每轨 MIDI 端口（从 MidiPort meta FF 21 提取，默认 0）
    pub track_ports: Vec<u8>,
    /// 每轨最大音符结束 tick 缓存（None = 脏，惰性重算）。
    ///
    /// 2026-08-06 性能修复：走带视图滚动范围（`arrangement_max_tick_end`）在编辑后
    /// 全量扫描 1600W 音符 ≈ 29.8ms/次。本缓存由所有写入入口增量维护：
    /// 插入 O(1)（与当前 max 取大），删除/整轨替换/可变引用保守置脏（查询时
    /// 惰性重算一次 O(N)）。用 Mutex 而非 Cell 保证 Send（loader 跨线程传递）。
    ///
    /// 内部缓存：外部请使用 [`MidiDocument::track_max_end_tick`] 查询，
    /// 直接读写本字段会绕过置脏逻辑导致缓存失效。
    #[doc(hidden)]
    pub track_max_end_ticks: Vec<std::sync::Arc<std::sync::Mutex<Option<u32>>>>,
}

impl std::fmt::Debug for MidiDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total_notes: usize = self.notes.iter().map(|v| v.len()).sum();
        f.debug_struct("MidiDocument")
            .field("track_count", &self.track_count)
            .field("total_ticks", &self.total_ticks)
            .field("total_notes", &total_notes)
            .field("division", &self.division)
            .field("control_events.len", &self.control_events.len())
            .finish()
    }
}

/// MidiDocument 可写 API 单元测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "document_write_tests.rs"]
mod tests;
