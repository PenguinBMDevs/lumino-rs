use crate::ParsedMidi;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::sync::mpsc;
use crate::{DmsInfo, ParsedDms};

static PROGRESS_SENDER: OnceLock<mpsc::UnboundedSender<(String, f64)>> = OnceLock::new();

pub fn set_progress_sender(sender: mpsc::UnboundedSender<(String, f64)>) {
    let _ = PROGRESS_SENDER.set(sender);
}

fn send_progress(message: &str, progress: f64) {
    if let Some(sender) = PROGRESS_SENDER.get() {
        let _ = sender.send((message.to_string(), progress.clamp(0.0, 1.0)));
    }
}
use crate::MidiInfo;

/// 加载MIDI文件信息（带进度回调）
/// progress_callback 接收 0.0..=100.0 的百分比值。
pub fn load_midi_info_with_progress(
    path: PathBuf,
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<MidiInfo, String> {
    // 注意：此函数在当前线程上执行阻塞 I/O
    // 使用和 benchmark 完全一致的顺序扫描逻辑 (scan_midi_file)
    if let Some(cb) = progress_callback {
        cb(0.0);
    }

    let bench_start = std::time::Instant::now();
    let result = midly::scan_midi_file(std::path::Path::new(&path))
        .map_err(|e| format!("扫描 MIDI 文件失败: {:?}", e))?;

    let elapsed_ms = bench_start.elapsed().as_millis();
    tracing::info!(
        "scan_midi_file: tracks={}, notes={}, time_ms={}",
        result.track_count,
        result.note_count,
        elapsed_ms
    );

    if let Some(cb) = progress_callback {
        cb(100.0);
    }

    Ok(MidiInfo {
        path,
        track_count: result.track_count,
        total_notes: result.note_count as u64,
        duration_ticks: result.max_tick,
        division: result.division,
        parse_progress: Some(100.0),
    })
}

pub async fn load_parsed_midi(path: PathBuf) -> Result<ParsedMidi, String> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if extension == "lmpj" {
        send_progress("正在加载 Lumino 工程文件", 0.05);
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| format!("读取 LMPJ 失败: {e}"))?;
        send_progress("解压 Lumino 工程文件", 0.2);
        let decoded = tokio::task::spawn_blocking(move || {
            let cursor = std::io::Cursor::new(data);
            zstd::stream::decode_all(cursor)
        })
        .await
        .map_err(|e| format!("解压 LMPJ 失败: {e}"))
        .and_then(|r| r.map_err(|e| format!("解压 LMPJ 失败: {e}")))?;

        send_progress("解析 Lumino 工程文件", 0.6);
        let parsed: ParsedMidi =
            bincode::deserialize(&decoded).map_err(|e| format!("解析 LMPJ 失败: {e}"))?;

        send_progress("Lumino 工程文件加载完成", 1.0);
        return Ok(parsed);
    }

    send_progress("正在加载 MIDI 文件", 0.5);
    let path_clone = path.clone();
    let info = tokio::task::spawn_blocking(move || load_midi_info_with_progress(path_clone, None))
        .await
        .map_err(|e| format!("解析 MIDI 失败: {e}"))
        .and_then(|r| r.map_err(|e| format!("解析 MIDI 失败: {e}")))?;

    send_progress("MIDI 文件加载完成", 1.0);
    Ok(ParsedMidi {
        info,
        midi_data: None,
    })
}

pub async fn save_to_lmpj(parsed: &ParsedMidi, path: PathBuf) -> Result<(), String> {
    send_progress("保存 LMPJ", 0.1);

    // 构造用于序列化的数据，对于 LMPJ 格式，我们不存储原始 midi_data
    // 因为 LMPJ 本身已经是压缩格式，只存储元数据即可awa
    let data_for_save = ParsedMidi {
        info: parsed.info.clone(),
        midi_data: None, // LMPJ 不需要存储原始 MIDI 数据
    };

    let data = bincode::serialize(&data_for_save).map_err(|e| format!("序列化 LMPJ 失败: {e}"))?;

    send_progress("压缩 LMPJ", 0.4);
    let compressed = tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(data);
        zstd::stream::encode_all(cursor, 3)
    })
    .await
    .map_err(|e| format!("压缩 LMPJ 失败: {e}"))
    .and_then(|r| r.map_err(|e| format!("压缩 LMPJ 失败: {e}")))?;

    tokio::fs::write(&path, compressed)
        .await
        .map_err(|e| format!("写入 LMPJ 失败: {e}"))?;

    send_progress("完成", 1.0);
    Ok(())
}

/// 加载 DMS 文件（轻量级，低内存占用）
pub async fn load_dms(path: PathBuf) -> Result<ParsedDms, String> {
    send_progress("正在打开 Domino 工程文件", 0.05);

    let path_clone = path.clone();
    let scan_result = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&path_clone)
            .map_err(|e| format!("打开 DMS 文件失败: {e}"))?;
        let mut reader = std::io::BufReader::new(file);
        lumino_dms::scan_dms_streaming_with_progress(&mut reader, |progress| {
            // 将解压进度映射到 0.1 - 0.8 的范围
            send_progress("正在解析 Domino 工程文件", 0.1 + progress * 0.7);
        }).map_err(|e| format!("扫描 DMS 失败: {e}"))
    })
    .await
    .map_err(|e| format!("扫描 DMS 失败: {e}"))?
    .map_err(|e| format!("扫描 DMS 失败: {e}"))?;

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


/// 将 DMS 解析结果保存为 LDMS 格式（Lumino DMS Project）
pub async fn save_dms_to_ldms(parsed: &ParsedDms, path: PathBuf) -> Result<(), String> {
    send_progress("保存 LDMS", 0.1);

    // 构造用于序列化的数据
    let data_for_save = ParsedDms {
        info: parsed.info.clone(),
        data: None, // LDMS 不需要存储原始 DMS 数据，只存储元数据
    };

    let data = bincode::serialize(&data_for_save).map_err(|e| format!("序列化 LDMS 失败: {e}"))?;

    send_progress("压缩 LDMS", 0.4);
    let compressed = tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(data);
        zstd::stream::encode_all(cursor, 3)
    })
    .await
    .map_err(|e| format!("压缩 LDMS 失败: {e}"))
    .and_then(|r| r.map_err(|e| format!("压缩 LDMS 失败: {e}")))?;

    tokio::fs::write(&path, compressed)
        .await
        .map_err(|e| format!("写入 LDMS 失败: {e}"))?;

    send_progress("LDMS 保存完成", 1.0);
    Ok(())
}
