use crate::ParsedMidi;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::sync::mpsc;

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
///
/// `progress_callback` 接收 0.0..=100.0 的百分比值。
pub fn load_midi_info_with_progress(
    path: PathBuf,
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<MidiInfo, String> {
    // 注意：此函数在当前线程上执行阻塞 I/O
    // 调用者预期在后台线程调用此函数（如 Runner 所做的那样）

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
        send_progress("读取 LMPJ", 0.05);
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| format!("读取 LMPJ 失败: {e}"))?;
        send_progress("解压 LMPJ", 0.2);
        let decoded = tokio::task::spawn_blocking(move || {
            let cursor = std::io::Cursor::new(data);
            zstd::stream::decode_all(cursor)
        })
        .await
        .map_err(|e| format!("解压 LMPJ 失败: {e}"))
        .and_then(|r| r.map_err(|e| format!("解压 LMPJ 失败: {e}")))?;

        send_progress("解析 LMPJ", 0.6);
        let parsed: ParsedMidi =
            bincode::deserialize(&decoded).map_err(|e| format!("解析 LMPJ 失败: {e}"))?;

        send_progress("完成", 1.0);
        return Ok(parsed);
    }

    send_progress("解析 MIDI", 0.5);
    let path_clone = path.clone();
    let info = tokio::task::spawn_blocking(move || {
        load_midi_info_with_progress(path_clone, None)
    })
    .await
    .map_err(|e| format!("解析 MIDI 失败: {e}"))
    .and_then(|r| r.map_err(|e| format!("解析 MIDI 失败: {e}")))?;

    send_progress("完成", 1.0);
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

// ============================================================================
// DMS 加载功能
// ============================================================================

use crate::{DmsInfo, ParsedDms};
use lumino_dms::DmsNodeType;

/// 节点头大小
const HEADER_SIZE: usize = 6; // 2 (type_id) + 4 (data_length)

/// DMS 元数据提取结果
type DmsMetadata = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<u32>,
    Option<u64>,
);

/// 加载 DMS 文件（轻量级，低内存占用）
pub async fn load_dms(path: PathBuf) -> Result<ParsedDms, String> {
    send_progress("扫描 DMS", 0.1);

    let path_clone = path.clone();
    let scan_result = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&path_clone)
            .map_err(|e| format!("打开 DMS 文件失败: {e}"))?;
        let mut reader = std::io::BufReader::new(file);
        lumino_dms::scan_dms_streaming(&mut reader).map_err(|e| format!("扫描 DMS 失败: {e}"))
    })
    .await
    .map_err(|e| format!("扫描 DMS 失败: {e}"))?
    .map_err(|e| format!("扫描 DMS 失败: {e}"))?;

    send_progress("完成", 1.0);

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
        data: None,
    })
}

/// 从轻量级数据中提取元数据
fn extract_dms_metadata_lightweight(
    lightweight: &lumino_dms::DmsLightweightData,
    top_level: &[(u16, usize, usize)],
) -> DmsMetadata {
    let mut song_name = None;
    let mut copyright = None;
    let mut comment = None;
    let mut ppqn = None;
    let mut working_time_sec = None;

    let data = lightweight.data.as_ref();

    for (type_id, data_length, data_start) in top_level {
        let node_type = DmsNodeType::from_parts(*type_id, 0, None);
        if data_start + data_length > data.len() {
            continue;
        }
        let node_data = &data[*data_start..*data_start + *data_length];

        match node_type {
            t if t == DmsNodeType::SONG_NAME => {
                song_name = decode_gb18030(node_data);
            }
            t if t == DmsNodeType::SONG_COPYRIGHT => {
                copyright = decode_gb18030(node_data);
            }
            t if t == DmsNodeType::SONG_COMMENT => {
                comment = decode_gb18030(node_data);
            }
            t if t == DmsNodeType::SONG_PPQN => {
                ppqn = decode_u32_le(node_data);
            }
            t if t == DmsNodeType::WORKING_TIME_SEC => {
                working_time_sec = decode_u64_le(node_data);
            }
            _ => {}
        }
    }

    (song_name, copyright, comment, ppqn, working_time_sec)
}

/// 统计 DMS 音符数量（轻量级扫描）
fn count_dms_notes_lightweight(
    lightweight: &lumino_dms::DmsLightweightData,
    top_level: &[(u16, usize, usize)],
) -> (u64, usize) {
    let mut total_notes = 0u64;
    let mut track_count = 0usize;

    let data = lightweight.data.as_ref();

    for (type_id, data_length, data_start) in top_level {
        let node_type = DmsNodeType::from_parts(*type_id, 0, None);
        if node_type == DmsNodeType::TRACK {
            track_count += 1;
            // 统计轨道内的音符事件
            if *data_start + *data_length <= data.len() {
                total_notes +=
                    count_notes_in_track_data(&data[*data_start..*data_start + *data_length]);
            }
        }
    }

    (total_notes, track_count)
}

/// 统计轨道数据中的音符数量
fn count_notes_in_track_data(track_data: &[u8]) -> u64 {
    let mut count = 0u64;
    let mut offset = 0usize;

    // 原始 type_id（文件中存储的值，不含父节点信息）
    // NOTE_EVENT 的原始 type_id 是 2001
    const NOTE_EVENT_RAW_TYPE_ID: u16 = 2001;

    while offset + HEADER_SIZE <= track_data.len() {
        let type_id = u16::from_le_bytes([track_data[offset], track_data[offset + 1]]);
        let data_length = u32::from_le_bytes([
            track_data[offset + 2],
            track_data[offset + 3],
            track_data[offset + 4],
            track_data[offset + 5],
        ]) as usize;

        if type_id == NOTE_EVENT_RAW_TYPE_ID {
            count += 1;
        }

        offset += HEADER_SIZE + data_length;
    }

    count
}

/// GB18030 解码
fn decode_gb18030(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    let (decoded, _, had_errors) = encoding_rs::GB18030.decode(data);
    if had_errors {
        None
    } else {
        let s = decoded.to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

/// 小端 u32 解码
fn decode_u32_le(data: &[u8]) -> Option<u32> {
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// 小端 u64 解码
fn decode_u64_le(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        return None;
    }
    Some(u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
}
