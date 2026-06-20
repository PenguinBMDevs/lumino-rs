use std::path::PathBuf;
use std::sync::Arc;

use lumino_memory_monitor::MemoryMonitor;
use midly::loader::{MidiScanResult, scan_midi_file};

use crate::LmpjData;
use crate::ParsedMidi;
use crate::document::MidiDocument;
use crate::error::{LoaderError, LoaderResult};
use crate::info::MidiInfo;

use super::types::ProgressCallback;

fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> LoaderResult<T> {
    let decoded = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| LoaderError::Compression(format!("解压失败: {e}")))?;
    bincode::deserialize(&decoded).map_err(LoaderError::from)
}

fn scan_midi_info(path: &std::path::Path) -> LoaderResult<MidiScanResult> {
    scan_midi_file(path).map_err(|e| LoaderError::MidiParse(format!("扫描失败: {e}")))
}

pub async fn load_parsed_midi(
    path: PathBuf,
    progress: Option<&ProgressCallback>,
) -> LoaderResult<ParsedMidi> {
    // 大分配前检查内存，防止 OOM 导致系统无响应
    MemoryMonitor::global().check();

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
        .ok_or_else(|| LoaderError::FileFormat("无法获取文件扩展名".to_string()))?;

    if extension == "lmpj" {
        cb("正在加载 Lumino 工程文件", 0.1);
        let data = tokio::fs::read(&path).await.map_err(|e| {
            let err = LoaderError::Io(e);
            cb(&err.to_string(), 1.0);
            err
        })?;
        cb("解析 Lumino 工程文件", 0.5);

        let parsed = tokio::task::spawn_blocking(move || {
            let mut lmpj_data: LmpjData = decode_lmpj(&data)
                .map_err(|e| LoaderError::FileFormat(format!("解析 LMPJ 失败: {e}")))?;

            tracing::info!(
                "LMPJ 解析成功: info.path={:?}, midi_data 存在={}",
                lmpj_data.info.path,
                lmpj_data.midi_data.is_some()
            );

            // LMPJ 加载时直接构建 MidiDocument，避免中间态 midi_data 常驻内存
            let track_count = lmpj_data.info.track_count;
            let midi_bytes = lmpj_data.midi_data.take();
            let mut parsed = lmpj_data.to_parsed_midi();
            if let Some(midi_bytes) = midi_bytes {
                match build_document_from_midi_bytes(&midi_bytes, track_count) {
                    Ok(doc) => {
                        let total_events = doc.all_events().len() as u64;
                        parsed.info.total_notes = total_events / 2;
                        parsed.info.duration_ticks = doc.total_ticks();
                        parsed.document = Some(std::sync::Arc::new(doc));
                    }
                    Err(e) => {
                        tracing::warn!("LMPJ 内嵌 MIDI 构建文档失败: {e}，将回退到重新加载");
                    }
                }
            }

            Ok::<ParsedMidi, LoaderError>(parsed)
        })
        .await
        .map_err(|e| {
            let err = LoaderError::Other(format!("解析 LMPJ 失败: {e}"));
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
    .map_err(|e| LoaderError::Other(format!("扫描 MIDI 失败: {e}")))??;

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
        crate::MidiDocument::from_notes_file(&path_for_cache, p_ref)
    })
    .await
    .map_err(|e| LoaderError::Other(format!("加载线程 panic: {e}")))?
    .map_err(|e| LoaderError::MidiParse(format!("解析 MIDI 数据失败: {e}")))?;

    let info = MidiInfo {
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

    // 不再缓存原始 MIDI 字节。356MB 的黑乐谱原始数据仅在解析时暂存，
    // 解析为 MidiDocument 后立即释放。音频导出等场景从 info.division 读取 PPQN。
    Ok(ParsedMidi {
        info,
        document: Some(std::sync::Arc::new(document)),
    })
}

/// 从 MIDI 字节数据直接加载 ParsedMidi（无需文件路径）
///
/// 适用于已从其他格式（如 DMS）转换得到 MIDI 字节的场景，
/// 避免写入临时文件再读取的 IO 开销。
///
/// **不再缓存原始字节**——解析后立即释放，避免黑乐谱 356MB 冗余内存。
pub async fn load_parsed_midi_from_bytes(
    midi_bytes: Vec<u8>,
    track_count: u16,
    total_ticks: u32,
    progress: Option<&ProgressCallback>,
) -> LoaderResult<ParsedMidi> {
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
        build_document_from_midi_bytes(&midi_bytes, track_count)
    })
    .await
    .map_err(|e| LoaderError::Other(format!("加载线程 panic: {e}")))??;

    {
        let final_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
        cb(&format!("MIDI 加载完成 (内存: {final_rss} MB)"), 1.0);
    }

    let info = MidiInfo {
        path: PathBuf::new(),
        track_count,
        total_notes: 0,
        duration_ticks: total_ticks,
        division: 960,
        parse_progress: Some(100.0),
    };

    Ok(ParsedMidi {
        info,
        document: Some(std::sync::Arc::new(document)),
    })
}

/// 从 MIDI 字节构建 MidiDocument（同步函数，供 LMPJ 加载和 from_bytes 共用）
///
/// 不再返回 midi_data——原始字节解析后立即释放。
pub(super) fn build_document_from_midi_bytes(
    midi_bytes: &[u8],
    _track_count: u16,
) -> LoaderResult<MidiDocument> {
    let (notes, tempo_changes, control_events) =
        midly::loader::extract_notes_and_control_events_from_bytes(midi_bytes)
            .map_err(|e| LoaderError::MidiParse(format!("提取音符失败: {e}")))?;
    let track_names = crate::document::scan::scan_track_names(midi_bytes);
    MidiDocument::build_from_extracted_notes(
        notes,
        tempo_changes,
        control_events,
        track_names,
        None,
    )
    .map_err(|e| LoaderError::MidiParse(format!("构建文档失败: {e}")))
}
