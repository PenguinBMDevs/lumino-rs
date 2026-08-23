use std::path::PathBuf;
use std::sync::Arc;

use lumino_diagnostics::memory_monitor::MemoryMonitor;

use crate::LmpjData;
use crate::ParsedMidi;
use crate::info::MidiInfo;
use crate::{LoaderError, LoaderResult, MidiDocument};

use super::types::ProgressCallback;

fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> LoaderResult<T> {
    let decoded = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| LoaderError::Compression(format!("解压失败: {e}")))?;
    bincode::deserialize(&decoded).map_err(LoaderError::from)
}

/// 从磁盘加载 MIDI / LMPJ 文件并解析为 `ParsedMidi`。
///
/// 自动识别文件类型：压缩包会被解压后查找内部 MIDI 文件，`.lmpj` 为
/// Lumino 工程文件，其余按标准 MIDI 解析。
///
/// # 参数
/// * `path` — 待加载的文件路径
/// * `progress` — 可选的进度回调 `(阶段说明, 进度 0.0–1.0)`，`None` 表示不回调
///
/// # 错误
/// 当文件不存在、无法解压或解析失败时返回 [`LoaderError`]。
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

    // ── 文件格式验证 ──
    // 检查扩展名是否为支持的格式（MIDI / LMPJ 或压缩包）
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| LoaderError::FileFormat("无法获取文件扩展名".to_string()))?;

    // 有效的 MIDI 文件扩展名
    let is_midi_ext = matches!(extension.as_str(), "mid" | "midi" | "lmpj");

    // 如果是压缩包，调用方应提前处理。这里只检查常规 MIDI/LMPJ 文件。
    // 如果是压缩包但走到了这里（未提前处理），给出明确错误。
    if !is_midi_ext {
        // 检查是否为已知的压缩包格式
        use crate::archive::is_archive;
        if is_archive(&path) {
            return Err(LoaderError::FileFormat(
                "文件是压缩包格式，请先解压后再加载。".to_string(),
            ));
        }
        return Err(LoaderError::FileFormat(
            "本文件不支持加载，请检查文件格式！支持的格式：.mid / .midi / .lmpj".to_string(),
        ));
    }

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
                        let total_notes: u64 = (0..doc.track_count())
                            .map(|t| doc.track_note_count(t as u16))
                            .sum();
                        parsed.info.total_notes = total_notes;
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
        // 解析临时数据（zstd 解压 + midly 中间态）已全部 drop，
        // 主动回收 mimalloc 空闲页，避免峰值残留抬高 RSS
        lumino_diagnostics::memtrace::purge_free_pages();
        return Ok(parsed);
    }

    // ── 统一加载路径：单次读取 + from_notes_bytes ──
    // 避免 scan_midi_file 与 from_notes_file 两次完整 IO
    {
        let read_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
        cb(&format!("正在读取文件... (内存: {read_rss} MB)"), 0.05);
    }
    let file_bytes = tokio::fs::read(&path).await.map_err(|e| {
        let err = LoaderError::Io(e);
        cb(&err.to_string(), 1.0);
        err
    })?;

    let file_size_mb = file_bytes.len() / (1024 * 1024);
    cb(
        &format!("文件读取完成 ({file_size_mb} MB)，开始解析..."),
        0.10,
    );

    // 桥接进度回调：将 from_notes_bytes 的 f64 进度映射到 ProgressCallback
    let cache_progress: Option<Arc<dyn Fn(f64) + Send + Sync>> = progress.map(|p| {
        let p = Arc::clone(p);
        Arc::new(move |val: f64| {
            let inner_rss = MemoryMonitor::global().current_rss() / (1024 * 1024);
            p(
                &format!("正在提取音符并构建缓存... (内存: {inner_rss} MB)"),
                0.10 + val * 0.85,
            );
        }) as Arc<dyn Fn(f64) + Send + Sync>
    });

    let (document, division, total_notes) = tokio::task::spawn_blocking(move || {
        let p_ref = cache_progress.as_ref().map(|a| a.as_ref() as &dyn Fn(f64));
        crate::MidiDocument::from_notes_bytes(&file_bytes, p_ref)
    })
    .await
    .map_err(|e| LoaderError::Other(format!("加载线程 panic: {e}")))?
    .map_err(|e| LoaderError::MidiParse(format!("解析 MIDI 数据失败: {e}")))?;

    let info = MidiInfo {
        path: path.clone(),
        track_count: document.track_count,
        total_notes,
        duration_ticks: document.total_ticks(),
        division,
        parse_progress: Some(100.0),
    };

    tracing::info!(
        "MIDI 加载完成: {} ticks, {} 音轨, {} 音符, division={}",
        info.duration_ticks,
        info.track_count,
        info.total_notes,
        info.division
    );

    // 解析临时数据（原始字节 + midly 中间态 + 分块构建）已全部 drop，
    // 主动回收 mimalloc 空闲页，避免加载峰值残留抬高 RSS
    // （2000W 音符场景实测峰值残留 ~180MB）
    lumino_diagnostics::memtrace::purge_free_pages();

    let rss_mb = MemoryMonitor::global().current_rss() / (1024 * 1024);
    cb(
        &format!("MIDI 加载完成 ({total_notes} 音符, 内存: {rss_mb} MB)"),
        1.0,
    );

    // 不再缓存原始 MIDI 字节。356MB 的黑乐谱原始数据仅在解析时暂存，
    // 解析为 MidiDocument 后立即释放。音频导出等场景从 info.division 读取 PPQN。
    Ok(ParsedMidi {
        info,
        document: Some(std::sync::Arc::new(document)),
        // 常规 MIDI 文件无工程 stats，历史累计时间为 0
        accumulated_editing_secs: 0.0,
        // 常规 MIDI 文件无作者/版权信息
        author: String::new(),
        copyright: String::new(),
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

    // 解析临时数据已全部 drop，主动回收 mimalloc 空闲页（同 load_parsed_midi）
    lumino_diagnostics::memtrace::purge_free_pages();

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
        // 字节加载路径无工程 stats，历史累计时间为 0
        accumulated_editing_secs: 0.0,
        // 字节加载路径无作者/版权信息
        author: String::new(),
        copyright: String::new(),
    })
}

/// 从 MIDI 字节构建 MidiDocument（同步函数，供 LMPJ 加载和 from_bytes 共用）
///
/// 不再返回 midi_data——原始字节解析后立即释放。
pub(super) fn build_document_from_midi_bytes(
    midi_bytes: &[u8],
    _track_count: u16,
) -> LoaderResult<MidiDocument> {
    let (doc, _, _) = MidiDocument::from_notes_bytes(midi_bytes, None)?;
    Ok(doc)
}
