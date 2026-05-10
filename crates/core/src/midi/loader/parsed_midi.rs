use std::path::PathBuf;
use std::sync::Arc;

use midly::loader::{MidiScanResult, scan_midi_file};

use crate::ParsedMidi;
use crate::memory_monitor::MemoryMonitor;
use crate::midi::document::MidiDocument;

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
    // 大分配前检查内存，防止 OOM 导致系统无响应
    crate::memory_monitor::MemoryMonitor::global().check();

    let cb = |msg: &str, val: f64| {
        if let Some(p) = progress {
            p(msg, val);
        }
    };

    {
        let initial_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
        cb(&format!("正在准备加载文件 (内存: {initial_rss} MB)"), 0.0);
    }

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
    {
        let scan_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
        cb(&format!("正在扫描文件信息... (内存: {scan_rss} MB)"), 0.05);
    }
    let scan_result = tokio::task::spawn_blocking({
        let path = path.clone();
        move || scan_midi_info(&path)
    })
    .await
    .map_err(|e| crate::CoreError::Other(format!("扫描 MIDI 失败: {e}")))??;

    let note_count = scan_result.note_count;
    let rss_mb = MemoryMonitor::global().current_rss() / (1024 * 1024);
    cb(
        &format!("正在提取音符并构建缓存... ({note_count} 音符, 内存: {rss_mb} MB)"),
        0.1,
    );

    // 桥接进度回调：将 from_notes_file 的 f64 进度映射到 ProgressCallback
    let cache_progress: Option<Arc<dyn Fn(f64) + Send + Sync>> = progress.map(|p| {
        let p = Arc::clone(p);
        let f: Arc<dyn Fn(f64) + Send + Sync> = Arc::new(move |val: f64| {
            let inner_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
            p(
                &format!("正在提取音符并构建缓存... ({note_count} 音符, 内存: {inner_rss} MB)"),
                0.1 + val * 0.85,
            );
        });
        f
    });

    let path_for_cache = path.clone();
    let document = tokio::task::spawn_blocking(move || {
        let p_ref = cache_progress.as_ref().map(|a| a.as_ref() as &dyn Fn(f64));
        crate::midi::MidiDocument::from_notes_file(&path_for_cache, p_ref)
    })
    .await
    .map_err(|e| crate::CoreError::Other(format!("加载线程 panic: {e}")))?
    .map_err(|e| crate::CoreError::MidiParse(format!("解析 MIDI 数据失败: {e}")))?;

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

    let rss_mb = MemoryMonitor::global().current_rss() / (1024 * 1024);
    cb(
        &format!("MIDI 加载完成 ({note_count} 音符, 内存: {rss_mb} MB)"),
        1.0,
    );

    Ok(ParsedMidi {
        info,
        midi_data: None,
        document: Some(std::sync::Arc::new(document)),
    })
}

/// 从 MIDI 字节数据直接加载 ParsedMidi（无需文件路径）
///
/// 适用于已从其他格式（如 DMS）转换得到 MIDI 字节的场景，
/// 避免写入临时文件再读取的 IO 开销。
pub async fn load_parsed_midi_from_bytes(
    midi_bytes: Vec<u8>,
    track_count: u16,
    total_ticks: u32,
    progress: Option<&ProgressCallback>,
) -> crate::Result<ParsedMidi> {
    let cb = |msg: &str, val: f64| {
        if let Some(p) = progress {
            p(msg, val);
        }
    };

    {
        let parse_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
        cb(
            &format!("正在解析 MIDI 数据... (内存: {parse_rss} MB)"),
            0.1,
        );
    }

    let document = tokio::task::spawn_blocking(move || {
        let (notes, tempo_changes, control_events) =
            midly::loader::extract_notes_and_control_events_from_bytes(&midi_bytes)
                .map_err(|e| crate::CoreError::MidiParse(format!("提取音符失败: {e}")))?;
        let track_names = crate::midi::document::scan_track_names(&midi_bytes);
        MidiDocument::build_from_extracted_notes(
            notes,
            tempo_changes,
            control_events,
            track_names,
            None,
        )
        .map_err(|e| crate::CoreError::MidiParse(format!("构建文档失败: {e}")))
    })
    .await
    .map_err(|e| crate::CoreError::Other(format!("加载线程 panic: {e}")))??;

    {
        let final_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
        cb(&format!("MIDI 加载完成 (内存: {final_rss} MB)"), 1.0);
    }

    let info = crate::MidiInfo {
        path: PathBuf::new(),
        track_count,
        total_notes: 0,
        duration_ticks: total_ticks,
        division: 960,
        parse_progress: Some(100.0),
    };

    Ok(ParsedMidi {
        info,
        midi_data: None,
        document: Some(std::sync::Arc::new(document)),
    })
}
