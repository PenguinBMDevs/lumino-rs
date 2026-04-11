use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use tokio::sync::mpsc as tokio_mpsc;

use crate::{DmsInfo, MidiInfo, ParsedDms, ParsedMidi};
use crate::{TrackBasedCache, cache_utils::compute_cache_key, event_cache::TrackEvents};

/// 进度回调函数类型：(消息, 进度 0.0~1.0)
/// 使用 Arc 包装以便跨线程共享和克隆
pub type ProgressCallback = Arc<dyn Fn(&str, f64) + Send + Sync>;

/// 从 tokio unbounded sender 创建进度回调（供应用层使用）
pub fn progress_from_sender(
    sender: tokio_mpsc::UnboundedSender<(String, f64)>,
) -> ProgressCallback {
    Arc::new(move |message: &str, progress: f64| {
        let _ = sender.send((message.to_string(), progress.clamp(0.0, 1.0)));
    })
}

/// 无操作的进度回调（静默模式）
pub fn silent_progress() -> ProgressCallback {
    Arc::new(|_, _| {})
}

fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> crate::Result<T> {
    let decoded = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| crate::CoreError::Compression(format!("解压失败: {e}")))?;
    bincode::deserialize(&decoded).map_err(crate::CoreError::from)
}

pub fn load_midi_info_with_progress(
    path: PathBuf,
    progress_callback: Option<&dyn Fn(f64)>,
) -> crate::Result<MidiInfo> {
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

/// 准备缓存目录
fn prepare_cache_dir(path: &Path, cache: Option<&TrackBasedCache>) -> PathBuf {
    if let Some(cc) = cache {
        let cache_dir = cc.cache_dir().to_path_buf();

        if let Ok(metadata) = std::fs::metadata(path)
            && let Ok(modified) = metadata.modified()
        {
            let key = compute_cache_key(path, modified);
            let cache_subdir = cache_dir.join(format!("{:016x}", key));
            if !cache_subdir.exists() {
                let _ = std::fs::create_dir_all(&cache_subdir);
            }
        }
        cache_dir
    } else {
        PathBuf::new()
    }
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

pub async fn load_parsed_midi(
    path: PathBuf,
    progress: Option<&ProgressCallback>,
) -> crate::Result<ParsedMidi> {
    let cb = |msg: &str, val: f64| {
        if let Some(p) = progress {
            p(msg, val);
        }
    };

    cb("正在准备加载文件", 0.0);

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| crate::CoreError::FileFormat("无法获取文件扩展名".to_string()))?;

    if extension == "lmpj" {
        cb("正在加载 Lumino 工程文件", 0.1);
        let data = tokio::fs::read(&path).await.map_err(|e| {
            let err = crate::CoreError::Io(e);
            cb(&err.to_string(), 1.0);
            err
        })?;
        cb("解析 Lumino 工程文件", 0.5);

        let parsed = tokio::task::spawn_blocking(move || {
            let lmpj_data: crate::LmpjData = decode_lmpj(&data)
                .map_err(|e| crate::CoreError::FileFormat(format!("解析 LMPJ 失败: {e}")))?;

            tracing::info!(
                "LMPJ 解析成功: info.path={:?}, midi_data.len={:?}",
                lmpj_data.info.path,
                lmpj_data.midi_data.as_ref().map(|d| d.len())
            );

            Ok::<ParsedMidi, crate::CoreError>(lmpj_data.to_parsed_midi())
        })
        .await
        .map_err(|e| {
            let err = crate::CoreError::Other(format!("解析 LMPJ 失败: {e}"));
            cb(&err.to_string(), 1.0);
            err
        })?
        .inspect_err(|e| {
            cb(&e.to_string(), 1.0);
        })?;

        cb("Lumino 工程文件加载完成", 1.0);
        return Ok(parsed);
    }

    cb("正在加载并缓存 MIDI 事件...", 0.1);

    let cache = TrackBasedCache::new_in_program_dir().map_err(|e| {
        let err = crate::CoreError::Cache(format!("创建缓存失败: {e}"));
        cb(&err.to_string(), 1.0);
        err
    })?;

    let cache_dir = cache.cache_dir().to_path_buf();
    let path_clone = path.clone();

    // 为 spawn_blocking 闭包捕获进度回调的克隆
    let progress_clone = progress.cloned();
    let (info, memory_manager) = tokio::task::spawn_blocking(move || {
        puffin::profile_scope!("load_midi_blocking");

        let pcb = progress_clone.as_ref();
        let cb = |msg: &str, val: f64| {
            if let Some(p) = pcb {
                p(msg, val);
            }
        };

        let manager = crate::midi::managed_midi::MidiMemoryManager::load(
            &path_clone,
            &cache_dir,
            Some(&|progress| {
                cb(
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
                .ok_or_else(|| crate::CoreError::MidiParse("无法计算最大 tick".to_string()))?,
            division: {
                let stream = crate::midi::MidiEventStream::from_path(manager.source_path())?;
                stream.division()
            },
            parse_progress: Some(100.0),
        };

        Ok::<_, crate::CoreError>((info, manager))
    })
    .await
    .map_err(|e| {
        let err = crate::CoreError::Other(format!("解析 MIDI 失败: {e}"));
        cb(&err.to_string(), 1.0);
        err
    })?
    .inspect_err(|e| {
        cb(&e.to_string(), 1.0);
    })?;

    cb("MIDI 加载完成", 1.0);

    let mgr_arc = std::sync::Arc::new(std::sync::Mutex::new(memory_manager));

    Ok(ParsedMidi {
        info,
        midi_data: None,
        memory_manager: Some(mgr_arc),
    })
}

pub async fn load_dms(
    path: PathBuf,
    progress: Option<&ProgressCallback>,
) -> crate::Result<ParsedDms> {
    let cb = |msg: &str, val: f64| {
        if let Some(p) = progress {
            p(msg, val);
        }
    };

    cb("正在准备加载 Domino 工程文件", 0.0);
    cb("正在打开 Domino 工程文件", 0.05);

    tracing::info!("[DMS加载] 开始加载文件: {:?}", path);

    let path_clone = path.clone();
    let progress_clone = progress.cloned();
    let (scan_result, lightweight_data) = tokio::task::spawn_blocking(move || {
        puffin::profile_scope!("load_dms_blocking");

        let pcb = progress_clone.as_ref();
        let scan_cb = |msg: &str, val: f64| {
            if let Some(p) = pcb {
                p(msg, val);
            }
        };

        // 首先流式扫描获取元数据
        tracing::info!("[DMS加载] 步骤1: 打开文件");
        let file = std::fs::File::open(&path_clone).map_err(crate::CoreError::Io)?;
        let mut reader = std::io::BufReader::new(file);
        tracing::info!("[DMS加载] 步骤2: 开始流式扫描");
        let scan_result = lumino_dms::scan_dms_streaming_with_progress(&mut reader, |progress| {
            scan_cb("正在解析 Domino 工程文件", 0.1 + progress * 0.4);
        })
        .map_err(|e| crate::CoreError::FileFormat(format!("扫描 DMS 失败: {e}")))?;
        tracing::info!("[DMS加载] 步骤3: 扫描完成, 轨道数={}, 音符数={}", 
            scan_result.track_count, scan_result.total_notes);

        // 然后加载完整数据
        scan_cb("正在加载完整数据", 0.5);
        tracing::info!("[DMS加载] 步骤4: 读取完整文件数据");
        let bytes = std::fs::read(&path_clone).map_err(crate::CoreError::Io)?;
        tracing::info!("[DMS加载] 步骤5: 文件大小 {} 字节", bytes.len());
        
        tracing::info!("[DMS加载] 步骤6: 解压 DMS 数据");
        let lightweight_data = lumino_dms::read_dms_lightweight(&bytes)
            .map_err(|e| crate::CoreError::FileFormat(format!("读取 DMS 数据失败: {e}")))?;
        tracing::info!("[DMS加载] 步骤7: 解压完成, 解压后大小 {} 字节", lightweight_data.len());

        scan_cb("数据加载完成", 0.9);

        Ok::<_, crate::CoreError>((scan_result, lightweight_data))
    })
    .await
    .map_err(|e| {
        let err = crate::CoreError::Other(format!("加载 DMS 失败: {e}"));
        tracing::error!("[DMS加载] 任务执行失败: {}", e);
        cb(&err.to_string(), 1.0);
        err
    })?
    .map_err(|e| {
        let err = crate::CoreError::Compression(format!("处理 DMS 失败: {e}"));
        tracing::error!("[DMS加载] 数据处理失败: {}", e);
        cb(&err.to_string(), 1.0);
        err
    })?;

    cb("Domino 工程文件加载完成", 1.0);
    tracing::info!("[DMS加载] 加载完成成功");

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

    Ok(ParsedDms {
        info,
        data: Some(lightweight_data),
    })
}

pub async fn save_dms_to_ldms(
    parsed: &ParsedDms,
    path: PathBuf,
    progress: Option<&ProgressCallback>,
) -> crate::Result<()> {
    let cb = |msg: &str, val: f64| {
        if let Some(p) = progress {
            p(msg, val);
        }
    };

    cb("准备保存 LDMS", 0.0);
    cb("保存 LDMS", 0.1);

    let data_for_save = ParsedDms {
        info: parsed.info.clone(),
        data: None,
    };

    let data = bincode::serialize(&data_for_save).map_err(|e| {
        let err = crate::CoreError::from(e);
        cb(&err.to_string(), 1.0);
        err
    })?;

    cb("压缩 LDMS", 0.4);
    let compressed = tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(data);
        zstd::stream::encode_all(cursor, 3)
    })
    .await
    .map_err(|e| {
        let err = crate::CoreError::Other(format!("压缩 LDMS 失败: {e}"));
        cb(&err.to_string(), 1.0);
        err
    })?
    .map_err(|e| {
        let err = crate::CoreError::Compression(format!("压缩 LDMS 失败: {e}"));
        cb(&err.to_string(), 1.0);
        err
    })?;

    tokio::fs::write(&path, compressed).await.map_err(|e| {
        let err = crate::CoreError::Io(e);
        cb(&err.to_string(), 1.0);
        err
    })?;

    cb("LDMS 保存完成", 1.0);
    Ok(())
}
