use std::path::PathBuf;
use std::sync::Arc;

use midly::loader::{MidiScanResult, scan_midi_file};

use crate::ParsedMidi;

use super::types::ProgressCallback;

fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> crate::Result<T> {
    let decoded = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| crate::CoreError::Compression(format!("解压失败: {e}")))?;
    bincode::deserialize(&decoded).map_err(crate::CoreError::from)
}

fn scan_midi_info(path: &std::path::Path) -> crate::Result<MidiScanResult> {
    scan_midi_file(path).map_err(|e| crate::CoreError::MidiParse(format!("扫描失败: {e}")))
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

    // ── 统一加载路径：scan_midi_file + from_notes_file ──
    // 单次解析，峰值内存 ~1.3GB（vs 原标准模式 5-8GB）
    cb("正在扫描文件信息...", 0.05);
    let scan_result = tokio::task::spawn_blocking({
        let path = path.clone();
        move || scan_midi_info(&path)
    })
    .await
    .map_err(|e| crate::CoreError::Other(format!("扫描 MIDI 失败: {e}")))??;

    cb("正在提取音符并构建缓存...", 0.1);

    // 桥接进度回调：将 from_notes_file 的 f64 进度映射到 ProgressCallback
    let cache_progress: Option<Arc<dyn Fn(f64) + Send + Sync>> = progress.map(|p| {
        let p = Arc::clone(p);
        let f: Arc<dyn Fn(f64) + Send + Sync> = Arc::new(move |val: f64| {
            p("正在提取音符并构建缓存...", 0.1 + val * 0.85);
        });
        f
    });

    let path_for_cache = path.clone();
    let cache = tokio::task::spawn_blocking(move || {
        let p_ref = cache_progress.as_ref().map(|a| a.as_ref() as &dyn Fn(f64));
        lumino_cache::MidiCache::from_notes_file(&path_for_cache, p_ref)
    })
    .await
    .map_err(|e| crate::CoreError::Other(format!("缓存线程 panic: {e}")))?
    .map_err(|e| crate::CoreError::Cache(format!("创建缓存失败: {e}")))?;

    let info = crate::MidiInfo {
        path: path.clone(),
        track_count: scan_result.track_count,
        total_notes: scan_result.note_count,
        duration_ticks: scan_result.max_tick,
        division: scan_result.division,
        parse_progress: Some(100.0),
    };

    tracing::info!(
        "MIDI 加载完成: {} ticks, {} 音轨, {} 音符, division={}",
        info.duration_ticks,
        info.track_count,
        info.total_notes,
        info.division
    );

    cb("MIDI 加载完成", 1.0);

    Ok(ParsedMidi {
        info,
        midi_data: None,
        cache: Some(Arc::new(cache)),
    })
}
