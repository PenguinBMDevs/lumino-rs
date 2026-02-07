use std::path::PathBuf;

/// MIDI文件信息
#[derive(Debug, Clone)]
pub struct MidiInfo {
    pub path: PathBuf,
    pub track_count: u16,
    pub total_notes: u64,
    pub duration_ticks: u32,
    pub division: u16,
    pub parse_progress: Option<f64>,
}

impl MidiInfo {
    /// 解析MIDI文件
    pub fn from_path(path: PathBuf) -> Result<Self, String> {
        Self::from_path_with_progress(path, None)
    }

    /// 解析MIDI文件（带进度回调）
    ///
    /// `progress_callback` 接收 0.0..=100.0 的百分比值。
    pub fn from_path_with_progress(
        path: PathBuf,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self, String> {
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
}

impl std::fmt::Display for MidiInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MIDI文件: {}\n音轨数: {}\n音符事件数: {}\n时长: {} ticks\n分辨率: {}",
            self.path.display(),
            self.track_count,
            self.total_notes,
            self.duration_ticks,
            self.division,
        )
    }
}