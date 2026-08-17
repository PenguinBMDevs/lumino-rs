//! MidiDocument — 解析后的 MIDI 文档（全内存紧凑存放）
//!
//! 使用 midly 提取音符后以 NoteEvent（16 bytes/note）紧凑存放。
//! 每轨独立一个 Vec，按 start_tick 排序，查询时无需 active-table 配对。

use lumino_memory_monitor::MemoryMonitor;

use crate::error::{LoaderError, LoaderResult};
use crate::note_event::NoteEvent;
use crate::track::TrackManager;

#[path = "document_scan.rs"]
pub(crate) mod scan;

use std::path::Path;

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

/// 按 tick 排序去重；若首项 tick != 0，在头部插入 tick=0 的默认事件。
///
/// 收敛 `from_notes_bytes` 中 tempo/time_sig/key_sig 三处同构的
/// "sort → dedup → 首项补默认" 模板。
fn finalize_sorted_events<T>(events: &mut Vec<T>, get_tick: impl Fn(&T) -> u32, default_at_zero: T) {
    events.sort_unstable_by_key(&get_tick);
    events.dedup_by(|a, b| get_tick(a) == get_tick(b));
    if events.first().is_none_or(|e| get_tick(e) != 0) {
        events.insert(0, default_at_zero);
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

impl MidiDocument {
    /// 构造空白文档（N 条空轨道，供新建工程初始化为可编辑空白工程）。
    ///
    /// 2026-08 修复：新建工程/关闭文件后 `clear_editor` 复用本构造重建空文档，
    /// 使 `insert_note` 不再因 `document = None` 拦截音符创建（空白工程可直接编辑）。
    ///
    /// 轨道命名与 `Sidebar::new()` 默认轨道一致：轨道 0 = "Conductor"，轨道 1 = "Setup"，
    /// 其余为 "Track {i}"。`division` 为文档头 PPQ（调用方传入，通常是编辑器当前 PPQ，
    /// 避免新建工程落盘时回退到硬编码 480）。
    pub fn empty_with_tracks(track_count: u16, division: u16) -> Self {
        Self {
            notes: (0..track_count)
                .map(|_| crate::chunked_list::ChunkedList::new())
                .collect(),
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: Vec::new(),
            control_events: crate::chunked_list::ChunkedList::new(),
            lyrics: Vec::new(),
            markers: Vec::new(),
            sys_ex: Vec::new(),
            track_names: (0..track_count)
                .map(|i| match i {
                    0 => Some("Conductor".to_string()),
                    1 => Some("Setup".to_string()),
                    _ => Some(format!("Track {i}")),
                })
                .collect(),
            total_ticks: 0,
            track_count,
            tracks: crate::track::TrackManager::new(track_count),
            division,
            track_ports: vec![0; track_count as usize],
            track_max_end_ticks: Self::new_track_max_ticks(track_count as usize),
        }
    }

    /// 使用 midly 从 MIDI 文件加载并构建紧凑内存文档。
    pub fn from_notes_file<P: AsRef<Path>>(
        midi_path: P,
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<Self> {
        let path = midi_path.as_ref();
        let file_bytes = std::fs::read(path).map_err(LoaderError::Io)?;
        let (doc, _, _) = Self::from_notes_bytes(&file_bytes, progress)?;
        Ok(doc)
    }

    /// 使用 midly 从 MIDI 字节加载并构建紧凑内存文档。
    pub fn from_notes_bytes(
        file_bytes: &[u8],
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<(Self, u16, u64)> {
        MemoryMonitor::global().check();

        if let Some(cb) = progress {
            (cb)(0.05);
        }

        let (_, division) = scan::scan_header_info(file_bytes)
            .ok_or_else(|| LoaderError::FileFormat("无法解析 MIDI 文件头".to_string()))?;

        if let Some(cb) = progress {
            (cb)(0.10);
        }

        // 使用 MIDI 标签追踪解析过程中的密集内存分配
        lumino_memtrace::with_tag(lumino_memtrace::AllocTag::Midi, || {
            let track_names = scan::scan_track_names(file_bytes);

            if let Some(cb) = progress {
                (cb)(0.15);
            }

            let mut notes: Vec<crate::chunked_list::ChunkedList<NoteEvent>> = Vec::new();
            let mut track_ports: Vec<u8> = Vec::new();
            let mut all_tempo_changes: Vec<(u32, f32)> = Vec::new();
            let mut all_time_signatures: Vec<(u32, u8, u8)> = Vec::new();
            let mut all_key_signatures: Vec<(u32, i8, bool)> = Vec::new();
            let mut control_events: Vec<midly::loader::PackedControlEvent> = Vec::new();
            let mut lyrics: Vec<(u32, u16, Vec<u8>)> = Vec::new();
            let mut markers: Vec<(u32, u16, Vec<u8>)> = Vec::new();
            let mut sys_ex: Vec<(u32, u16, Vec<u8>)> = Vec::new();
            let mut total_notes: u64 = 0;
            let mut total_ticks: u32 = 0;

            midly::loader::extract_all_events_per_track_streaming_from_bytes(
                file_bytes,
                |track_idx, mut events| {
                    if track_idx >= notes.len() {
                        notes.resize_with(track_idx + 1, crate::chunked_list::ChunkedList::new);
                        track_ports.resize(track_idx + 1, 0);
                    }

                    // 2026-08-15 峰值优化：不再经中间 Vec<NoteEvent> 中转，
                    // PackedNote 直接分块转换进 ChunkedList（省 16B/音符的峰值内存，
                    // 2.9 亿音符场景 ≈ 4.6GB）。
                    if let Some(last) = events.notes.iter().max_by_key(|n| n.end_tick) {
                        total_ticks = total_ticks.max(last.end_tick);
                    }
                    total_notes += events.notes.len() as u64;

                    // midly 的 TrackAllEvents 按「NoteOff 到达顺序」产出音符（重叠音符中
                    // 先结束后开始的会排在先开始音符之前），并不保证按 start_tick 升序——
                    // 其文档注释「sorted by start tick」对该流式路径并不成立。
                    // ChunkedList 依赖 start_tick 升序做区间查询（range / 走带视图），
                    // 必须在转换前显式排序。仅对 midly 已物化的本轨 Vec<PackedNote>
                    // 做原地排序，不引入额外峰值内存（无第二个 Vec<NoteEvent> 常驻）。
                    events.notes.sort_by_key(|n| n.start_tick);
                    notes[track_idx] = crate::chunked_list::ChunkedList::from_sorted_iter(
                        events.notes.into_iter().map(NoteEvent::from),
                    );

                    // MidiPort meta (FF 21)：流式提取首个出现值（与旧 Smf::parse
                    // 语义一致），不再对文件做第二次全量解析——2.9 亿音符的
                    // 黑乐谱此前在此产生 15-18GB 临时峰值。
                    track_ports[track_idx] = events.midi_port.unwrap_or(0);

                    all_tempo_changes.extend(events.tempo_changes);
                    control_events.extend(events.control_events);
                    all_time_signatures.extend(events.time_signatures.into_iter().map(|ts| {
                        (
                            ts.tick,
                            ts.numerator,
                            1u32.wrapping_shl(ts.denominator as u32) as u8,
                        )
                    }));
                    all_key_signatures.extend(
                        events
                            .key_signatures
                            .into_iter()
                            .map(|ks| (ks.tick, ks.sharps, ks.is_minor)),
                    );
                    lyrics.extend(
                        events
                            .lyrics
                            .into_iter()
                            .map(|ev| (ev.tick, ev.track, ev.text.to_vec())),
                    );
                    markers.extend(
                        events
                            .markers
                            .into_iter()
                            .map(|ev| (ev.tick, ev.track, ev.text.to_vec())),
                    );
                    sys_ex.extend(
                        events
                            .sys_ex
                            .into_iter()
                            .map(|ev| (ev.tick, ev.track, ev.data.to_vec())),
                    );
                },
            )
            .map_err(|e| LoaderError::MidiParse(format!("提取音符失败: {e}")))?;

            finalize_sorted_events(&mut all_tempo_changes, |&(t, _)| t, (0u32, 120.0f32));
            finalize_sorted_events(&mut all_time_signatures, |&(t, _, _)| t, (0u32, 4u8, 4u8));
            finalize_sorted_events(&mut all_key_signatures, |&(t, _, _)| t, (0u32, 0i8, false));

            control_events.sort_unstable_by_key(|e| e.tick);
            lyrics.sort_unstable_by_key(|e| e.0);
            markers.sort_unstable_by_key(|e| e.0);
            sys_ex.sort_unstable_by_key(|e| e.0);

            if let Some(cb) = progress {
                (cb)(0.75);
            }

            let track_count = notes.len() as u16;
            let tracks_manager = TrackManager::new(track_count);

            if let Some(cb) = progress {
                (cb)(0.90);
            }

            Ok((
                Self {
                    notes,
                    tempo_changes: all_tempo_changes,
                    time_signatures: all_time_signatures,
                    key_signatures: all_key_signatures,
                    control_events: crate::chunked_list::ChunkedList::from_sorted(control_events),
                    lyrics,
                    markers,
                    sys_ex,
                    track_names,
                    total_ticks,
                    track_count,
                    tracks: tracks_manager,
                    division,
                    track_ports,
                    // max_end_tick 缓存：初始 None（脏），首次查询惰性重算
                    track_max_end_ticks: Self::new_track_max_ticks(track_count as usize),
                },
                division,
                total_notes,
            ))
        })
    }

    /// 获取总 tick 数
    #[inline]
    pub fn total_ticks(&self) -> u32 {
        self.total_ticks
    }

    /// 获取音轨可见性管理（只读视图）。
    #[inline]
    pub fn tracks(&self) -> &TrackManager {
        &self.tracks
    }

    /// 获取 MIDI 文件头 division（PPQ）。
    #[inline]
    pub fn division(&self) -> u16 {
        self.division
    }

    /// 获取所有 CompactEvent（按需从 NoteEvent 实时构造）。
    pub fn all_events(&self) -> Vec<crate::compact::CompactEvent> {
        let mut events = Vec::with_capacity(self.total_note_count() * 2);
        for (track_id, track_notes) in self.notes.iter().enumerate() {
            let track_id_u16 = track_id as u16;
            for note in track_notes {
                let [on, off] = note.to_compact_events(track_id_u16);
                events.push(on);
                events.push(off);
            }
        }
        events
    }

    /// 获取指定音轨的所有 CompactEvent（按需从 NoteEvent 构造）。
    pub fn get_track_events(&self, track_id: u16) -> Vec<crate::compact::CompactEvent> {
        let tid = track_id as usize;
        match self.notes.get(tid) {
            Some(track_notes) => {
                let mut events = Vec::with_capacity(track_notes.len() * 2);
                for note in track_notes {
                    let [on, off] = note.to_compact_events(track_id);
                    events.push(on);
                    events.push(off);
                }
                events
            }
            None => Vec::new(),
        }
    }

    /// 获取指定 tick 范围内的所有 CompactEvent（按需从 NoteEvent 构造）。
    pub fn get_events_in_range(
        &self,
        from_tick: u32,
        to_tick: u32,
        max_events: usize,
    ) -> Vec<crate::compact::CompactEvent> {
        let limit = if max_events == 0 {
            usize::MAX
        } else {
            max_events
        };
        let mut events = Vec::new();
        for (track_id, track_notes) in self.notes.iter().enumerate() {
            let track_id_u16 = track_id as u16;
            for note in track_notes {
                let [on, off] = note.to_compact_events(track_id_u16);
                let on_tick = on.delta_tick();
                let off_tick = off.delta_tick();
                if on_tick >= from_tick && on_tick < to_tick {
                    events.push(on);
                }
                if off_tick >= from_tick && off_tick < to_tick {
                    events.push(off);
                }
                if events.len() >= limit {
                    return events;
                }
            }
        }
        events
    }

    /// 检查指定音轨在指定范围内是否有事件。
    pub fn has_track_events_in_range(&self, track_id: u16, from_tick: u32, to_tick: u32) -> bool {
        let tid = track_id as usize;
        let Some(track_notes) = self.notes.get(tid) else {
            return false;
        };
        track_notes.iter().any(|note| {
            (note.start_tick >= from_tick && note.start_tick < to_tick)
                || (note.end_tick > from_tick && note.end_tick < to_tick)
        })
    }

    /// 轻量获取指定音轨的音符数。
    pub fn track_note_count(&self, track_id: u16) -> u64 {
        let tid = track_id as usize;
        self.notes
            .get(tid)
            .map(|notes| notes.len() as u64)
            .unwrap_or(0)
    }

    /// 获取总音符数。
    fn total_note_count(&self) -> usize {
        self.notes.iter().map(|v| v.len()).sum()
    }

    /// 获取指定音轨在指定 tick 范围内的音符。
    pub fn get_track_notes_in_range(
        &self,
        track_id: u16,
        tick_start: f32,
        tick_end: f32,
    ) -> Vec<TrackNoteView> {
        let tid = track_id as usize;
        let notes = match self.notes.get(tid) {
            Some(n) if !n.is_empty() => n,
            _ => return Vec::new(),
        };

        let tick_start_u = tick_start as u32;
        let tick_end_u = tick_end as u32;

        // 分块二分：从 tick_start - TICK_SEARCH_BUFFER 开始扫描（跨视口长音符）
        let search_start_tick = tick_start_u.saturating_sub(TICK_SEARCH_BUFFER);
        let mut result = Vec::with_capacity(256);
        for n in notes.range(search_start_tick, tick_end_u + 1) {
            if n.end_tick() >= tick_start_u && n.start_tick <= tick_end_u {
                result.push(TrackNoteView::from_event(n));
            }
        }

        result
    }

    /// 获取指定音轨的所有音符。
    pub fn get_track_notes(&self, track_id: u16) -> Vec<TrackNoteView> {
        let tid = track_id as usize;
        match self.notes.get(tid) {
            Some(notes) if !notes.is_empty() => {
                let mut result = Vec::with_capacity(notes.len());
                for n in notes {
                    result.push(TrackNoteView::from_event(n));
                }
                result
            }
            _ => Vec::new(),
        }
    }

    /// 获取指定音轨的代表性 MIDI 通道。
    ///
    /// 通道确定策略（参考 yinhe MIDI 导入逻辑）：
    /// 1. 如果有音符，取**第一个音符**的通道；
    /// 2. 如果没有音符但有控制事件（CC/PC/PB），取第一个控制事件的通道；
    /// 3. 如果既无音符也无控制事件，返回 0（默认）。
    ///
    /// 取首事件通道而非统计最频通道，原因：
    /// - 一个音轨中绝大多数音符在单一通道，但可能混入少量其他通道的事件
    ///   （如控制器事件），统计最频会导致偶然偏差；
    /// - 首个事件的通道代表 DAW/MIDI 编排时为该轨分配的"意图通道"。
    pub fn track_channel(&self, track_id: u16) -> u8 {
        let tid = track_id as usize;
        // 策略 1：取第一个音符的通道
        if let Some(first) = self.notes.get(tid).and_then(|n| n.first()) {
            return first.channel & 0x0F;
        }
        // 策略 2：没有音符时，取第一个控制事件的通道
        for ev in &self.control_events {
            if ev.track == track_id {
                return ev.channel & 0x0F;
            }
        }
        // 策略 3：都没有，返回 0
        0
    }

    /// 获取指定音轨的 MIDI 端口（从 MidiPort meta FF 21 提取）。
    /// 若音轨无 MidiPort 事件，返回 0（默认端口）。
    #[inline]
    pub fn track_port(&self, track_id: u16) -> u8 {
        self.track_ports
            .get(track_id as usize)
            .copied()
            .unwrap_or(0)
    }

    /// 获取所有音轨（排除指定音轨）在指定 tick 范围内的音符。
    pub fn get_all_notes_in_range_except(
        &self,
        exclude_track: usize,
        tick_start: f32,
        tick_end: f32,
    ) -> Vec<TrackNoteView> {
        let tick_start_u = tick_start as u32;
        let tick_end_u = tick_end as u32;

        let mut all_notes = Vec::with_capacity(1024);

        for track_idx in 0..self.track_count() {
            if track_idx == exclude_track {
                continue;
            }

            let notes = match self.notes.get(track_idx) {
                Some(n) => n,
                None => continue,
            };

            if notes.is_empty() {
                continue;
            }

            let search_start_tick = tick_start_u.saturating_sub(TICK_SEARCH_BUFFER);
            for n in notes.range(search_start_tick, tick_end_u + 1) {
                if n.end_tick() >= tick_start_u && n.start_tick <= tick_end_u {
                    all_notes.push(TrackNoteView::from_event(n));
                }
            }
        }

        all_notes.sort_by(|a, b| a.start_tick.total_cmp(&b.start_tick));
        all_notes
    }

    /// 获取音轨数量
    #[inline]
    pub fn track_count(&self) -> usize {
        self.track_count as usize
    }

    /// 追加一条空音轨（图片转 MIDI 自动建轨用），返回新音轨 id
    ///
    /// 同步维护全部音轨相关字段：notes / track_names / track_ports /
    /// track_max_end_ticks / tracks / track_count。
    pub fn add_empty_track(&mut self) -> u16 {
        let new_id = self.track_count;
        self.notes.push(crate::chunked_list::ChunkedList::new());
        self.track_names.push(None);
        self.track_ports.push(0);
        self.track_max_end_ticks
            .push(std::sync::Arc::new(std::sync::Mutex::new(None)));
        self.tracks.push(crate::track::TrackView::new(new_id));
        self.track_count = self.track_count.saturating_add(1);
        new_id
    }

    /// 获取指定音轨的名称
    #[inline]
    pub fn track_name(&self, track_id: usize) -> Option<&str> {
        self.track_names.get(track_id).and_then(|n| n.as_deref())
    }

    /// 获取指定音轨的预解析音符引用（分块容器）。
    #[inline]
    pub fn track_notes(&self, track_id: usize) -> &crate::chunked_list::ChunkedList<NoteEvent> {
        static EMPTY: crate::chunked_list::ChunkedList<NoteEvent> =
            crate::chunked_list::ChunkedList::EMPTY;
        self.notes.get(track_id).unwrap_or(&EMPTY)
    }

    /// 构造每轨 max_end_tick 缓存（N 个**独立** Arc<Mutex>）。
    ///
    /// 注意：不能用 `vec![Arc::new(Mutex::new(None)); N]` —— 该写法把同一个 Arc
    /// 克隆 N 次，导致所有音轨共享**同一个** Mutex（缓存串号）。必须逐个构造。
    pub fn new_track_max_ticks(n: usize) -> Vec<std::sync::Arc<std::sync::Mutex<Option<u32>>>> {
        (0..n)
            .map(|_| std::sync::Arc::new(std::sync::Mutex::new(None)))
            .collect()
    }

    /// 在指定音轨按 start_tick 升序插入一个音符（保持每轨有序不变式）。
    /// 若 track_id 越界（音轨不存在）返回 false；成功返回 true。
    /// 同 start_tick 的音符插到已存在同 tick 音符之后（稳定插入）。
    pub fn insert_note(&mut self, track_id: usize, note: NoteEvent) -> bool {
        let Some(track_notes) = self.notes.get_mut(track_id) else {
            return false;
        };
        // 分块插入：只移动目标块内元素（O(块内)），满块自动分裂
        track_notes.insert(note);
        // 增量更新 max 缓存（脏时保持脏，查询时惰性重算）
        if let Some(cell) = self.track_max_end_ticks.get(track_id)
            && let Some(cur) = cell.lock().ok().and_then(|g| *g)
            && note.end_tick > cur
        {
            *cell.lock().unwrap_or_else(|e| e.into_inner()) = Some(note.end_tick);
        }
        true
    }

    /// 使指定音轨的 max_end_tick 缓存失效（置脏），下次查询时惰性重算。
    ///
    /// 毒锁时恢复（`into_inner`）而非 panic：缓存失效是保守操作，
    /// 即使锁被 panic 污染也不应中断编辑流程。
    fn invalidate_track_max_tick(&self, track_id: usize) {
        if let Some(cell) = self.track_max_end_ticks.get(track_id) {
            *cell.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    /// 删除指定音轨指定索引处的音符，返回被删除的音符副本。
    /// track_id 越界或 index 越界返回 None。
    pub fn remove_note(&mut self, track_id: usize, index: usize) -> Option<NoteEvent> {
        let removed = {
            let track_notes = self.notes.get_mut(track_id)?;
            track_notes.remove(index)
        };
        // 保守置脏：被删音符可能是当前 max，查询时惰性重算
        self.invalidate_track_max_tick(track_id);
        removed
    }

    /// 替换指定音轨指定索引处的音符：删除旧音符后按 start_tick 升序重新插入新音符，
    /// 保持每轨有序不变式。track_id 或 index 越界返回 false。
    pub fn update_note(&mut self, track_id: usize, index: usize, note: NoteEvent) -> bool {
        // 先删除旧音符；删除失败（track_id/index 越界）直接返回 false
        if self.remove_note(track_id, index).is_none() {
            return false;
        }
        // 删除成功已证明音轨存在，插入必然成功，不会出现中间不一致状态
        self.insert_note(track_id, note)
    }

    /// 返回指定音轨的可变音符引用（供批量编辑/排序场景使用）。
    /// track_id 越界返回 None。
    /// 注意：调用方必须保持 start_tick 升序不变式，本方法不校验。
    /// 返回后 max 缓存被置脏，下次 `track_max_end_tick` 查询时惰性重算。
    pub fn track_notes_mut(
        &mut self,
        track_id: usize,
    ) -> Option<&mut crate::chunked_list::ChunkedList<NoteEvent>> {
        // 可变引用逃逸后无法感知修改内容，保守置脏
        self.invalidate_track_max_tick(track_id);
        self.notes.get_mut(track_id)
    }

    /// 整轨替换音符（undo/redo 快照恢复专用）。
    ///
    /// `notes` 需按 start_tick 升序（调用方负责排序）；本方法直接整体赋值，
    /// 不做排序校验。track_id 越界返回 false。
    pub fn replace_track_notes(&mut self, track_id: usize, notes: Vec<NoteEvent>) -> bool {
        let Some(track) = self.notes.get_mut(track_id) else {
            return false;
        };
        *track = crate::chunked_list::ChunkedList::from_sorted(notes);
        self.invalidate_track_max_tick(track_id);
        true
    }

    /// 整轨替换音符（undo/redo 快照恢复专用，O(块数) 浅拷贝版）。
    ///
    /// 直接共享 `notes` 的块 Arc（`ChunkedList::clone` 为 O(块数) 指针拷贝），
    /// 不做数据复制——1600W 音符工程 undo/redo 恢复不再产生整轨拷贝。
    /// track_id 越界返回 false。
    pub fn replace_track_notes_chunked(
        &mut self,
        track_id: usize,
        notes: &crate::chunked_list::ChunkedList<NoteEvent>,
    ) -> bool {
        let Some(track) = self.notes.get_mut(track_id) else {
            return false;
        };
        *track = notes.clone();
        self.invalidate_track_max_tick(track_id);
        true
    }

    /// 清空指定音轨的所有音符。track_id 越界返回 false。
    pub fn clear_track_notes(&mut self, track_id: usize) -> bool {
        let Some(track) = self.notes.get_mut(track_id) else {
            return false;
        };
        track.clear();
        // 空轨缓存置脏（None），与 recompute 的空轨处理一致，避免残留 Some(0) 误判
        self.invalidate_track_max_tick(track_id);
        true
    }

    /// 指定音轨的最大音符结束 tick（O(1) 缓存命中；缓存脏时惰性重算一次 O(N)）。
    #[inline]
    pub fn track_max_end_tick(&self, track_id: usize) -> u32 {
        let Some(cell) = self.track_max_end_ticks.get(track_id) else {
            return 0;
        };
        if let Some(v) = cell.lock().ok().and_then(|g| *g) {
            return v;
        }
        // 惰性重算：end_tick 与 start_tick 排序无关，需全轨扫描取最大
        let max = self
            .notes
            .get(track_id)
            .map(|n| n.iter().map(|note| note.end_tick).max().unwrap_or(0))
            .unwrap_or(0);
        // 空轨 max=0 不缓存为 Some(0)（避免与"脏"语义混淆），保持脏（None），
        // 下次查询继续惰性重算（空轨重算成本为 0）。
        *cell.lock().unwrap_or_else(|e| e.into_inner()) =
            if max == 0 { None } else { Some(max) };
        max
    }

    /// 所有音轨的最大音符结束 tick（走带视图滚动范围用，O(音轨数)）。
    #[inline]
    pub fn tracks_max_end_tick(&self) -> u32 {
        (0..self.track_count())
            .map(|t| self.track_max_end_tick(t))
            .max()
            .unwrap_or(0)
    }
}

/// MidiDocument 可写 API 单元测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "document_write_tests.rs"]
mod tests;
