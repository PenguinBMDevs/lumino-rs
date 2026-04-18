use std::path::PathBuf;

use crate::ParsedMidi;
use crate::TrackBasedCache;

use super::types::ProgressCallback;

fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> crate::Result<T> {
    let decoded = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| crate::CoreError::Compression(format!("解压失败: {e}")))?;
    bincode::deserialize(&decoded).map_err(crate::CoreError::from)
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
