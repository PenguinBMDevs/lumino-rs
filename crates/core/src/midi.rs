pub mod loader;

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
        loader::load_midi_info_with_progress(path, progress_callback)
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