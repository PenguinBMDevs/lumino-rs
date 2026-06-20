//! MidiDocument — 解析后的 MIDI 文档（全内存紧凑存放）
//!
//! 使用 midly 提取音符后以 CompactEvent（12 bytes/event）紧凑存放。
//! events 按音轨连续存放（不做按 tick 排序），per-track range 为真实连续区间，
//! 避免 get_track_notes 扫描无关事件导致 O(N×T) 性能灾难。
//!
//! 多线程优化：使用 rayon 并行处理音轨级别的音符转换

use lumino_memory_monitor::MemoryMonitor;
use lumino_midi_io::compact::{CompactEvent, EventKind};

use crate::error::{LoaderError, LoaderResult};
use crate::note_info::NoteInfo;
use crate::track::TrackManager;

#[path = "document_scan.rs"]
pub(crate) mod scan;

use std::path::Path;
use std::sync::OnceLock;

/// 解析后的 MIDI 文档（全内存紧凑存放）
///
/// events 按音轨连续存放（PackedNote 的自然顺序），不做按 tick 排序。
/// `track_events_range` 为每轨事件的 start..end 真实连续区间。
/// `track_notes_cache` 按需懒加载（首次查询时从 events 计算），
/// 避免加载时预构建 160MB 冗余缓存。
#[derive(Clone)]
pub struct MidiDocument {
    /// 所有事件按音轨连续存放（不做 tick 排序）
    pub events: Vec<CompactEvent>,
    /// per-track 索引：track_events_range[track_id] = (start_index, end_index)
    /// 因为 events 按音轨连续排列，此 range 为真实连续区间
    track_events_range: Vec<(usize, usize)>,
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
    /// **懒加载音符缓存** — 每轨按 start_tick 排序的 NoteInfo 列表。
    ///
    /// 首次调用 `get_track_notes*` 时从 `events` 通过 active-table 扫描计算并缓存。
    /// 避免加载时预构建，节省 160MB（黑乐谱场景）。
    track_notes_cache: OnceLock<Vec<Vec<NoteInfo>>>,
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
    ) -> LoaderResult<Self> {
        MemoryMonitor::global().check();

        let path = midi_path.as_ref();

        if let Some(cb) = progress {
            (cb)(0.05);
        }

        let file_bytes = std::fs::read(path).map_err(LoaderError::Io)?;

        if let Some(cb) = progress {
            (cb)(0.15);
        }

        let track_names = scan::scan_track_names(&file_bytes);

        let (notes, tempo_changes, control_events) =
            midly::loader::extract_notes_and_control_events_from_bytes(&file_bytes)
                .map_err(|e| LoaderError::MidiParse(format!("提取音符失败: {e}")))?;

        drop(file_bytes);

        if let Some(cb) = progress {
            (cb)(0.50);
        }

        Self::build_from_extracted_notes(
            notes,
            tempo_changes,
            control_events,
            track_names,
            progress,
        )
    }

    pub(crate) fn build_from_extracted_notes(
        notes: Vec<midly::loader::PackedNote>,
        tempo_changes: Vec<(u32, f32)>,
        control_events: Vec<midly::loader::PackedControlEvent>,
        track_names: Vec<Option<String>>,
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<Self> {
        let (total_ticks, track_note_counts, total_note_count) =
            Self::build_track_statistics(&notes);

        if let Some(cb) = progress {
            (cb)(0.55);
        }

        let track_events_offset = Self::compute_track_offsets(&track_note_counts);
        let track_events = Self::convert_notes_parallel(&notes, &track_note_counts);

        // 不再预构建 track_notes_cache（160MB 冗余）。
        // 首次查询 `get_track_notes*` 时从 events 懒加载计算。
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
            "MidiDocument: 已加载 {} 个音符事件, {} 个控制事件, {} 音轨, {} ticks, {} tempo 变化 (多线程并行处理)",
            events.len(),
            control_events.len(),
            track_count,
            total_ticks,
            all_tempo_changes.len(),
        );

        Ok(Self {
            events,
            track_events_range,
            tempo_changes: all_tempo_changes,
            control_events,
            track_names,
            total_ticks,
            track_count,
            tracks,
            track_notes_cache: OnceLock::new(),
        })
    }

    /// 阶段1：统计每个音轨的音符数量和总 ticks
    fn build_track_statistics(notes: &[midly::loader::PackedNote]) -> (u32, Vec<u64>, usize) {
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
        let mut all_tempo_changes: Vec<(u32, f32)> = Vec::with_capacity(tempo_changes.len() + 1);

        // MIDI 标准默认速度为 120 BPM。如果 MIDI 文件提供了 tick 0 处的 tempo 事件，
        // 则使用文件速度；否则插入默认值确保至少有一个有效的起始速度
        if !tempo_changes.iter().any(|(t, _)| *t == 0) {
            all_tempo_changes.push((0u32, 120.0f32));
        }

        for &(tick, bpm) in &tempo_changes {
            if !all_tempo_changes.iter().any(|(t, _)| *t == tick) {
                all_tempo_changes.push((tick, bpm));
            }
        }
        all_tempo_changes.sort_unstable_by_key(|&(t, _)| t);

        for &(tick, bpm) in &all_tempo_changes {
            let tempo_microseconds = if bpm > 0.0 {
                crate::bpm_to_tempo(bpm as f64)
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

    /// 从 events 音轨切片计算 NoteInfo（首次查询时懒加载）。
    ///
    /// 使用固定大小数组（4096 = 16 channels × 256 keys）代替 HashMap 做 active-table。
    /// events 每轨已按 tick 排序，直接线性扫描即可配对 NoteOn/NoteOff。
    fn compute_track_notes_from_events(
        events: &[CompactEvent],
        track_events_range: &[(usize, usize)],
        track_count: usize,
    ) -> Vec<Vec<NoteInfo>> {
        use crate::constants::MIDI_KEY_RANGE;
        use lumino_midi_io::MIDI_CHANNEL_COUNT;

        let mut track_notes: Vec<Vec<NoteInfo>> = vec![Vec::new(); track_count];

        for (track_id, track_out) in track_notes.iter_mut().enumerate().take(track_count) {
            let (start, end) = track_events_range.get(track_id).copied().unwrap_or((0, 0));
            if start >= end {
                continue;
            }
            let track_events = &events[start..end];

            // active table: [channel * KEY_RANGE + key] = (start_tick, velocity, is_active)
            let mut active: Vec<(u32, u8, bool)> =
                vec![(0, 0, false); MIDI_CHANNEL_COUNT as usize * MIDI_KEY_RANGE as usize];
            let mut notes: Vec<NoteInfo> = Vec::new();

            for ev in track_events {
                let tick = ev.delta_tick();
                let key = ev.param1() as u8;
                let vel = ev.param2() as u8;
                let ch = ev.channel();
                let idx = (ch as usize) * (MIDI_KEY_RANGE as usize) + (key as usize);

                match ev.kind() {
                    EventKind::NoteOn if vel > 0 => {
                        if active[idx].2 {
                            // 同键重叠 NoteOn → 关闭上一个
                            let (st, pv, _) = active[idx];
                            notes.push(NoteInfo::new(st, tick.saturating_sub(st), key, pv, ch));
                        }
                        active[idx] = (tick, vel, true);
                    }
                    EventKind::NoteOn | EventKind::NoteOff if active[idx].2 => {
                        let (st, pv, _) = active[idx];
                        notes.push(NoteInfo::new(st, tick.saturating_sub(st), key, pv, ch));
                        active[idx].2 = false;
                    }
                    _ => {}
                }
            }

            // 扫描悬挂音符（有 NoteOn 无 NoteOff）
            let last_tick = track_events.last().map(|e| e.delta_tick()).unwrap_or(0);
            for ch in 0..MIDI_CHANNEL_COUNT as usize {
                for key in 0..MIDI_KEY_RANGE as usize {
                    let idx = ch * MIDI_KEY_RANGE as usize + key;
                    if active[idx].2 {
                        let (st, pv, _) = active[idx];
                        notes.push(NoteInfo::new(
                            st,
                            last_tick.saturating_sub(st).max(1),
                            key as u8,
                            pv,
                            ch as u8,
                        ));
                    }
                }
            }

            if notes.len() > 1 {
                notes.sort_by_key(|n| n.start_tick);
            }
            *track_out = notes;
        }

        track_notes
    }

    /// 获取或计算音符缓存（懒加载）。
    ///
    /// 首次调用时从 events 执行 active-table 扫描，之后返回缓存结果。
    fn get_or_compute_track_notes_cache(&self) -> &Vec<Vec<NoteInfo>> {
        self.track_notes_cache.get_or_init(|| {
            Self::compute_track_notes_from_events(
                &self.events,
                &self.track_events_range,
                self.track_count as usize,
            )
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

    /// 获取指定音轨事件在 events 中的连续 range
    /// 返回值: (start_index, end_index)，音轨不存在时返回 (0, 0)
    #[inline]
    pub fn track_events_range(&self, track_id: u16) -> (usize, usize) {
        let tid = track_id as usize;
        self.track_events_range.get(tid).copied().unwrap_or((0, 0))
    }

    /// 轻量获取指定音轨的音符数（懒加载，首次访问触发 active-table 扫描）
    pub fn track_note_count(&self, track_id: u16) -> u64 {
        let tid = track_id as usize;
        self.get_or_compute_track_notes_cache()
            .get(tid)
            .map(|notes| notes.len() as u64)
            .unwrap_or(0)
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
    ///
    /// 优化：使用 partition_point 二分查找替代线性扫描，O(log N) 而非 O(N)。
    /// events 在每轨内按 tick 排序（见 from_notes_file），可直接二分。
    pub fn has_track_events_in_range(&self, track_id: u16, from_tick: u32, to_tick: u32) -> bool {
        let tid = track_id as usize;
        let (start, end) = self.track_events_range.get(tid).copied().unwrap_or((0, 0));
        if start >= end {
            return false;
        }
        let events = &self.events[start..end];
        let search_start = events.partition_point(|e| e.delta_tick() < from_tick);
        if search_start >= events.len() {
            return false;
        }
        events[search_start..]
            .iter()
            .any(|e| e.delta_tick() < to_tick)
    }

    /// 获取指定音轨在指定 tick 范围内的音符（从懒加载 cache 读取）。
    ///
    /// 利用预排序的 cache 做二分查找 + 线性扫描，O(log N + K) 而非 O(N)。
    /// 首次调用时触发一次 active-table 扫描构建 cache。
    ///
    /// 返回格式：(start_tick, key, length, velocity, channel)
    pub fn get_track_notes_in_range(
        &self,
        track_id: u16,
        tick_start: f32,
        tick_end: f32,
    ) -> Vec<(f32, u8, f32, u8, u8)> {
        let tid = track_id as usize;
        let notes = match self.get_or_compute_track_notes_cache().get(tid) {
            Some(n) => n,
            None => return Vec::new(),
        };
        if notes.is_empty() {
            return Vec::new();
        }

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
            let end = n.end_tick();
            if end >= tick_start_u && n.start_tick <= tick_end_u {
                result.push((
                    n.start_tick as f32,
                    n.key,
                    n.length as f32,
                    n.velocity,
                    n.channel,
                ));
            }
        }

        result
    }

    /// 获取指定音轨的所有音符（从懒加载 cache 读取，首次触发 active-table 扫描）。
    ///
    /// 返回格式：(start_tick, key, length, velocity, channel)
    ///
    /// 与传统事件扫描相比：
    /// - **之前**：每次查询都扫描 NoteOn/NoteOff 事件 + active-table 配对
    /// - **之前**：预构建 cache 占 160MB
    /// - **现在**：首次查询时一次性 active-table 扫描构建 cache，后续零配对开销
    ///
    /// 对于黑乐谱（88M 事件 → 44M 音符），首次查询约数百毫秒，后续 track-switch 为零开销。
    pub fn get_track_notes(&self, track_id: u16) -> Vec<(f32, u8, f32, u8, u8)> {
        let tid = track_id as usize;
        match self.get_or_compute_track_notes_cache().get(tid) {
            Some(notes) if !notes.is_empty() => {
                let mut result = Vec::with_capacity(notes.len());
                for n in notes {
                    result.push((
                        n.start_tick as f32,
                        n.key,
                        n.length as f32,
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
    /// 从懒加载 cache 读取，首次调用时触发 active-table 扫描。
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
        let cache = self.get_or_compute_track_notes_cache();

        for track_idx in 0..self.track_count() {
            if track_idx == exclude_track {
                continue;
            }

            let notes = match cache.get(track_idx) {
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
                let end = n.end_tick();
                if end >= tick_start_u && n.start_tick <= tick_end_u {
                    all_notes.push((
                        n.start_tick as f32,
                        n.key,
                        n.length as f32,
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

    /// 共享音符收集逻辑：从一段已排序的事件切片中提取音符。
    /// 使用固定大小数组替代 HashMap：256 keys × 16 channels = 4096。
    ///
    /// **已不再被公开查询路径调用** — 所有 `get_track_notes*` 已切换到 `track_notes_cache`。
    /// 保留此方法作为 fallback/测试用。
    #[expect(dead_code)]
    fn collect_notes(events: &[CompactEvent]) -> Vec<(f32, u8, f32, u8, u8)> {
        use crate::constants::{MAX_CONCURRENT_NOTES, MIDI_KEY_RANGE};
        use lumino_midi_io::MIDI_CHANNEL_COUNT;

        let mut active_notes: [(u32, u8, u8, bool); MAX_CONCURRENT_NOTES] =
            [(0, 0, 0, false); MAX_CONCURRENT_NOTES];
        let mut notes = Vec::new();
        let last_tick = events.last().map(|e| e.delta_tick()).unwrap_or(0);

        for ev in events {
            let tick = ev.delta_tick();
            let key = ev.param1() as u8;
            let vel = ev.param2() as u8;
            let channel = ev.channel();
            let idx = (channel as usize) * (MIDI_KEY_RANGE as usize) + (key as usize);

            match ev.kind() {
                EventKind::NoteOn if vel > 0 => {
                    if active_notes[idx].3 {
                        let (st, pv, pc, _) = active_notes[idx];
                        notes.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                    }
                    active_notes[idx] = (tick, vel, channel, true);
                }
                EventKind::NoteOn | EventKind::NoteOff if active_notes[idx].3 => {
                    let (st, pv, pc, _) = active_notes[idx];
                    notes.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                    active_notes[idx].3 = false;
                }
                _ => {}
            }
        }

        for channel in 0..MIDI_CHANNEL_COUNT {
            for key in 0..=u8::MAX {
                let idx = (channel as usize) * (MIDI_KEY_RANGE as usize) + (key as usize);
                if active_notes[idx].3 {
                    let (st, vel, ch, _) = active_notes[idx];
                    notes.push((st as f32, key, last_tick.saturating_sub(st) as f32, vel, ch));
                }
            }
        }

        notes.sort_by(|a, b| a.0.total_cmp(&b.0));
        notes
    }

    /// 将音符收集到指定的 Vec 中，避免中间分配。
    /// 与 `collect_notes` 逻辑相同，但直接追加到传入的 Vec。
    #[expect(dead_code)]
    fn collect_notes_to(events: &[CompactEvent], out: &mut Vec<(f32, u8, f32, u8, u8)>) {
        use crate::constants::{MAX_CONCURRENT_NOTES, MIDI_KEY_RANGE};
        use lumino_midi_io::MIDI_CHANNEL_COUNT;

        let mut active_notes: [(u32, u8, u8, bool); MAX_CONCURRENT_NOTES] =
            [(0, 0, 0, false); MAX_CONCURRENT_NOTES];
        let last_tick = events.last().map(|e| e.delta_tick()).unwrap_or(0);

        for ev in events {
            let tick = ev.delta_tick();
            let key = ev.param1() as u8;
            let vel = ev.param2() as u8;
            let channel = ev.channel();
            let idx = (channel as usize) * (MIDI_KEY_RANGE as usize) + (key as usize);

            match ev.kind() {
                EventKind::NoteOn if vel > 0 => {
                    if active_notes[idx].3 {
                        let (st, pv, pc, _) = active_notes[idx];
                        out.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                    }
                    active_notes[idx] = (tick, vel, channel, true);
                }
                EventKind::NoteOn | EventKind::NoteOff if active_notes[idx].3 => {
                    let (st, pv, pc, _) = active_notes[idx];
                    out.push((st as f32, key, tick.saturating_sub(st) as f32, pv, pc));
                    active_notes[idx].3 = false;
                }
                _ => {}
            }
        }

        for channel in 0..MIDI_CHANNEL_COUNT {
            for key in 0..=u8::MAX {
                let idx = (channel as usize) * (MIDI_KEY_RANGE as usize) + (key as usize);
                if active_notes[idx].3 {
                    let (st, vel, ch, _) = active_notes[idx];
                    out.push((st as f32, key, last_tick.saturating_sub(st) as f32, vel, ch));
                }
            }
        }
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

    /// 获取指定音轨的预解析音符缓存引用
    ///
    /// 返回 `&[NoteInfo]`，每个元素为完整的自包含音符（start_tick + length + key + vel + channel）。
    /// 与 `get_track_notes` / `get_track_notes_in_range` 同源，但避免 tuple 分配。
    ///
    /// 音符在每轨内按 start_tick 升序排列，可直接用 `partition_point` 二分查找。
    /// 首次调用触发一次 active-table 扫描构建 cache。
    #[inline]
    pub fn track_notes(&self, track_id: usize) -> &[NoteInfo] {
        self.get_or_compute_track_notes_cache()
            .get(track_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}
