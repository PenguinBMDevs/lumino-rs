use std::path::PathBuf;

/// 解析后的MIDI文件信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedMidi {
    pub path: PathBuf,
    pub tracks: Vec<(u64, String)>, // (事件数, 音轨名)
    pub duration_ticks: u32,
    pub division: u16,
    pub format: u16,
}

impl ParsedMidi {
    pub fn total_events(&self) -> u64 {
        self.tracks.iter().map(|(count, _)| count).sum()
    }
}

/// MIDI文件元信息（用于列表显示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MidiInfo {
    pub path: PathBuf,
    pub track_count: u16,
    pub total_notes: u64,
    pub duration_ticks: u32,
    pub division: u16,
    pub parse_progress: Option<f64>,
}

impl MidiInfo {
    pub fn from_path(path: PathBuf) -> Result<Self, String> {
        Self::from_path_with_progress(path, None)
    }

    pub fn from_path_with_progress(
        path: PathBuf,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self, String> {
        super::loader::load_midi_info_with_progress(path, progress_callback)
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
