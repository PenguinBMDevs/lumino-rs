//! MidiDocument — 解析后的 MIDI 文档（全内存紧凑存放）
//!
//! 使用 midly 提取音符后以 CompactEvent（12 bytes/event）紧凑存放。
//! events 按音轨连续存放（不做按 tick 排序），per-track range 为真实连续区间，
//! 避免 get_track_notes 扫描无关事件导致 O(N×T) 性能灾难。
//!
//! 多线程优化：使用 rayon 并行处理音轨级别的音符转换

use lumino_midi::compact::{CompactEvent, EventKind};

use super::error::MidiResult;
use super::track::TrackManager;

use std::path::Path;

/// 解析后的 MIDI 文档（全内存紧凑存放）
///
/// events 按音轨连续存放（PackedNote 的自然顺序），不做按 tick 排序。
/// `track_events_range` 为每轨事件的 start..end 真实连续区间。
/// `get_track_notes` 只需扫描本轨事件，无需 O(N×T) 全量遍历。
#[derive(Clone)]
pub struct MidiDocument {
    /// 所有事件按音轨连续存放（不做 tick 排序）
    pub events: Vec<CompactEvent>,
    /// per-track 索引：track_events_range[track_id] = (start_index, end_index)
    /// 因为 events 按音轨连续排列，此 range 为真实连续区间
    track_events_range: Vec<(usize, usize)>,
    /// 预提取的 tempo 变化（tick, bpm）
    pub tempo_changes: Vec<(u32, f32)>,
    /// 音轨名称（索引 = track_index）
    pub track_names: Vec<Option<String>>,
    /// MIDI 文件总 tick 数
    pub total_ticks: u32,
    /// 音轨数量
    pub track_count: u16,
    /// 音轨可见性管理
    pub tracks: TrackManager,
}

impl std::fmt::Debug for MidiDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiDocument")
            .field("track_count", &self.track_count)
            .field("total_ticks", &self.total_ticks)
            .field("events.len", &self.events.len())
            .finish()
    }
}

impl MidiDocument {
    /// 使用 midly 从 MIDI 文件加载并构建紧凑内存文档。
    ///
    /// 使用 per-track 提取避免全量 `Vec<PackedNote>` 中间态的峰值内存。
    /// 每个音轨的音符提取后立即转换为 `CompactEvent`，该音轨的中间数据即释放。
    ///
    /// 内存优化：原来 1 个 PackedNote (12B) + 2 个 CompactEvent (24B) = 36B/note 峰值，
    /// 现在只有 CompactEvent (24B/note) 在构造过程中常驻，per-track PackedNote 用完即丢。
    pub fn from_notes_file<P: AsRef<Path>>(
        midi_path: P,
        progress: Option<&dyn Fn(f64)>,
    ) -> MidiResult<Self> {
        crate::memory_monitor::MemoryMonitor::global().check();

        let path = midi_path.as_ref();

        if let Some(cb) = progress {
            (cb)(0.05);
        }

        let file_bytes = std::fs::read(path).map_err(super::error::MidiError::Io)?;

        if let Some(cb) = progress {
            (cb)(0.15);
        }

        let track_names = scan_track_names(&file_bytes);

        let (notes, tempo_changes) = midly::loader::extract_notes_from_bytes(&file_bytes)
            .map_err(|e| super::error::MidiError::Parse(format!("提取音符失败: {e}")))?;

        drop(file_bytes);

        if let Some(cb) = progress {
            (cb)(0.50);
        }

        Self::build_from_extracted_notes(notes, tempo_changes, track_names, progress)
    }

    pub(crate) fn build_from_extracted_notes(
        notes: Vec<midly::loader::PackedNote>,
        tempo_changes: Vec<(u32, f32)>,
        track_names: Vec<Option<String>>,
        progress: Option<&dyn Fn(f64)>,
    ) -> MidiResult<Self> {
        let (total_ticks, track_note_counts, total_note_count) =
            Self::build_track_statistics(&notes);

        if let Some(cb) = progress {
            (cb)(0.55);
        }

        let track_events_offset = Self::compute_track_offsets(&track_note_counts);
        let track_events = Self::convert_notes_parallel(&notes, &track_note_counts);

        drop(notes);

        let track_events_range =
            Self::build_track_event_ranges(&track_note_counts, &track_events_offset);
        let (events, all_tempo_changes) =
            Self::merge_events_with_tempos(track_events, total_note_count, tempo_changes);

        if let Some(cb) = progress {
            (cb)(0.75);
        }

        let mut events = events;
        events.shrink_to_fit();

        if let Some(cb) = progress {
            (cb)(0.90);
        }

        let track_count = track_note_counts.len() as u16;
        let tracks = TrackManager::new(track_count);

        tracing::info!(
            "MidiDocument: 已加载 {} 个事件, {} 音轨, {} ticks, {} tempo 变化 (多线程并行处理)",
            events.len(),
            track_count,
            total_ticks,
            all_tempo_changes.len(),
        );

        Ok(Self {
            events,
            track_events_range,
            tempo_changes: all_tempo_changes,
            track_names,
            total_ticks,
            track_count,
            tracks,
        })
    }

    /// 阶段1：统计每个音轨的音符数量和总 ticks
    fn build_track_statistics(
        notes: &[midly::loader::PackedNote],
    ) -> (u32, Vec<u64>, usize) {
        let mut total_ticks: u32 = 0;
        let mut track_note_counts: Vec<u64> = Vec::new();
        let mut total_note_count: usize = 0;

        for note in notes {
            total_ticks = total_ticks.max(note.end_tick);
            let tid = note.track as usize;
            while track_note_counts.len() <= tid {
                track_note_counts.push(0);
            }
            track_note_counts[tid] += 1;
            total_note_count += 1;
        }

        (total_ticks, track_note_counts, total_note_count)
    }

    /// 计算每个音轨在最终 events 数组中的起始偏移
    fn compute_track_offsets(track_note_counts: &[u64]) -> Vec<usize> {
        let mut offsets = Vec::with_capacity(track_note_counts.len());
        let mut offset: usize = 0;
        for count in track_note_counts {
            offsets.push(offset);
            offset += *count as usize * 2;
        }
        offsets
    }

    /// 阶段2：并行处理 - 按音轨分组并转换为 CompactEvent
    fn convert_notes_parallel(
        notes: &[midly::loader::PackedNote],
        track_note_counts: &[u64],
    ) -> Vec<Vec<CompactEvent>> {
        use rayon::prelude::*;

        let track_count = track_note_counts.len();
        let mut track_note_indices: Vec<Vec<usize>> = vec![Vec::new(); track_count];
        for (idx, note) in notes.iter().enumerate() {
            track_note_indices[note.track as usize].push(idx);
        }

        track_note_indices
            .par_iter()
            .map(|indices| {
                let mut events = Vec::with_capacity(indices.len() * 2);
                for &idx in indices {
                    let note = &notes[idx];
                    events.push(CompactEvent::new(
                        note.start_tick,
                        note.track,
                        EventKind::NoteOn,
                        0,
                        note.key as u16,
                        note.velocity as u16,
                    ));
                    events.push(CompactEvent::new(
                        note.end_tick,
                        note.track,
                        EventKind::NoteOff,
                        0,
                        note.key as u16,
                        note.velocity as u16,
                    ));
                }
                events.sort_by_key(|e| e.delta_tick());
                events
            })
            .collect()
    }

    /// 阶段3：合并所有音轨的事件 + tempo 事件
    fn merge_events_with_tempos(
        track_events: Vec<Vec<CompactEvent>>,
        total_note_count: usize,
        tempo_changes: Vec<(u32, f32)>,
    ) -> (Vec<CompactEvent>, Vec<(u32, f32)>) {
        let estimated_capacity = total_note_count
            .saturating_mul(2)
            .saturating_add(tempo_changes.len());
        let mut events: Vec<CompactEvent> = Vec::with_capacity(estimated_capacity);

        for track_ev in track_events {
            events.extend(track_ev);
        }

        // 处理 tempo 变化
        let mut all_tempo_changes: Vec<(u32, f32)> = vec![(0u32, 120.0f32)];
        for &(tick, bpm) in &tempo_changes {
            if !all_tempo_changes.iter().any(|(t, _)| *t == tick) {
                all_tempo_changes.push((tick, bpm));
            }
        }
        all_tempo_changes.sort_unstable_by_key(|&(t, _)| t);

        for &(tick, bpm) in &all_tempo_changes {
            let tempo_microseconds = if bpm > 0.0 {
                (60_000_000.0 / bpm) as u32
            } else {
                500_000
            };
            events.push(CompactEvent::new(
                tick,
                0,
                EventKind::Tempo,
                0,
                (tempo_microseconds & 0xFFFF) as u16,
                ((tempo_microseconds >> 16) & 0xFFFF) as u16,
            ));
        }

        (events, all_tempo_changes)
    }

    /// 构建音轨事件范围索引
    fn build_track_event_ranges(
        track_note_counts: &[u64],
        track_events_offset: &[usize],
    ) -> Vec<(usize, usize)> {
        track_note_counts
            .iter()
            .enumerate()
            .map(|(i, count)| {
                let start = track_events_offset[i];
                let end = start + *count as usize * 2;
                (start, end)
            })
            .collect()
    }

    /// 获取总 tick 数
    #[inline]
    pub fn total_ticks(&self) -> u32 {
        self.total_ticks
    }

    /// 获取所有事件
    #[inline]
    pub fn all_events(&self) -> &[CompactEvent] {
        &self.events
    }

    /// 获取指定音轨事件在 events 中的连续 range
    /// 返回值: (start_index, end_index)，音轨不存在时返回 (0, 0)
    #[inline]
    pub fn track_events_range(&self, track_id: u16) -> (usize, usize) {
        let tid = track_id as usize;
        self.track_events_range.get(tid).copied().unwrap_or((0, 0))
    }

    /// 轻量获取指定音轨的音符数（O(notes_in_track)，零分配）
    /// 仅扫描 NoteOn 事件计数，不提取任何音符数据
    pub fn track_note_count(&self, track_id: u16) -> u64 {
        let (start, end) = self.track_events_range(track_id);
        if start >= end {
            return 0;
        }
        let mut count: u64 = 0;
        for ev in &self.events[start..end] {
            if matches!(ev.kind(), EventKind::NoteOn) {
                count += 1;
            }
        }
        count
    }

    /// 获取指定音轨的所有事件（O(events_in_track)，连续 range 直接切片）
    pub fn get_track_events(&self, track_id: u16) -> Vec<CompactEvent> {
        let tid = track_id as usize;
        let (start, end) = self.track_events_range.get(tid).copied().unwrap_or((0, 0));
        if start >= end {
            return Vec::new();
        }
        self.events[start..end].to_vec()
    }

    /// 线性扫描指定 tick 范围的事件（events 按音轨连续不排序，无法二分查找）
    pub fn get_events_in_range(
        &self,
        from_tick: u32,
        to_tick: u32,
        max_events: usize,
    ) -> Vec<CompactEvent> {
        let limit = if max_events == 0 {
            usize::MAX
        } else {
            max_events
        };
        let mut result = Vec::new();
        for ev in &self.events {
            let t = ev.delta_tick();
            if t >= from_tick && t < to_tick {
                result.push(*ev);
                if result.len() >= limit {
                    break;
                }
            }
        }
        result
    }

    /// 检查指定音轨在指定范围内是否有事件
    pub fn has_track_events_in_range(&self, track_id: u16, from_tick: u32, to_tick: u32) -> bool {
        let tid = track_id as usize;
        let (start, end) = self.track_events_range.get(tid).copied().unwrap_or((0, 0));
        if start >= end {
            return false;
        }
        for ev in &self.events[start..end] {
            let t = ev.delta_tick();
            if t >= from_tick && t < to_tick {
                return true;
            }
        }
        false
    }

    /// 获取指定音轨在指定 tick 范围内的音符。
    /// 利用预排序的 events 做二分查找 + 线性扫描，O(log N + K) 而非 O(N)。
    /// 
    /// 优化：使用固定大小数组替代 HashMap，避免内存分配。
    pub fn get_track_notes_in_range(
        &self,
        track_id: u16,
        tick_start: f32,
        tick_end: f32,
    ) -> Vec<(f32, u8, f32, u8, u8)> {
        let tid = track_id as usize;
        let (range_start, range_end) = self.track_events_range.get(tid).copied().unwrap_or((0, 0));
        if range_start >= range_end {
            return Vec::new();
        }

        let events = &self.events[range_start..range_end];
        let tick_start_u = tick_start as u32;
        let tick_end_u = tick_end as u32;

        use crate::midi::constants::TICK_SEARCH_BUFFER;
        
        // events 已按 tick 排序，二分查找起始位置
        let search_start =
            events.partition_point(|e| e.delta_tick() < tick_start_u.saturating_sub(TICK_SEARCH_BUFFER));
        let search_end = events.len().min(
            search_start + events[search_start..].partition_point(|e| e.delta_tick() <= tick_end_u),
        );

        if search_start >= search_end {
            return Vec::new();
        }

        use crate::midi::constants::{MAX_CONCURRENT_NOTES, MIDI_CHANNEL_COUNT, MIDI_KEY_RANGE};
        
        // 使用固定大小数组替代 HashMap：256 keys × 16 channels = 4096
        // 支持扩展 MIDI 标准 (0-255 key range)
        // 每个条目存储 (start_tick, velocity, channel, is_active)
        let mut active_notes: [(u32, u8, u8, bool); MAX_CONCURRENT_NOTES] = [(0, 0, 0, false); MAX_CONCURRENT_NOTES];
        let mut notes = Vec::new();

        #[inline]
        fn note_index(channel: u8, key: u8) -> usize {
            (channel as usize) * MIDI_KEY_RANGE as usize + (key as usize)
        }

        for ev in &events[search_start..search_end] {
            let tick = ev.delta_tick();
            let key = ev.param1() as u8;
            let vel = ev.param2() as u8;
            let channel = ev.channel();
            let idx = note_index(channel, key);

            match ev.kind() {
                EventKind::NoteOn if vel > 0 => {
                    if active_notes[idx].3 {
                        // 有未关闭的音符，先关闭它
                        let (st, pv, pc, _) = active_notes[idx];
                        let len = tick.saturating_sub(st) as f32;
                        if st as f32 + len >= tick_start && st as f32 <= tick_end {
                            notes.push((st as f32, key, len, pv, pc));
                        }
                    }
                    active_notes[idx] = (tick, vel, channel, true);
                }
                EventKind::NoteOn | EventKind::NoteOff => {
                    if active_notes[idx].3 {
                        let (st, pv, pc, _) = active_notes[idx];
                        let len = tick.saturating_sub(st) as f32;
                        if st as f32 + len >= tick_start && st as f32 <= tick_end {
                            notes.push((st as f32, key, len, pv, pc));
                        }
                        active_notes[idx].3 = false;
                    }
                }
                _ => {}
            }
        }

        // 未关闭的 NoteOn，用 search_end 的 tick 作为结束
        let last_tick = if search_end < events.len() {
            events[search_end].delta_tick()
        } else {
            events.last().map(|e| e.delta_tick()).unwrap_or(0)
        };
        
        for channel in 0..MIDI_CHANNEL_COUNT {
            for key in 0..=u8::MAX {
                let idx = note_index(channel, key);
                if active_notes[idx].3 {
                    let (st, vel, ch, _) = active_notes[idx];
                    let len = last_tick.saturating_sub(st) as f32;
                    if st as f32 + len >= tick_start && st as f32 <= tick_end {
                        notes.push((st as f32, key, len, vel, ch));
                    }
                }
            }
        }

        notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        notes
    }

    /// 获取指定音轨的所有音符（仅扫描本轨事件，O(notes_in_track)）
    ///
    /// 注意：events 数组在加载时已按 tick 排序（见 from_notes_file），
    /// 本函数不用再次排序，直接线性扫描。
    /// 
    /// 优化：使用固定大小数组替代 HashMap，避免内存分配。
    pub fn get_track_notes(&self, track_id: u16) -> Vec<(f32, u8, f32, u8, u8)> {
        let tid = track_id as usize;
        let (start, end) = self.track_events_range.get(tid).copied().unwrap_or((0, 0));
        if start >= end {
            return Vec::new();
        }

        let events = &self.events[start..end];
        
        // 使用固定大小数组替代 HashMap：256 keys × 16 channels = 4096
        // 支持扩展 MIDI 标准 (0-255 key range)
        const MAX_NOTES: usize = 256 * 16;
        let mut active_notes: [(u32, u8, u8, bool); MAX_NOTES] = [(0, 0, 0, false); MAX_NOTES];
        let mut notes = Vec::new();
        let last_tick = events.last().map(|e| e.delta_tick()).unwrap_or(0);

        #[inline]
        fn note_index(channel: u8, key: u8) -> usize {
            (channel as usize) * 256 + (key as usize)
        }

        for ev in events {
            let tick = ev.delta_tick();
            let key = ev.param1() as u8;
            let vel = ev.param2() as u8;
            let channel = ev.channel();
            let idx = note_index(channel, key);

            match ev.kind() {
                EventKind::NoteOn if vel > 0 => {
                    if active_notes[idx].3 {
                        let (st, pv, pc, _) = active_notes[idx];
                        notes.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                    }
                    active_notes[idx] = (tick, vel, channel, true);
                }
                EventKind::NoteOn | EventKind::NoteOff => {
                    if active_notes[idx].3 {
                        let (st, pv, pc, _) = active_notes[idx];
                        notes.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                        active_notes[idx].3 = false;
                    }
                }
                _ => {}
            }
        }
        
        for channel in 0..16u8 {
            for key in 0..=255u8 {
                let idx = note_index(channel, key);
                if active_notes[idx].3 {
                    let (st, vel, ch, _) = active_notes[idx];
                    notes.push((st as f32, key, last_tick.saturating_sub(st) as f32, vel, ch));
                }
            }
        }

        // 按 tick 排序输出，保证 instance 数组有序
        notes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        notes
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
}

/// 读取 VLQ（Variable Length Quantity）编码的值
fn read_vlq(data: &[u8], pos: &mut usize, end: usize) -> u32 {
    let mut value: u32 = 0;
    while *pos < end {
        let b = data[*pos];
        *pos += 1;
        value = (value << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            break;
        }
    }
    value
}

/// 在单个 MTrk chunk 中扫描 TrackName 事件
fn scan_track_name_in_chunk(data: &[u8], chunk_start: usize, chunk_end: usize) -> Option<String> {
    let mut pos = chunk_start;
    let mut last_status: u8 = 0;

    while pos < chunk_end {
        let _delta = read_vlq(data, &mut pos, chunk_end);
        if pos >= chunk_end {
            break;
        }

        let mut status = data[pos];
        if status >= 0x80 {
            pos += 1;
            if status < 0xF0 {
                last_status = status;
            }
        } else {
            status = last_status;
        }

        match status {
            0xFF => {
                if pos >= chunk_end {
                    break;
                }
                let meta_type = data[pos];
                pos += 1;
                let meta_len = read_vlq(data, &mut pos, chunk_end);
                let end = (pos + meta_len as usize).min(chunk_end);

                if meta_type == 0x03 {
                    let name_bytes = &data[pos..end];
                    let name = decode_midi_text(name_bytes);
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
                pos = end;
            }
            0xF0 | 0xF7 => {
                let sysex_len = read_vlq(data, &mut pos, chunk_end);
                pos = (pos + sysex_len as usize).min(chunk_end);
            }
            0xF8..=0xFE => {}
            _ if status < 0xF0 => {
                let skip = match status & 0xF0 {
                    0xC0 | 0xD0 => 1,
                    0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                    _ => 0,
                };
                pos = (pos + skip).min(chunk_end);
            }
            _ => break,
        }
    }
    None
}

/// 轻量扫描原始 MIDI 字节，提取所有音轨的 TrackName 事件。
/// 使用 encoding_rs 自动检测编码（UTF-8 → Shift-JIS → GBK → Latin-1）。
pub fn scan_track_names(data: &[u8]) -> Vec<Option<String>> {
    if data.len() < 14 {
        return Vec::new();
    }

    let data = if &data[..4] == b"RIFF" {
        let mthd_pos = data.windows(4).position(|w| w == b"MThd");
        match mthd_pos {
            Some(pos) => &data[pos..],
            None => return Vec::new(),
        }
    } else if &data[..4] == b"MThd" {
        data
    } else {
        return Vec::new();
    };

    if data.len() < 14 {
        return Vec::new();
    }

    let header_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let track_count = u16::from_be_bytes([data[10], data[11]]) as usize;
    let header_total = 8 + header_len;
    if header_total > data.len() {
        return Vec::new();
    }

    let mut track_names = vec![None; track_count];
    let mut track_idx = 0;
    let mut offset = header_total;

    while track_idx < track_count && offset + 8 <= data.len() {
        if &data[offset..offset + 4] != b"MTrk" {
            let chunk_len =
                u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap_or([0; 4]))
                    as usize;
            offset += 8 + chunk_len;
            continue;
        }

        let chunk_len =
            u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap_or([0; 4])) as usize;
        offset += 8;
        let track_end = (offset + chunk_len).min(data.len());

        let name = scan_track_name_in_chunk(data, offset, track_end);
        if let Some(n) = name {
            track_names[track_idx] = Some(n);
        }

        track_idx += 1;
        offset = track_end;
    }

    track_names
}

/// 解码 MIDI 文本（尝试 UTF-8 → Shift-JIS → GBK → Latin-1）
fn decode_midi_text(bytes: &[u8]) -> String {
    use encoding_rs::*;

    // 1. 先检查纯 ASCII
    if bytes.is_ascii() {
        return unsafe { String::from_utf8_unchecked(bytes.to_vec()) };
    }

    // 2. 尝试 UTF-8
    if let Ok(s) = String::from_utf8(bytes.to_vec()) {
        return s;
    }

    // 3. 尝试常见日语编码 Shift-JIS
    let (cow, _) = SHIFT_JIS.decode_without_bom_handling(bytes);
    if !cow.contains('\u{FFFD}') {
        return cow.into_owned();
    }

    // 4. 尝试 GBK（简体中文）
    let (cow, _) = GBK.decode_without_bom_handling(bytes);
    if !cow.contains('\u{FFFD}') {
        return cow.into_owned();
    }

    // 5. 尝试 EUC-JP
    let (cow, _) = EUC_JP.decode_without_bom_handling(bytes);
    if !cow.contains('\u{FFFD}') {
        return cow.into_owned();
    }

    // 6. 回退到 Latin-1（逐字节映射）
    bytes.iter().map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_simple_midi_bytes() -> Vec<u8> {
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track_data = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x0D, 0x00, 0x90, 0x3C, 0x64, 0x83, 0x60,
            0x80, 0x3C, 0x00, 0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut midi = Vec::with_capacity(header.len() + track_data.len());
        midi.extend_from_slice(&header);
        midi.extend_from_slice(&track_data);
        midi
    }

    #[test]
    fn test_from_notes_file() {
        let bytes = create_simple_midi_bytes();
        let tmp = std::env::temp_dir().join("doc_test.mid");
        std::fs::write(&tmp, &bytes).expect("测试：写入临时文件失败");

        let doc = MidiDocument::from_notes_file(&tmp, None).expect("测试：加载MIDI文档失败");
        assert_eq!(doc.track_count(), 1);
        assert!(doc.total_ticks > 0);
        assert!(!doc.events.is_empty());

        let evs = doc.get_track_events(0);
        assert!(!evs.is_empty());

        let notes = doc.get_track_notes(0);
        assert!(!notes.is_empty());

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_get_events_in_range() {
        let bytes = create_simple_midi_bytes();
        let tmp = std::env::temp_dir().join("doc_range.mid");
        std::fs::write(&tmp, &bytes).expect("测试：写入临时文件失败");

        let doc = MidiDocument::from_notes_file(&tmp, None).expect("测试：加载MIDI文档失败");
        let events = doc.get_events_in_range(0, 1000, 0);
        assert!(!events.is_empty());

        let empty = doc.get_events_in_range(99999, 100000, 0);
        assert!(empty.is_empty());

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_track_notes_contiguous_range() {
        // Verify per-track ranges are contiguous (no interleaving)
        let bytes = create_simple_midi_bytes();
        let tmp = std::env::temp_dir().join("doc_contig.mid");
        std::fs::write(&tmp, &bytes).expect("测试：写入临时文件失败");

        let doc = MidiDocument::from_notes_file(&tmp, None).expect("测试：加载MIDI文档失败");
        // get_track_events should return events with matching track_id only
        let evs = doc.get_track_events(0);
        for ev in &evs {
            assert_eq!(
                ev.track_id(),
                0,
                "all events in get_track_events(0) must have track_id=0"
            );
        }

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_decode_midi_text() {
        // ASCII
        assert_eq!(decode_midi_text(b"Piano"), "Piano");

        // UTF-8 Chinese
        let utf8 = "钢琴".as_bytes();
        assert_eq!(decode_midi_text(utf8), "钢琴");

        // Shift-JIS (Japanese for "piano")
        let sjis = [0x83, 0x70, 0x83, 0x41, 0x83, 0x6E]; // "ピアノ" in Shift-JIS
        let decoded = decode_midi_text(&sjis);
        assert!(!decoded.is_empty(), "Shift-JIS should decode to something");
    }

    #[test]
    fn test_scan_track_names_empty() {
        let names = scan_track_names(&[]);
        assert!(names.is_empty());
    }

    #[test]
    fn test_scan_track_names_invalid() {
        let names = scan_track_names(b"NOTMIDI");
        assert!(names.is_empty());
    }

    #[test]
    fn test_scan_track_names_single_track() {
        // MThd header (format=0, tracks=1, division=480)
        let header = [
            0x4D, 0x54, 0x68, 0x64,  // "MThd"
            0x00, 0x00, 0x00, 0x06,  // header length = 6
            0x00, 0x00,              // format 0
            0x00, 0x01,              // 1 track
            0x01, 0xE0,              // division = 480
        ];
        // MTrk chunk with TrackName "Piano"
        // delta=0, meta FF 03 05 "Piano", delta=0, end-of-track FF 2F 00
        let track = [
            0x4D, 0x54, 0x72, 0x6B,  // "MTrk"
            0x00, 0x00, 0x00, 0x0F,  // chunk length = 15
            0x00,                     // delta=0
            0xFF, 0x03, 0x05,        // Meta TrackName, len=5
            0x50, 0x69, 0x61, 0x6E, 0x6F,  // "Piano"
            0x00,                     // delta=0
            0xFF, 0x2F, 0x00,        // End of Track
        ];
        let mut midi = Vec::new();
        midi.extend_from_slice(&header);
        midi.extend_from_slice(&track);

        let names = scan_track_names(&midi);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], Some("Piano".to_string()));
    }

    #[test]
    fn test_scan_track_names_no_track_name() {
        let header = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06,
            0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x04,
            0x00, 0xFF, 0x2F, 0x00,  // Just end-of-track
        ];
        let mut midi = Vec::new();
        midi.extend_from_slice(&header);
        midi.extend_from_slice(&track);

        let names = scan_track_names(&midi);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], None);
    }

    #[test]
    fn test_scan_track_names_riff_wrapper() {
        // RIFF wrapper around a standard MIDI file
        let riff_header = [
            0x52, 0x49, 0x46, 0x46,  // "RIFF"
            0x00, 0x00, 0x00, 0x00,  // size (placeholder)
            0x52, 0x4D, 0x49, 0x44,  // "RMID"
        ];
        let mthd = [
            0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06,
            0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
        ];
        let track = [
            0x4D, 0x54, 0x72, 0x6B, 0x00, 0x00, 0x00, 0x0F,
            0x00, 0xFF, 0x03, 0x05, 0x50, 0x69, 0x61, 0x6E, 0x6F,
            0x00, 0xFF, 0x2F, 0x00,
        ];
        let mut midi = Vec::new();
        midi.extend_from_slice(&riff_header);
        midi.extend_from_slice(&mthd);
        midi.extend_from_slice(&track);

        let names = scan_track_names(&midi);
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], Some("Piano".to_string()));
    }
}
