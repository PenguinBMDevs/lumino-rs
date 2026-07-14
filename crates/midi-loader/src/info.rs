use std::path::PathBuf;

use midly::loader::scan_midi_file;

use crate::LoaderError;

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
    /// 使用 midly-fork 的轻量扫描快速获取 MIDI 文件信息
    ///
    /// 顺序读取，峰值内存 < 10MB，不解析事件细节
    pub fn from_path(path: PathBuf) -> crate::LoaderResult<Self> {
        Self::from_path_with_progress(path, None)
    }

    /// 带进度回调的 MIDI 文件扫描
    pub fn from_path_with_progress(
        path: PathBuf,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> crate::LoaderResult<Self> {
        if let Some(cb) = progress_callback {
            cb(0.0);
        }

        let scan_result = scan_midi_file(&path)
            .map_err(|e| LoaderError::MidiParse(format!("扫描 MIDI 文件失败: {e}")))?;

        if let Some(cb) = progress_callback {
            cb(1.0);
        }

        Ok(MidiInfo {
            path,
            track_count: scan_result.track_count,
            total_notes: scan_result.note_count,
            duration_ticks: scan_result.max_tick,
            division: scan_result.division,
            parse_progress: Some(100.0),
        })
    }
}

impl std::fmt::Display for MidiInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MIDI文件: {} | 音轨数: {} | 音符事件数: {} | 时长: {} ticks | 分辨率: {}",
            self.path.display(),
            self.track_count,
            self.total_notes,
            self.duration_ticks,
            self.division,
        )
    }
}
