use std::path::PathBuf;
use crate::MidiInfo;

/// 加载MIDI文件信息（带进度回调）
///
/// `progress_callback` 接收 0.0..=100.0 的百分比值。
pub fn load_midi_info_with_progress(
    path: PathBuf,
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<MidiInfo, String> {
    // NOTE: This function performs blocking I/O on the current thread.
    // The caller is expected to call this from a background thread (as the Runner does).

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
        result.track_count, result.note_count, elapsed_ms
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