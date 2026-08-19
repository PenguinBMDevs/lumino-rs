//! MIDI 文件元信息。
//!
//! 通过轻量级扫描获取文件的路径、音轨数、音符数等信息，用于列表展示。

use std::path::PathBuf;

use midly::loader::scan_midi_file;

use crate::LoaderError;

/// MIDI文件元信息（用于列表显示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MidiInfo {
    /// 文件路径。
    pub path: PathBuf,
    /// 音轨数量。
    pub track_count: u16,
    /// 总音符数。
    pub total_notes: u64,
    /// 总时长（tick）。
    pub duration_ticks: u32,
    /// 时基（每四分音符的 tick 数，PPQN）。
    pub division: u16,
    /// 扫描进度（0.0–1.0，`None` 表示未知/未开始）。
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
