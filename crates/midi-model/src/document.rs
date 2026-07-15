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
const TICK_SEARCH_BUFFER: u32 = 19200;

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
    /// MIDI 控制事件（CC / PC / PB），以 midly PackedControlEvent 紧凑存储
    pub control_events: Vec<midly::loader::PackedControlEvent>,
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
        let total_notes: usize = self.notes.iter().map(|v| v.len()).sum();
        f.debug_struct("MidiDocument")
            .field("track_count", &self.track_count)
            .field("total_ticks", &self.total_ticks)
            .field("total_notes", &total_notes)
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
            let mut control_events: Vec<midly::loader::PackedControlEvent> = Vec::new();
            let mut total_notes: u64 = 0;
            let mut total_ticks: u32 = 0;

            midly::loader::extract_notes_and_control_events_per_track_streaming_from_bytes(
                file_bytes,
                |track_idx, packed_notes, tempos, ctrls| {
                    if track_idx >= notes.len() {
                        notes.resize_with(track_idx + 1, Vec::new);
                    }

                    let mut track_notes: Vec<NoteEvent> =
                        packed_notes.into_iter().map(NoteEvent::from).collect();
                    if let Some(last) = track_notes.iter().max_by_key(|n| n.end_tick) {
                        total_ticks = total_ticks.max(last.end_tick);
                    }
                    if track_notes.len() > 1 {
                        track_notes.sort_unstable_by_key(|n| n.start_tick);
                    }
                    total_notes += track_notes.len() as u64;
                    notes[track_idx] = track_notes;

                    for tempo in tempos {
                        if !all_tempo_changes.iter().any(|(t, _)| *t == tempo.0) {
                            all_tempo_changes.push(tempo);
                        }
                    }
                    control_events.extend_from_slice(&ctrls);
                },
            )
            .map_err(|e| LoaderError::MidiParse(format!("提取音符失败: {e}")))?;

            if !all_tempo_changes.iter().any(|(t, _)| *t == 0) {
                all_tempo_changes.push((0u32, 120.0f32));
            }
            all_tempo_changes.sort_unstable_by_key(|&(t, _)| t);
            control_events.sort_unstable_by_key(|e| e.tick);

            if let Some(cb) = progress {
                (cb)(0.75);
            }

            let track_count = notes.len() as u16;
            let tracks_manager = TrackManager::new(track_count);

            tracing::info!(
                "MidiDocument: 已加载 {} 个音符, {} 个控制事件, {} 音轨, {} ticks, {} tempo 变化",
                total_notes,
                control_events.len(),
                track_count,
                total_ticks,
                all_tempo_changes.len(),
            );

            if let Some(cb) = progress {
                (cb)(0.90);
            }

            Ok((
                Self {
                    notes,
                    tempo_changes: all_tempo_changes,
                    control_events,
                    track_names,
                    total_ticks,
                    track_count,
                    tracks: tracks_manager,
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
        let mut result = Vec::new();
        for (track_id, track_notes) in self.notes.iter().enumerate() {
            let track_id_u16 = track_id as u16;
            for note in track_notes {
                let [on, off] = note.to_compact_events(track_id_u16);
                let on_tick = on.delta_tick();
                let off_tick = off.delta_tick();
                if on_tick >= from_tick && on_tick < to_tick {
                    result.push(on);
                }
                if off_tick >= from_tick && off_tick < to_tick {
                    result.push(off);
                }
                if result.len() >= limit {
                    return result;
                }
            }
        }
        result
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
}
