//! MidiDocument — 解析后的 MIDI 文档（全内存紧凑存放）
//!
//! 使用 midly 提取音符后以 CompactEvent（12 bytes/event）紧凑存放。
//! events 按音轨连续存放（不做按 tick 排序），per-track range 为真实连续区间，
//! 避免 get_track_notes 扫描无关事件导致 O(N×T) 性能灾难。

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
    /// extract_notes_from_bytes 返回的 PackedNote 按音轨分组（track 0, track 1, ...），
    /// 本函数保持此分组顺序，不按 tick 排序，确保 per-track range 为真实连续区间。
    pub fn from_notes_file<P: AsRef<Path>>(
        midi_path: P,
        progress: Option<&dyn Fn(f64)>,
    ) -> MidiResult<Self> {
        let path = midi_path.as_ref();

        if let Some(cb) = progress { (cb)(0.05); }

        let file_bytes = std::fs::read(path).map_err(super::error::MidiError::Io)?;

        if let Some(cb) = progress { (cb)(0.15); }

        // 轻量扫描音轨名称
        let track_names = scan_track_names(&file_bytes);

        // 使用 midly 提取 PackedNote（返回结果按音轨分组）
        let (notes, tempo_changes) = midly::loader::extract_notes_from_bytes(&file_bytes)
            .map_err(|e| super::error::MidiError::Parse(format!("提取音符失败: {e}")))?;

        drop(file_bytes);

        if let Some(cb) = progress { (cb)(0.50); }

        // 统计音轨数和总 tick
        let mut total_ticks: u32 = 0;
        let mut track_count: u16 = 0;
        let mut track_note_counts: Vec<u64> = Vec::new();
        for note in &notes {
            total_ticks = total_ticks.max(note.end_tick);
            let tid = note.track as usize;
            track_count = track_count.max(note.track.saturating_add(1));
            // 扩展 track_note_counts 数组
            while track_note_counts.len() <= tid {
                track_note_counts.push(0);
            }
            track_note_counts[tid] += 1;
        }

        if let Some(cb) = progress { (cb)(0.55); }

        // 计算 per-track events 偏移量（PackedNote 与 events 的映射关系）
        // 每个 PackedNote 产生 2 个 CompactEvent（NoteOn + NoteOff）
        let mut track_events_offset: Vec<usize> = Vec::with_capacity(track_count as usize);
        let mut offset: usize = 0;
        for count in &track_note_counts {
            track_events_offset.push(offset);
            offset += *count as usize * 2;
        }

        let estimated_capacity = notes.len().saturating_mul(2).saturating_add(tempo_changes.len());
        let mut events: Vec<CompactEvent> = Vec::with_capacity(estimated_capacity);

        // PackedNote -> CompactEvent（按音轨连续写入）
        // events 将被填充到 track_events_offset[track] 开始的连续区域
        // 利用 track_events_offset 作为当前写入位置
        let mut write_pos = track_events_offset.clone();
        // 为 tempos 预留位置
        events.resize(estimated_capacity, CompactEvent::new(0, 0, EventKind::Tempo, 0, 0, 0));

        for note in &notes {
            let tid = note.track as usize;
            let pos = write_pos[tid];
            events[pos] = CompactEvent::new(
                note.start_tick, note.track, EventKind::NoteOn, 0,
                note.key as u16, note.velocity as u16,
            );
            events[pos + 1] = CompactEvent::new(
                note.end_tick, note.track, EventKind::NoteOff, 0,
                note.key as u16, note.velocity as u16,
            );
            write_pos[tid] = pos + 2;
        }

        // 追加 tempo 事件到 events 末尾
        let tempo_start = events.len() - tempo_changes.len();
        for (i, &(tick, bpm)) in tempo_changes.iter().enumerate() {
            let tempo_microseconds = if bpm > 0.0 {
                (60_000_000.0 / bpm) as u32
            } else {
                500_000
            };
            events[tempo_start + i] = CompactEvent::new(
                tick, 0, EventKind::Tempo, 0,
                (tempo_microseconds & 0xFFFF) as u16,
                ((tempo_microseconds >> 16) & 0xFFFF) as u16,
            );
        }

        drop(notes);

        if let Some(cb) = progress { (cb)(0.75); }

        // 构建 per-track range（真实连续区间）
        let mut track_events_range: Vec<(usize, usize)> = Vec::with_capacity(track_count as usize);
        for (i, count) in track_note_counts.iter().enumerate() {
            let start = track_events_offset[i];
            let end = start + *count as usize * 2;
            track_events_range.push((start, end));
        }

        events.shrink_to_fit();

        if let Some(cb) = progress { (cb)(0.90); }

        let tracks = TrackManager::new(track_count);

        tracing::info!(
            "MidiDocument: 已加载 {} 个事件, {} 音轨, {} ticks, {} tempo 变化",
            events.len(),
            track_count,
            total_ticks,
            tempo_changes.len(),
        );

        Ok(Self {
            events,
            track_events_range,
            tempo_changes,
            track_names,
            total_ticks,
            track_count,
            tracks,
        })
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

    /// 获取指定音轨的所有事件（O(events_in_track)，连续 range 直接切片）
    pub fn get_track_events(&self, track_id: u16) -> Vec<CompactEvent> {
        let tid = track_id as usize;
        let (start, end) = self
            .track_events_range
            .get(tid)
            .copied()
            .unwrap_or((0, 0));
        if start >= end { return Vec::new(); }
        self.events[start..end].to_vec()
    }

    /// 线性扫描指定 tick 范围的事件（events 按音轨连续不排序，无法二分查找）
    pub fn get_events_in_range(&self, from_tick: u32, to_tick: u32, max_events: usize) -> Vec<CompactEvent> {
        let limit = if max_events == 0 { usize::MAX } else { max_events };
        let mut result = Vec::new();
        for ev in &self.events {
            let t = ev.delta_tick();
            if t >= from_tick && t < to_tick {
                result.push(*ev);
                if result.len() >= limit { break; }
            }
        }
        result
    }

    /// 检查指定音轨在指定范围内是否有事件
    pub fn has_track_events_in_range(&self, track_id: u16, from_tick: u32, to_tick: u32) -> bool {
        let tid = track_id as usize;
        let (start, end) = self
            .track_events_range
            .get(tid)
            .copied()
            .unwrap_or((0, 0));
        if start >= end { return false; }
        for ev in &self.events[start..end] {
            let t = ev.delta_tick();
            if t >= from_tick && t < to_tick {
                return true;
            }
        }
        false
    }

    /// 获取指定音轨的所有音符（仅扫描本轨事件，O(notes_in_track)）
    pub fn get_track_notes(&self, track_id: u16) -> Vec<(f32, u8, f32, u8, u8)> {
        use std::collections::HashMap;

        let tid = track_id as usize;
        let (start, end) = self
            .track_events_range
            .get(tid)
            .copied()
            .unwrap_or((0, 0));
        if start >= end { return Vec::new(); }

        let events = &self.events[start..end];
        let mut active_notes: HashMap<(u8, u8), (u32, u8, u8)> = HashMap::new();
        let mut notes = Vec::new();

        for ev in events {
            let tick = ev.delta_tick();
            let key = ev.param1() as u8;
            let vel = ev.param2() as u8;
            let channel = ev.channel();

            match ev.kind() {
                EventKind::NoteOn if vel > 0 => {
                    if let Some((st, pv, pc)) = active_notes.remove(&(channel, key)) {
                        notes.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                    }
                    active_notes.insert((channel, key), (tick, vel, channel));
                }
                EventKind::NoteOn | EventKind::NoteOff => {
                    if let Some((st, pv, pc)) = active_notes.remove(&(channel, key)) {
                        notes.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                    }
                }
                _ => {}
            }
        }

        let last_tick = events.last().map(|e| e.delta_tick()).unwrap_or(0);
        for ((_ch, key), (st, vel, ch)) in active_notes {
            notes.push((st as f32, key, last_tick.saturating_sub(st) as f32, vel, ch));
        }

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

/// 轻量扫描原始 MIDI 字节，提取所有音轨的 TrackName 事件。
/// 使用 encoding_rs 自动检测编码（UTF-8 → Shift-JIS → GBK → Latin-1）。
pub fn scan_track_names(data: &[u8]) -> Vec<Option<String>> {
    if data.len() < 14 {
        return Vec::new();
    }

    let data = if &data[..4] == b"RIFF" {
        let mthd_pos = data.windows(4).position(|w| w == b"MThd");
        match mthd_pos { Some(pos) => &data[pos..], None => return Vec::new() }
    } else if &data[..4] == b"MThd" {
        data
    } else {
        return Vec::new();
    };

    if data.len() < 14 { return Vec::new(); }

    let header_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let track_count = u16::from_be_bytes([data[10], data[11]]) as usize;
    let header_total = 8 + header_len;
    if header_total > data.len() { return Vec::new(); }

    let mut track_names = vec![None; track_count];
    let mut track_idx = 0;
    let mut offset = header_total;

    while track_idx < track_count && offset + 8 <= data.len() {
        if &data[offset..offset + 4] != b"MTrk" {
            let chunk_len = u32::from_be_bytes(
                data[offset + 4..offset + 8].try_into().unwrap_or([0; 4]),
            ) as usize;
            offset += 8 + chunk_len;
            continue;
        }

        let chunk_len = u32::from_be_bytes(
            data[offset + 4..offset + 8].try_into().unwrap_or([0; 4]),
        ) as usize;
        offset += 8;
        let track_end = (offset + chunk_len).min(data.len());

        let mut pos = offset;
        let mut last_status: u8 = 0;

        while pos < track_end {
            // VLQ delta (skip)
                    let mut _delta: u32 = 0;
                    loop {
                        if pos >= track_end { break; }
                        let b = data[pos];
                        pos += 1;
                        _delta = (_delta << 7) | (b & 0x7F) as u32;
                        if b & 0x80 == 0 { break; }
                    }
                    if pos >= track_end { break; }

            let mut status = data[pos];
            if status >= 0x80 {
                pos += 1;
                if status < 0xF0 { last_status = status; }
            } else {
                status = last_status;
            }

            match status {
                0xFF => {
                    if pos >= track_end { break; }
                    let meta_type = data[pos];
                    pos += 1;
                    let mut meta_len: u32 = 0;
                    loop {
                        if pos >= track_end { break; }
                        let b = data[pos];
                        pos += 1;
                        meta_len = (meta_len << 7) | (b & 0x7F) as u32;
                        if b & 0x80 == 0 { break; }
                    }
                    let end = (pos + meta_len as usize).min(track_end);

                    if meta_type == 0x03 {
                        let name_bytes = &data[pos..end];
                        let name = decode_midi_text(name_bytes);
                        if !name.is_empty() {
                            track_names[track_idx] = Some(name);
                        }
                    }

                    pos = end;
                }
                0xF0 | 0xF7 => {
                    let mut sysex_len: u32 = 0;
                    loop {
                        if pos >= track_end { break; }
                        let b = data[pos];
                        pos += 1;
                        sysex_len = (sysex_len << 7) | (b & 0x7F) as u32;
                        if b & 0x80 == 0 { break; }
                    }
                    pos = (pos + sysex_len as usize).min(track_end);
                }
                0xF8..=0xFE => {}
                _ if status < 0xF0 => {
                    let msg_type = status & 0xF0;
                    let skip = match msg_type {
                        0xC0 | 0xD0 => 1,
                        0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
                        _ => 0,
                    };
                    pos = (pos + skip).min(track_end);
                }
                _ => break,
            }
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
            0x4D, 0x54, 0x68, 0x64,
            0x00, 0x00, 0x00, 0x06,
            0x00, 0x00, 0x00, 0x01,
            0x01, 0xE0,
        ];
        let track_data = [
            0x4D, 0x54, 0x72, 0x6B,
            0x00, 0x00, 0x00, 0x0D,
            0x00, 0x90, 0x3C, 0x64,
            0x83, 0x60, 0x80, 0x3C, 0x00,
            0x00, 0xFF, 0x2F, 0x00,
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
        std::fs::write(&tmp, &bytes).unwrap();

        let doc = MidiDocument::from_notes_file(&tmp, None).unwrap();
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
        std::fs::write(&tmp, &bytes).unwrap();

        let doc = MidiDocument::from_notes_file(&tmp, None).unwrap();
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
        std::fs::write(&tmp, &bytes).unwrap();

        let doc = MidiDocument::from_notes_file(&tmp, None).unwrap();
        // get_track_events should return events with matching track_id only
        let evs = doc.get_track_events(0);
        for ev in &evs {
            assert_eq!(ev.track_id(), 0, "all events in get_track_events(0) must have track_id=0");
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
}
