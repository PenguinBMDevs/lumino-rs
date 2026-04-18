use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use crate::MidiInfo;
use crate::TrackBasedCache;

use super::cache::{CompressionTask, compression_worker, prepare_cache_dir};

pub fn load_midi_info_with_progress(
    path: PathBuf,
    progress_callback: Option<&dyn Fn(f64)>,
) -> crate::Result<MidiInfo> {
    load_midi_info_with_cache(path, None, progress_callback)
}

/// 处理单个音轨
fn process_single_track(
    path: &Path,
    track_idx: usize,
    cache_dir: &Path,
    tx: &mpsc::Sender<CompressionTask>,
    has_cache: bool,
) -> crate::Result<(u64, u32, u64)> {
    let mut stream = crate::midi::MidiEventStream::from_path(path)?;
    let track_events = stream
        .read_track_events(track_idx)
        .map_err(|e| crate::CoreError::MidiParse(format!("读取音轨失败: {e}")))?;
    drop(stream);

    let event_count = track_events.len() as u64;
    let track_max = track_events
        .iter()
        .map(|e| e.tick())
        .max()
        .ok_or_else(|| crate::CoreError::MidiParse("音轨事件为空".to_string()))?;

    let note_count = track_events
        .iter()
        .filter(|e| {
            if let crate::midi::MidiEvent::NoteOn { velocity, .. } = e {
                *velocity > 0
            } else {
                false
            }
        })
        .count() as u64;

    if has_cache {
        let _ = tx.send(CompressionTask {
            track_idx,
            track_events,
            cache_dir: cache_dir.to_path_buf(),
            source_path: path.to_path_buf(),
        });
    }

    Ok((note_count, track_max, event_count))
}

/// 完成缓存并记录日志
fn finalize_midi_loading(
    cache: Option<&TrackBasedCache>,
    path: &Path,
    track_count: u16,
    division: u16,
    track_event_counts: &[u64],
    track_max_ticks: &[u32],
    total_notes: u64,
    bench_start: std::time::Instant,
) {
    if let Some(cc) = cache
        && let Err(e) = cc.finalize_cache(
            path,
            track_count,
            division,
            track_event_counts,
            track_max_ticks,
        )
    {
        tracing::warn!("完成缓存索引失败: {}", e);
    }

    let elapsed_ms = bench_start.elapsed().as_millis();
    tracing::info!(
        "MidiEventStream scan success: tracks={}, notes={}, time_ms={}",
        track_count,
        total_notes,
        elapsed_ms
    );
}

pub fn load_midi_info_with_cache(
    path: PathBuf,
    cache: Option<&TrackBasedCache>,
    progress_callback: Option<&dyn Fn(f64)>,
) -> crate::Result<MidiInfo> {
    if let Some(cb) = progress_callback {
        cb(0.0);
    }

    let bench_start = std::time::Instant::now();

    let stream = crate::midi::MidiEventStream::from_path(&path)?;
    let track_count = stream.track_count() as u16;
    let division = stream.division();
    drop(stream);

    let cache_dir = prepare_cache_dir(&path, cache);
    let has_cache = cache.is_some();

    let (tx, rx) = mpsc::channel::<CompressionTask>();
    let compression_thread = thread::spawn(move || {
        compression_worker(rx);
    });

    let mut total_notes = 0u64;
    let mut max_tick = 0u32;
    let mut track_event_counts = Vec::with_capacity(track_count as usize);
    let mut track_max_ticks = Vec::with_capacity(track_count as usize);

    for track_idx in 0..track_count as usize {
        let (note_count, track_max, event_count) =
            process_single_track(&path, track_idx, &cache_dir, &tx, has_cache)?;

        track_event_counts.push(event_count);
        track_max_ticks.push(track_max);

        if track_max > max_tick {
            max_tick = track_max;
        }

        total_notes += note_count;

        if let Some(cb) = progress_callback {
            let progress = ((track_idx + 1) as f64 / track_count as f64) * 0.99;
            cb(progress);
        }
    }

    drop(tx);

    if let Err(e) = compression_thread.join() {
        tracing::warn!("压缩线程异常结束: {:?}", e);
    }

    finalize_midi_loading(
        cache,
        &path,
        track_count,
        division,
        &track_event_counts,
        &track_max_ticks,
        total_notes,
        bench_start,
    );

    if let Some(cb) = progress_callback {
        cb(1.0);
    }

    Ok(MidiInfo {
        path,
        track_count,
        total_notes,
        duration_ticks: max_tick,
        division,
        parse_progress: Some(100.0),
    })
}
