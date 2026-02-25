use crate::ParsedMidi;
use crate::{DmsInfo, ParsedDms, TrackBasedCache, event_cache::TrackEvents};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use tokio::sync::mpsc as tokio_mpsc;

static PROGRESS_SENDER: OnceLock<tokio_mpsc::UnboundedSender<(String, f64)>> = OnceLock::new();

pub fn set_progress_sender(sender: tokio_mpsc::UnboundedSender<(String, f64)>) {
    let _ = PROGRESS_SENDER.set(sender);
}

fn send_progress(message: &str, progress: f64) {
    if let Some(sender) = PROGRESS_SENDER.get() {
        let _ = sender.send((message.to_string(), progress.clamp(0.0, 1.0)));
    }
}

pub fn send_progress_message(message: &str, progress: f64) {
    send_progress(message, progress);
}

fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let decoded = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| format!("解压失败: {e}"))?;
    bincode::deserialize(&decoded).map_err(|e| format!("反序列化失败: {e}"))
}

use crate::MidiInfo;

pub fn load_midi_info_with_progress(
    path: PathBuf,
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<MidiInfo, String> {
    load_midi_info_with_cache(path, None, progress_callback)
}

struct CompressionTask {
    track_idx: usize,
    track_events: Vec<crate::midi::MidiEvent>,
    cache_dir: PathBuf,
    source_path: PathBuf,
}

fn compression_worker(rx: mpsc::Receiver<CompressionTask>) {
    while let Ok(task) = rx.recv() {
        let track_events = TrackEvents {
            track_index: task.track_idx,
            events: task.track_events,
        };

        let result = (|| -> std::io::Result<()> {
            let metadata = std::fs::metadata(&task.source_path)?;
            let modified = metadata.modified()?;
            let key = compute_cache_key(&task.source_path, modified);

            let serialized = bincode::serialize(&track_events).map_err(std::io::Error::other)?;
            let compressed =
                zstd::stream::encode_all(&mut &serialized[..], 3).map_err(std::io::Error::other)?;

            let track_path = task
                .cache_dir
                .join(format!("{:016x}", key))
                .join(format!("track_{:04x}.lmt", task.track_idx));
            std::fs::write(&track_path, &compressed)?;

            Ok(())
        })();

        if let Err(e) = result {
            tracing::warn!("缓存音轨 {} 失败: {}", task.track_idx, e);
        }
    }
}

fn compute_cache_key(path: &std::path::Path, file_modified: std::time::SystemTime) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    file_modified.hash(&mut hasher);
    hasher.finish()
}

pub fn load_midi_info_with_cache(
    path: PathBuf,
    cache: Option<&TrackBasedCache>,
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<MidiInfo, String> {
    if let Some(cb) = progress_callback {
        cb(0.0);
    }

    let bench_start = std::time::Instant::now();

    let stream = crate::midi::MidiEventStream::from_path(&path)?;
    let track_count = stream.track_count() as u16;
    let division = stream.division();
    drop(stream);

    let cache_dir = if let Some(cc) = cache {
        let cache_dir = cc.cache_dir().to_path_buf();

        if let Ok(metadata) = std::fs::metadata(&path)
            && let Ok(modified) = metadata.modified()
        {
            let key = compute_cache_key(&path, modified);
            let cache_subdir = cache_dir.join(format!("{:016x}", key));
            if !cache_subdir.exists() {
                let _ = std::fs::create_dir_all(&cache_subdir);
            }
        }
        cache_dir
    } else {
        PathBuf::new()
    };

    let (tx, rx) = mpsc::channel::<CompressionTask>();
    let compression_thread = thread::spawn(move || {
        compression_worker(rx);
    });

    let mut total_notes = 0u64;
    let mut max_tick = 0u32;
    let mut track_event_counts = Vec::with_capacity(track_count as usize);
    let mut track_max_ticks = Vec::with_capacity(track_count as usize);

    for track_idx in 0..track_count as usize {
        let mut stream = crate::midi::MidiEventStream::from_path(&path)?;
        let track_events = stream
            .read_track_events(track_idx)
            .map_err(|e| format!("读取音轨失败: {e}"))?;
        drop(stream);

        let event_count = track_events.len() as u64;
        let track_max = track_events.iter().map(|e| e.tick()).max().unwrap_or(0);

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

        track_event_counts.push(event_count);
        track_max_ticks.push(track_max);

        if track_max > max_tick {
            max_tick = track_max;
        }

        total_notes += note_count;

        if let Some(_cc) = cache {
            let _ = tx.send(CompressionTask {
                track_idx,
                track_events,
                cache_dir: cache_dir.clone(),
                source_path: path.clone(),
            });
        }

        if let Some(cb) = progress_callback {
            let progress = ((track_idx + 1) as f64 / track_count as f64) * 0.99;
            cb(progress);
        }
    }

    drop(tx);

    let _ = compression_thread.join();

    if let Some(cc) = cache
        && let Err(e) = cc.finalize_cache(
            &path,
            track_count,
            division,
            &track_event_counts,
            &track_max_ticks,
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

pub async fn load_parsed_midi(path: PathBuf) -> Result<ParsedMidi, String> {
    send_progress("正在准备加载文件", 0.0);

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if extension == "lmpj" {
        send_progress("正在加载 Lumino 工程文件", 0.1);
        let data = tokio::fs::read(&path).await.map_err(|e| {
            let err = format!("读取 LMPJ 失败: {e}");
            send_progress(&err, 1.0);
            err
        })?;
        send_progress("解析 Lumino 工程文件", 0.5);

        let parsed = tokio::task::spawn_blocking(move || {
            decode_lmpj::<ParsedMidi>(&data).map_err(|e| format!("解析 LMPJ 失败: {e}"))
        })
        .await
        .map_err(|e| {
            let err = format!("解析 LMPJ 失败: {e}");
            send_progress(&err, 1.0);
            err
        })?
        .inspect_err(|e| {
            send_progress(e, 1.0);
        })?;

        send_progress("Lumino 工程文件加载完成", 1.0);
        return Ok(parsed);
    }

    send_progress("正在加载并缓存 MIDI 事件...", 0.1);

    let cache = TrackBasedCache::new_in_program_dir().map_err(|e| format!("创建缓存失败: {e}"))?;

    let cache_dir = cache.cache_dir().to_path_buf();
    let path_clone = path.clone();
    let (info, memory_manager) = tokio::task::spawn_blocking(move || {
        // 使用新的 MidiMemoryManager 加载
        let manager = crate::midi::managed_midi::MidiMemoryManager::load(
            &path_clone,
            &cache_dir,
            Some(&|progress| {
                send_progress(
                    &format!("加载中 ({}%)...", (progress * 100.0) as u32),
                    progress,
                );
            }),
            None,
        )?;

        let stats = manager.stats();
        let info = crate::MidiInfo {
            path: path_clone,
            track_count: stats.track_count as u16,
            total_notes: stats.total_notes,
            duration_ticks: manager
                .all_summaries()
                .iter()
                .map(|s| s.max_tick)
                .max()
                .unwrap_or(0),
            division: {
                let stream = crate::midi::MidiEventStream::from_path(manager.source_path())?;
                stream.division()
            },
            parse_progress: Some(100.0),
        };

        Ok::<_, String>((info, manager))
    })
    .await
    .map_err(|e| {
        let err = format!("解析 MIDI 失败: {e}");
        send_progress(&err, 1.0);
        err
    })?
    .inspect_err(|e| {
        send_progress(e, 1.0);
    })?;

    send_progress("MIDI 加载完成", 1.0);

    let mgr_arc = std::sync::Arc::new(std::sync::Mutex::new(memory_manager));

    Ok(ParsedMidi {
        info,
        midi_data: None,
        memory_manager: Some(mgr_arc),
    })
}

pub async fn load_dms(path: PathBuf) -> Result<ParsedDms, String> {
    send_progress("正在准备加载 Domino 工程文件", 0.0);
    send_progress("正在打开 Domino 工程文件", 0.05);

    let path_clone = path.clone();
    let scan_result = tokio::task::spawn_blocking(move || {
        let file =
            std::fs::File::open(&path_clone).map_err(|e| format!("打开 DMS 文件失败: {e}"))?;
        let mut reader = std::io::BufReader::new(file);
        lumino_dms::scan_dms_streaming_with_progress(&mut reader, |progress| {
            send_progress("正在解析 Domino 工程文件", 0.1 + progress * 0.7);
        })
        .map_err(|e| format!("扫描 DMS 失败: {e}"))
    })
    .await
    .map_err(|e| {
        let err = format!("扫描 DMS 失败: {e}");
        send_progress(&err, 1.0);
        err
    })?
    .map_err(|e| {
        let err = format!("压缩 LDMS 失败: {e}");
        send_progress(&err, 1.0);
        err
    })?;

    send_progress("Domino 工程文件加载完成", 1.0);

    let info = DmsInfo {
        path,
        song_name: scan_result.song_name,
        copyright: scan_result.copyright,
        comment: scan_result.comment,
        ppqn: scan_result.ppqn,
        track_count: scan_result.track_count,
        total_notes: scan_result.total_notes,
        working_time_sec: scan_result.working_time_sec,
    };

    Ok(ParsedDms { info, data: None })
}

pub async fn save_dms_to_ldms(parsed: &ParsedDms, path: PathBuf) -> Result<(), String> {
    send_progress("准备保存 LDMS", 0.0);
    send_progress("保存 LDMS", 0.1);

    let data_for_save = ParsedDms {
        info: parsed.info.clone(),
        data: None,
    };

    let data = bincode::serialize(&data_for_save).map_err(|e| {
        let err = format!("序列化 LDMS 失败: {e}");
        send_progress(&err, 1.0);
        err
    })?;

    send_progress("压缩 LDMS", 0.4);
    let compressed = tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(data);
        zstd::stream::encode_all(cursor, 3)
    })
    .await
    .map_err(|e| {
        let err = format!("压缩 LDMS 失败: {e}");
        send_progress(&err, 1.0);
        err
    })?
    .map_err(|e| {
        let err = format!("压缩 LDMS 失败: {e}");
        send_progress(&err, 1.0);
        err
    })?;

    tokio::fs::write(&path, compressed).await.map_err(|e| {
        let err = format!("写入 LDMS 失败: {e}");
        send_progress(&err, 1.0);
        err
    })?;

    send_progress("LDMS 保存完成", 1.0);
    Ok(())
}
