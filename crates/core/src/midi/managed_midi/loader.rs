//! 加载辅助函数

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::midi::MidiEvent;
use crate::midi::constants::DEFAULT_PPQN;
use crate::midi::managed_midi::{DiskTrackCache, TrackLocationSerde, TrackSummary};

/// 估算单个 MidiEvent 在内存中的大小（字节）
pub fn estimate_event_size(event: &MidiEvent) -> usize {
    match event {
        MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. } => 24,
        MidiEvent::ControlChange { .. } => 24,
        MidiEvent::ProgramChange { .. } => 16,
        MidiEvent::PitchBend { .. } => 16,
        MidiEvent::Tempo { .. } => 16,
        MidiEvent::TimeSignature { .. } => 16,
        MidiEvent::KeySignature { .. } => 16,
        MidiEvent::TrackName { name, .. } => 24 + name.len(),
        MidiEvent::Other { raw, .. } => 24 + raw.len(),
    }
}

/// 估算事件列表的内存占用
pub fn estimate_events_size(events: &[MidiEvent]) -> usize {
    // Vec 本身开销 (ptr + len + cap) ≈ 24 bytes
    let mut total = 24usize;
    for e in events {
        total += estimate_event_size(e);
    }
    total
}

/// 1 GB 内存上限（字节）
const MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;

/// 进度回调起始值 (1%)
const PROGRESS_START: f64 = 0.01;

/// 进度回调主要部分占比 (94%)
const PROGRESS_MAIN_RATIO: f64 = 0.94;

/// 启动后台磁盘写入线程
pub fn spawn_disk_writer(
    disk_cache_dir: PathBuf,
    disk_rx: mpsc::Receiver<(usize, Vec<MidiEvent>)>,
) -> std::thread::JoinHandle<Result<(), String>> {
    std::thread::spawn(move || -> Result<(), String> {
        for (track_idx, events) in disk_rx {
            let track_path = disk_cache_dir.join(format!("track_{:04x}.zst", track_idx));
            let serialized = bincode::serialize(&events).map_err(|e| format!("序列化失败: {e}"))?;
            let compressed = zstd::stream::encode_all(&mut &serialized[..], 3)
                .map_err(|e| format!("压缩失败: {e}"))?;
            let mut file_out =
                File::create(&track_path).map_err(|e| format!("创建缓存文件失败: {e}"))?;
            file_out
                .write_all(&compressed)
                .map_err(|e| format!("写入缓存失败: {e}"))?;
        }
        Ok(())
    })
}

/// 单个音轨的解析结果
pub struct ParsedTrack {
    pub events: Vec<MidiEvent>,
    pub note_count: u64,
    pub high_vel_count: u64,
    pub max_tick: u32,
}

/// 解析单个音轨的事件
pub fn parse_track_events_from_iter(
    track_idx: usize,
    event_iter: midly::EventIter,
) -> Result<ParsedTrack, String> {
    use crate::midi::managed_midi::MidiMemoryManager;

    let mut events = Vec::new();
    let mut current_tick = 0u32;
    let mut note_count = 0u64;
    let mut high_vel_count = 0u64;
    let mut max_tick = 0u32;

    for event_result in event_iter {
        let track_event =
            event_result.map_err(|e| format!("解析音轨 {} 事件失败: {e}", track_idx))?;

        current_tick = current_tick.saturating_add(u32::from(track_event.delta));

        if let Some(midi_event) =
            MidiMemoryManager::parse_track_event(track_idx, current_tick, &track_event.kind)
        {
            if current_tick > max_tick {
                max_tick = current_tick;
            }
            if let MidiEvent::NoteOn { velocity, .. } = &midi_event
                && *velocity > 0
            {
                note_count += 1;
                if *velocity > 1 {
                    high_vel_count += 1;
                }
            }
            events.push(midi_event);
        }
    }

    Ok(ParsedTrack {
        events,
        note_count,
        high_vel_count,
        max_tick,
    })
}

/// 创建音轨摘要
pub fn create_track_summary(
    track_idx: usize,
    event_count: u64,
    note_count: u64,
    high_vel_count: u64,
    max_tick: u32,
    memory_bytes: usize,
    in_memory: bool,
) -> TrackSummary {
    TrackSummary {
        track_index: track_idx,
        event_count,
        note_count,
        high_vel_note_count: high_vel_count,
        max_tick,
        memory_bytes,
        location: if in_memory {
            TrackLocationSerde::InMemory
        } else {
            TrackLocationSerde::OnDisk
        },
    }
}

/// 加载后的 MIDI 数据
pub struct LoadedMidiData {
    pub disk_cache: DiskTrackCache,
    pub summaries: Vec<TrackSummary>,
    pub in_memory_tracks: std::collections::HashMap<usize, Vec<MidiEvent>>,
    pub memory_used: usize,
    pub loaded_memory_limit: usize,
}

/// 从 MIDI 文件加载数据
pub fn load_midi_data(
    source_path: &Path,
    cache_base_dir: &Path,
    progress_callback: Option<&dyn Fn(f64)>,
    max_ram_bytes: Option<usize>,
) -> Result<LoadedMidiData, String> {
    let disk_cache = DiskTrackCache::new(cache_base_dir, source_path)
        .map_err(|e| format!("创建磁盘缓存失败: {e}"))?;

    // 读取文件到内存，避免 mmap 产生的 SIGBUS 等异常
    let file_data = std::fs::read(source_path).map_err(|e| format!("读取文件失败: {e}"))?;

    // 使用 midly::parse() 获取懒 TrackIter
    let (header, track_iter) =
        midly::parse(&file_data).map_err(|e| format!("解析 MIDI 头部失败: {e}"))?;

    let division = extract_division(&header);

    // 先收集所有 track 的 EventIter
    let event_iters: Vec<_> = track_iter
        .collect::<midly::Result<Vec<_>>>()
        .map_err(|e| format!("解析音轨块失败: {e}"))?;

    let track_count = event_iters.len();

    if let Some(cb) = progress_callback {
        cb(PROGRESS_START);
    }

    // 启动后台磁盘写入线程
    let (disk_tx, disk_rx) = mpsc::channel::<(usize, Vec<MidiEvent>)>();
    let disk_cache_dir = disk_cache.cache_dir().to_path_buf();
    let disk_writer = spawn_disk_writer(disk_cache_dir, disk_rx);

    // 逐个音轨解析事件，边统计边分配
    let memory_limit = max_ram_bytes.unwrap_or(MEMORY_LIMIT_BYTES);
    let loaded_memory_limit = memory_limit / 4;
    let initial_memory_limit = memory_limit - loaded_memory_limit;
    let mut memory_used: usize = 0;

    let mut in_memory_tracks: std::collections::HashMap<usize, Vec<MidiEvent>> =
        std::collections::HashMap::new();
    let mut summaries: Vec<TrackSummary> = Vec::with_capacity(track_count);

    for (track_idx, event_iter) in event_iters.into_iter().enumerate() {
        let parsed = parse_track_events_from_iter(track_idx, event_iter)?;
        let event_count = parsed.events.len() as u64;
        let _should_try_memory = parsed.high_vel_count > 0;

        let (summary, keep_in_memory) = decide_track_storage(
            &parsed,
            track_idx,
            event_count,
            &mut memory_used,
            initial_memory_limit,
        );

        summaries.push(summary);

        if keep_in_memory {
            in_memory_tracks.insert(track_idx, parsed.events.clone());
        }

        disk_tx
            .send((track_idx, parsed.events))
            .map_err(|e| format!("发送磁盘写入任务失败: {e}"))?;

        if let Some(cb) = progress_callback {
            let progress = PROGRESS_START
                + PROGRESS_MAIN_RATIO * ((track_idx + 1) as f64 / track_count as f64);
            cb(progress);
        }
    }

    // 释放内存
    drop(file_data);

    // 关闭 channel，等待后台写入完成
    drop(disk_tx);
    disk_writer
        .join()
        .map_err(|_| "磁盘写入线程 panic".to_string())?
        .map_err(|e| format!("磁盘写入失败: {e}"))?;

    if let Some(cb) = progress_callback {
        cb(1.0);
    }

    let in_mem_count = in_memory_tracks.len();
    let on_disk_count = track_count - in_mem_count;

    tracing::info!(
        "MidiMemoryManager: {} 音轨在内存 ({} MB), {} 音轨在磁盘, division={}",
        in_mem_count,
        memory_used / 1024 / 1024,
        on_disk_count,
        division,
    );

    Ok(LoadedMidiData {
        disk_cache,
        summaries,
        in_memory_tracks,
        memory_used,
        loaded_memory_limit,
    })
}

/// 提取 MIDI 时间分割值
fn extract_division(header: &midly::Header) -> u16 {
    match header.timing {
        midly::Timing::Metrical(ticks) => ticks.as_int(),
        _ => DEFAULT_PPQN,
    }
}

/// 决定音轨存储位置（内存或磁盘）
fn decide_track_storage(
    parsed: &ParsedTrack,
    track_idx: usize,
    event_count: u64,
    memory_used: &mut usize,
    initial_memory_limit: usize,
) -> (TrackSummary, bool) {
    let should_try_memory = parsed.high_vel_count > 0;

    if !should_try_memory {
        let summary = create_track_summary(
            track_idx,
            event_count,
            parsed.note_count,
            parsed.high_vel_count,
            parsed.max_tick,
            0,
            false,
        );
        return (summary, false);
    }

    let track_size = estimate_events_size(&parsed.events);

    if *memory_used + track_size <= initial_memory_limit {
        *memory_used += track_size;
        let summary = create_track_summary(
            track_idx,
            event_count,
            parsed.note_count,
            parsed.high_vel_count,
            parsed.max_tick,
            track_size,
            true,
        );
        (summary, true)
    } else {
        let summary = create_track_summary(
            track_idx,
            event_count,
            parsed.note_count,
            parsed.high_vel_count,
            parsed.max_tick,
            0,
            false,
        );
        (summary, false)
    }
}
