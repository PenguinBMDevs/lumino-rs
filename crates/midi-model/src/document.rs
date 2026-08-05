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

/// 解析后的 MIDI 文档（全内存紧凑存放）
///
/// 音符按音轨存放为 `Vec<Vec<NoteEvent>>`，每轨内按 `start_tick` 升序排列。
/// 控制事件和速度变化仍保留，用于播放、导出和工程保存。
#[derive(Clone)]
pub struct MidiDocument {
    /// 每轨的音符列表，按 `start_tick` 升序排列
    pub notes: Vec<Vec<NoteEvent>>,
    /// 预提取的 tempo 变化（tick, bpm）
    pub tempo_changes: Vec<(u32, f32)>,
    /// 预提取的拍号变化（tick, 分子, 分母）。
    /// 分母为人类可读值：4 = 四分音符，8 = 八分音符。
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 预提取的调号变化（tick, 升降号数, 是否小调）。
    /// 正数表示升号数量，负数表示降号数量。
    pub key_signatures: Vec<(u32, i8, bool)>,
    /// MIDI 控制事件（CC / PC / PB），以 midly PackedControlEvent 紧凑存储
    pub control_events: Vec<midly::loader::PackedControlEvent>,
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

            let mut notes: Vec<Vec<NoteEvent>> = Vec::new();
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
                |track_idx, events| {
                    if track_idx >= notes.len() {
                        notes.resize_with(track_idx + 1, Vec::new);
                    }

                    let mut track_notes: Vec<NoteEvent> =
                        events.notes.into_iter().map(NoteEvent::from).collect();

                    if let Some(last) = track_notes.iter().max_by_key(|n| n.end_tick) {
                        total_ticks = total_ticks.max(last.end_tick);
                    }
                    if track_notes.len() > 1 {
                        track_notes.sort_unstable_by_key(|n| n.start_tick);
                    }
                    total_notes += track_notes.len() as u64;
                    notes[track_idx] = track_notes;

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

            all_tempo_changes.sort_unstable_by_key(|&(t, _)| t);
            all_tempo_changes.dedup_by(|a, b| a.0 == b.0);
            if all_tempo_changes.first().is_none_or(|&(t, _)| t != 0) {
                all_tempo_changes.insert(0, (0u32, 120.0f32));
            }

            all_time_signatures.sort_unstable_by_key(|&(t, _, _)| t);
            all_time_signatures.dedup_by(|a, b| a.0 == b.0);
            if all_time_signatures.first().is_none_or(|&(t, _, _)| t != 0) {
                all_time_signatures.insert(0, (0u32, 4u8, 4u8));
            }

            all_key_signatures.sort_unstable_by_key(|&(t, _, _)| t);
            all_key_signatures.dedup_by(|a, b| a.0 == b.0);
            if all_key_signatures.first().is_none_or(|&(t, _, _)| t != 0) {
                all_key_signatures.insert(0, (0u32, 0i8, false));
            }

            control_events.sort_unstable_by_key(|e| e.tick);
            lyrics.sort_unstable_by_key(|e| e.0);
            markers.sort_unstable_by_key(|e| e.0);
            sys_ex.sort_unstable_by_key(|e| e.0);

            if let Some(cb) = progress {
                (cb)(0.75);
            }

            let track_count = notes.len() as u16;
            let tracks_manager = TrackManager::new(track_count);

            // 从 SMF 提取 MidiPort meta (FF 21) 事件，获取每轨的 MIDI 端口
            let track_ports: Vec<u8> = {
                if let Ok(smf) = midly::Smf::parse(file_bytes) {
                    smf.tracks
                        .iter()
                        .map(|track| {
                            for ev in track {
                                if let midly::TrackEventKind::Meta(midly::MetaMessage::MidiPort(
                                    port,
                                )) = &ev.kind
                                {
                                    return port.as_int();
                                }
                            }
                            0u8
                        })
                        .collect()
                } else {
                    vec![0u8; track_count as usize]
                }
            };

            if let Some(cb) = progress {
                (cb)(0.90);
            }

            Ok((
                Self {
                    notes,
                    tempo_changes: all_tempo_changes,
                    time_signatures: all_time_signatures,
                    key_signatures: all_key_signatures,
                    control_events,
                    lyrics,
                    markers,
                    sys_ex,
                    track_names,
                    total_ticks,
                    track_count,
                    tracks: tracks_manager,
                    division,
                    track_ports,
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
    ) -> Vec<(f32, u8, f32, u8, u8)> {
        let tid = track_id as usize;
        let notes = match self.notes.get(tid) {
            Some(n) if !n.is_empty() => n,
            _ => return Vec::new(),
        };

        let tick_start_u = tick_start as u32;
        let tick_end_u = tick_end as u32;

        let search_start = notes
            .partition_point(|n| n.start_tick < tick_start_u.saturating_sub(TICK_SEARCH_BUFFER));
        let search_end = notes.len().min(
            search_start + notes[search_start..].partition_point(|n| n.start_tick <= tick_end_u),
        );

        if search_start >= search_end {
            return Vec::new();
        }

        let slice = &notes[search_start..search_end];
        let mut result = Vec::with_capacity(slice.len());

        for n in slice {
            if n.end_tick() >= tick_start_u && n.start_tick <= tick_end_u {
                result.push((
                    n.start_tick as f32,
                    n.key,
                    n.length() as f32,
                    n.velocity,
                    n.channel,
                ));
            }
        }

        result
    }

    /// 获取指定音轨的所有音符。
    pub fn get_track_notes(&self, track_id: u16) -> Vec<(f32, u8, f32, u8, u8)> {
        let tid = track_id as usize;
        match self.notes.get(tid) {
            Some(notes) if !notes.is_empty() => {
                let mut result = Vec::with_capacity(notes.len());
                for n in notes {
                    result.push((
                        n.start_tick as f32,
                        n.key,
                        n.length() as f32,
                        n.velocity,
                        n.channel,
                    ));
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
    ) -> Vec<(f32, u8, f32, u8, u8)> {
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

            let search_start = notes.partition_point(|n| {
                n.start_tick < tick_start_u.saturating_sub(TICK_SEARCH_BUFFER)
            });
            let search_end = notes.len().min(
                search_start
                    + notes[search_start..].partition_point(|n| n.start_tick <= tick_end_u),
            );

            if search_start >= search_end {
                continue;
            }

            for n in &notes[search_start..search_end] {
                if n.end_tick() >= tick_start_u && n.start_tick <= tick_end_u {
                    all_notes.push((
                        n.start_tick as f32,
                        n.key,
                        n.length() as f32,
                        n.velocity,
                        n.channel,
                    ));
                }
            }
        }

        all_notes.sort_by(|a, b| a.0.total_cmp(&b.0));
        all_notes
    }

    /// 获取音轨数量
    #[inline]
    pub fn track_count(&self) -> usize {
        self.track_count as usize
    }

    /// 获取指定音轨的名称
    #[inline]
    pub fn track_name(&self, track_id: usize) -> Option<&str> {
        self.track_names.get(track_id).and_then(|n| n.as_deref())
    }

    /// 获取指定音轨的预解析音符引用。
    #[inline]
    pub fn track_notes(&self, track_id: usize) -> &[NoteEvent] {
        self.notes
            .get(track_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 在指定音轨按 start_tick 升序插入一个音符（保持每轨有序不变式）。
    /// 若 track_id 越界（音轨不存在）返回 false；成功返回 true。
    /// 同 start_tick 的音符插到已存在同 tick 音符之后（稳定插入）。
    pub fn insert_note(&mut self, track_id: usize, note: NoteEvent) -> bool {
        let Some(track_notes) = self.notes.get_mut(track_id) else {
            return false;
        };
        // 二分定位稳定插入点：分区谓词 <= 保证同 tick 音符插到已存在同 tick 之后
        let idx = track_notes.partition_point(|n| n.start_tick <= note.start_tick);
        track_notes.insert(idx, note);
        true
    }

    /// 删除指定音轨指定索引处的音符，返回被删除的音符副本。
    /// track_id 越界或 index 越界返回 None。
    pub fn remove_note(&mut self, track_id: usize, index: usize) -> Option<NoteEvent> {
        let track_notes = self.notes.get_mut(track_id)?;
        if index >= track_notes.len() {
            return None;
        }
        Some(track_notes.remove(index))
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
    pub fn track_notes_mut(&mut self, track_id: usize) -> Option<&mut Vec<NoteEvent>> {
        self.notes.get_mut(track_id)
    }
}

/// MidiDocument 可写 API 单元测试（独立文件，保持本文件 < 400 行）
#[cfg(test)]
#[path = "document_write_tests.rs"]
mod tests;
