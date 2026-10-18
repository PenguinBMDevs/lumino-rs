//! MIDI 文件元信息。
//!
//! 元信息由**加载链路**（`loader::parsed_midi`）在解析时填好（音符数/时长/PPQN），
//! 列表展示与工程序列化直接消费它。
//!
//! 2026-10-17（审计 §八）：删除了 `from_path` / `from_path_with_progress`
//! ——它们是走 `midly::loader::scan_midi_file` 的**第二套轻量扫描**实现，
//! 全仓 0 调用者（唯一构造来源是加载链路本身）。元信息不需要第二条扫描链。

use std::path::PathBuf;

use lumino_core::trf;

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

impl std::fmt::Display for MidiInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            trf!(
                "MIDI文件: {path} | 音轨数: {track_count} | 音符事件数: {total_notes} | 时长: {duration_ticks} ticks | 分辨率: {division}",
                path = self.path.display(),
                track_count = self.track_count,
                total_notes = self.total_notes,
                duration_ticks = self.duration_ticks,
                division = self.division
            )
        )
    }
}
