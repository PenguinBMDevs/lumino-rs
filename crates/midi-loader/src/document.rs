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
    ///
    /// 便捷函数：读取文件字节后委托给 [`Self::from_notes_bytes`]。
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
    ///
    /// 使用 per-track 提取避免全量 `Vec<PackedNote>` 中间态的峰值内存。
    /// 每个音轨的音符提取后立即转换为 `NoteEvent`，该音轨的中间数据即释放。
    ///
    /// 返回 `(document, division, total_notes)`，调用方只需读取一次文件。
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

        let track_names = scan::scan_track_names(file_bytes);

        if let Some(cb) = progress {
            (cb)(0.15);
        }

        let tracks =
            midly::loader::extract_notes_and_control_events_per_track_from_bytes(file_bytes)
                .map_err(|e| LoaderError::MidiParse(format!("提取音符失败: {e}")))?;

        let total_notes = tracks.iter().map(|t| t.notes.len() as u64).sum();

        if let Some(cb) = progress {
            (cb)(0.50);
        }

        let doc = Self::build_from_extracted_tracks(tracks, track_names, progress)?;
        Ok((doc, division, total_notes))
    }

    pub(crate) fn build_from_extracted_tracks(
        tracks: Vec<midly::loader::TrackExtractResult>,
        track_names: Vec<Option<String>>,
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<Self> {
        use rayon::prelude::*;

        let track_count = tracks.len();
        let (total_ticks, _track_note_counts, total_note_count) =
            Self::build_track_statistics(&tracks);

        // 预合并 tempo 和控制事件（在移动 tracks 之前完成）
        let all_tempo_changes = Self::merge_tempo_changes(&tracks);
        let total_control_events = tracks.iter().map(|t| t.control_events.len()).sum();
        let mut control_events = Vec::with_capacity(total_control_events);
        for track in &tracks {
            control_events.extend_from_slice(&track.control_events);
        }
        control_events.sort_unstable_by_key(|e| e.tick);

        if let Some(cb) = progress {
            (cb)(0.55);
        }

        // 按轨并行构建 NoteEvent，每轨按 start_tick 排序
        let notes: Vec<Vec<NoteEvent>> = tracks
            .into_par_iter()
            .map(|track| {
                let mut track_notes: Vec<NoteEvent> =
                    track.notes.into_iter().map(NoteEvent::from).collect();
                if track_notes.len() > 1 {
                    track_notes.sort_unstable_by_key(|n| n.start_tick);
                }
                track_notes
            })
            .collect();

        if let Some(cb) = progress {
            (cb)(0.75);
        }

        let track_count_u16 = track_count as u16;
        let tracks_manager = TrackManager::new(track_count_u16);

        tracing::info!(
            "MidiDocument: 已加载 {} 个音符, {} 个控制事件, {} 音轨, {} ticks, {} tempo 变化 (多线程并行处理)",
            total_note_count,
            control_events.len(),
            track_count_u16,
            total_ticks,
            all_tempo_changes.len(),
        );

        Ok(Self {
            notes,
            tempo_changes: all_tempo_changes,
            control_events,
            track_names,
            total_ticks,
            track_count: track_count_u16,
            tracks: tracks_manager,
        })
    }

    /// 阶段1：统计每个音轨的音符数量和总 ticks
    fn build_track_statistics(
        tracks: &[midly::loader::TrackExtractResult],
    ) -> (u32, Vec<u64>, usize) {
        let mut total_ticks: u32 = 0;
        let mut track_note_counts: Vec<u64> = Vec::with_capacity(tracks.len());
        let mut total_note_count: usize = 0;

        for track in tracks {
            let count = track.notes.len() as u64;
            track_note_counts.push(count);
            total_note_count += count as usize;

            if let Some(last_note) = track.notes.last() {
                total_ticks = total_ticks.max(last_note.end_tick);
            }
        }

        (total_ticks, track_note_counts, total_note_count)
    }

    /// 合并每轨的速度变化，按 tick 去重并补齐 tick 0 默认值。
    fn merge_tempo_changes(tracks: &[midly::loader::TrackExtractResult]) -> Vec<(u32, f32)> {
        let mut all_tempo_changes: Vec<(u32, f32)> = Vec::new();

        for track in tracks {
            for &(tick, bpm) in &track.tempo_changes {
                if !all_tempo_changes.iter().any(|(t, _)| *t == tick) {
                    all_tempo_changes.push((tick, bpm));
                }
            }
        }

        // MIDI 标准默认速度为 120 BPM。如果 MIDI 文件提供了 tick 0 处的 tempo 事件，
        // 则使用文件速度；否则插入默认值确保至少有一个有效的起始速度
        if !all_tempo_changes.iter().any(|(t, _)| *t == 0) {
            all_tempo_changes.push((0u32, 120.0f32));
        }

        all_tempo_changes.sort_unstable_by_key(|&(t, _)| t);
        all_tempo_changes
    }

    /// 获取总 tick 数
    #[inline]
    pub fn total_ticks(&self) -> u32 {
        self.total_ticks
    }

    /// 获取所有 CompactEvent（按需从 NoteEvent 实时构造）。
    ///
    /// 注意：这会为所有音符分配 NoteOn + NoteOff 事件，内存开销较大，
    /// 仅用于兼容性路径或测试；常规查询请使用 `track_notes`。
    pub fn all_events(&self) -> Vec<lumino_midi_io::compact::CompactEvent> {
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
    pub fn get_track_events(&self, track_id: u16) -> Vec<lumino_midi_io::compact::CompactEvent> {
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
    ) -> Vec<lumino_midi_io::compact::CompactEvent> {
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
    ///
    /// 返回格式：(start_tick, key, length, velocity, channel)
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

        use crate::constants::TICK_SEARCH_BUFFER;

        // 二分查找：找到第一个可能落在范围内的音符
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
    ///
    /// 返回格式：(start_tick, key, length, velocity, channel)
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
    /// 一次性查询所有音轨，避免多次二分查找和多次 Vec 分配。
    ///
    /// # 参数
    /// - `exclude_track`: 要排除的音轨索引（通常是当前编辑音轨）
    /// - `tick_start / tick_end`: tick 视口范围
    pub fn get_all_notes_in_range_except(
        &self,
        exclude_track: usize,
        tick_start: f32,
        tick_end: f32,
    ) -> Vec<(f32, u8, f32, u8, u8)> {
        let tick_start_u = tick_start as u32;
        let tick_end_u = tick_end as u32;
        use crate::constants::TICK_SEARCH_BUFFER;

        // 预分配容量，避免多次扩容
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

        // 按 tick 排序，保证输出顺序稳定
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
    ///
    /// 返回 `&[NoteEvent]`，每个元素为完整的自包含音符（start_tick + end_tick + key + vel + channel）。
    /// 音符在每轨内按 `start_tick` 升序排列，可直接用 `partition_point` 二分查找。
    #[inline]
    pub fn track_notes(&self, track_id: usize) -> &[NoteEvent] {
        self.notes
            .get(track_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}
