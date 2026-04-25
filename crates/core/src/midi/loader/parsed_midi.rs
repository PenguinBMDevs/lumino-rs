use std::path::PathBuf;

use crate::ParsedMidi;
use crate::TrackBasedCache;

use super::types::ProgressCallback;

fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> crate::Result<T> {
    let decoded = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| crate::CoreError::Compression(format!("解压失败: {e}")))?;
    bincode::deserialize(&decoded).map_err(crate::CoreError::from)
}

/// 快速读取 MIDI 文件头，获取 division 和 track_count
fn quick_scan_midi_header(path: &std::path::Path) -> crate::Result<(u16, u16)> {
    let file = std::fs::File::open(path).map_err(crate::CoreError::Io)?;
    let data = unsafe { memmap2::Mmap::map(&file).map_err(crate::CoreError::Io)? };
    let (header, track_iters) =
        midly::parse(&data).map_err(|e| crate::CoreError::MidiParse(format!("解析失败: {e}")))?;
    let track_count = track_iters.count() as u16;
    let division = match header.timing {
        midly::Timing::Metrical(t) => t.as_int(),
        _ => 480,
    };
    Ok((division, track_count))
}

pub async fn load_parsed_midi(
    path: PathBuf,
    progress: Option<&ProgressCallback>,
    skip_memory_manager: bool,
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

    // ── 内存优化模式：跳过 MidiMemoryManager，只创建流式缓存 ──
    if skip_memory_manager {
        cb("正在扫描文件信息...", 0.05);
        let (division, track_count) = tokio::task::spawn_blocking({
            let path = path.clone();
            move || quick_scan_midi_header(&path)
        })
        .await
        .map_err(|e| crate::CoreError::Other(format!("扫描 MIDI 头失败: {e}")))??;

        cb("正在初始化流式缓存 (内存优化模式)...", 0.1);

        let path_for_cache = path.clone();
        let cache = tokio::task::spawn_blocking(move || {
            lumino_cache::MidiCache::load(&path_for_cache, None)
        })
        .await
        .map_err(|e| crate::CoreError::Other(format!("缓存线程 panic: {e}")))?
        .map_err(|e| crate::CoreError::Cache(format!("创建缓存失败: {e}")))?;

        let info = crate::MidiInfo {
            path: path.clone(),
            track_count,
            total_notes: 0, // 大文件不统计精确音符数，避免二次扫描
            duration_ticks: cache.index.total_ticks,
            division,
            parse_progress: Some(100.0),
        };

        tracing::info!(
            "内存优化模式加载完成: {} ticks, {} 音轨, division={}",
            info.duration_ticks,
            info.track_count,
            info.division
        );

        cb("MIDI 加载完成", 1.0);

        return Ok(ParsedMidi {
            info,
            midi_data: None,
            memory_manager: None,
            cache: Some(std::sync::Arc::new(cache)),
        });
    }

    // ── 标准模式：加载 MidiMemoryManager + 可选缓存 ──
    cb("正在加载并缓存 MIDI 事件...", 0.1);

    let cache = TrackBasedCache::new_in_program_dir().map_err(|e| {
        let err = crate::CoreError::Cache(format!("创建缓存失败: {e}"));
        cb(&err.to_string(), 1.0);
        err
    })?;

    let cache_dir = cache.cache_dir().to_path_buf();
    let path_clone = path.clone();
    let path_for_cache = path.clone();

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

    cb("正在初始化播放缓存...", 0.95);

    // 初始化 lumino-cache 分层缓存（播放用，支持 tick 级随机跳转）
    let cache = match lumino_cache::MidiCache::load(&path_for_cache, None) {
        Ok(c) => {
            tracing::info!(
                "缓存初始化成功: {} ticks, {} 音轨",
                c.index.total_ticks,
                c.index.track_count
            );
            Some(std::sync::Arc::new(c))
        }
        Err(e) => {
            tracing::warn!("缓存初始化失败（不影响编辑）: {e}");
            None
        }
    };

    cb("MIDI 加载完成", 1.0);

    let mgr_arc = std::sync::Arc::new(std::sync::Mutex::new(memory_manager));

    Ok(ParsedMidi {
        info,
        midi_data: None,
        memory_manager: Some(mgr_arc),
        cache,
    })
}
